// [[B1]] upstreamIsSpec: replays tools/oracle/scenarios.cjs and compares with upstream's answers
// in fixtures/oracle.json (regenerate: node tools/oracle/run.mjs).
mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::{Yield, block_on};
use ruggine::classic::{ClassicNode, Connection, Input, Node, Output, Socket};
use ruggine::future::{LocalBoxFuture, join_all, ready};
use ruggine::{
    BaseSchemes, ConnectionBase, GetSchemes, KeyMap, NodeBase, NodeEditor, Root, keymap,
};
use ruggine_engine::{
    ControlFlow, ControlFlowNodeSetup, DataflowEngine, DataflowNode, Error, Inputs, Outputs,
};
use serde_json::{Value, json};

type Log = Rc<RefCell<Vec<String>>>;

struct TestNode {
    node: Node,
    log: Log,
    fail: bool,
    returns: Option<KeyMap<Value>>,
}

impl NodeBase for TestNode {
    fn id(&self) -> &str {
        &self.node.id
    }
}

impl ClassicNode for TestNode {
    type Socket = Socket;
    fn classic(&self) -> &Node {
        &self.node
    }
}

#[derive(Debug)]
struct Fail(String);
impl std::fmt::Display for Fail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Fail {}

impl DataflowNode<Value> for TestNode {
    fn data(&self, _: Inputs<Value>) -> LocalBoxFuture<'_, Result<Outputs<Value>, Error>> {
        self.log.borrow_mut().push(self.node.id.clone());
        if self.fail {
            return ready(Err(Error::node(Fail(format!("fail {}", self.node.id)))));
        }
        ready(Ok(self
            .returns
            .clone()
            .unwrap_or_else(|| keymap! { "o" => json!(self.node.id) })))
    }
}

type Schemes = GetSchemes<TestNode, Connection>;

struct Graph {
    editor: NodeEditor<Schemes>,
    engine: DataflowEngine<Schemes, Value>,
    log: Log,
}

impl Graph {
    fn new() -> Self {
        Self {
            editor: NodeEditor::new(),
            engine: DataflowEngine::new(),
            log: Rc::default(),
        }
    }

    fn attach(&self) {
        self.editor.use_scope(&self.engine).unwrap();
    }

    fn mk(&self, id: &str, inputs: &[&str], fail: bool) -> Rc<TestNode> {
        let mut node = Node::new(id);
        node.id = id.into();
        for i in inputs {
            let input = Input::new(Socket::new("s")).with_multiple_connections(true);
            node.add_input(*i, input).unwrap();
        }
        node.add_output("o", Output::new(Socket::new("s"))).unwrap();
        Rc::new(TestNode {
            node,
            log: self.log.clone(),
            fail,
            returns: None,
        })
    }

    fn add(&self, nodes: &[&Rc<TestNode>]) {
        for n in nodes {
            block_on(self.editor.add_node((*n).clone())).unwrap();
        }
    }

    fn link(&self, a: &TestNode, b: &TestNode, input: &str) {
        let c = Connection::new(a, "o", b, input).unwrap();
        block_on(self.editor.add_connection(Rc::new(c))).unwrap();
    }

    fn log(&self) -> Vec<String> {
        self.log.borrow().clone()
    }
}

fn chain(fail_a: bool, fail_c: bool) -> Graph {
    let g = Graph::new();
    g.attach();
    let a = g.mk("A", &[], fail_a);
    let b = g.mk("B", &["i"], false);
    let c = g.mk("C", &[], fail_c);
    let d = g.mk("D", &["b", "c"], false);
    g.add(&[&a, &b, &c, &d]);
    g.link(&a, &b, "i");
    g.link(&b, &d, "b");
    g.link(&c, &d, "c");
    g
}

fn message<T>(result: Result<T, Error>) -> String {
    match result {
        Ok(_) => "ok".into(),
        Err(e) => e.to_string(),
    }
}

fn to_json<V: serde::Serialize>(map: &KeyMap<V>) -> Value {
    Value::Object(map.iter().map(|(k, v)| (k.to_owned(), json!(v))).collect())
}

fn expected(name: &str) -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/oracle.json"
    ))
    .expect("fixtures/oracle.json");
    let all: Value = serde_json::from_str(&text).unwrap();
    all.get(name)
        .unwrap_or_else(|| panic!("no upstream answer for {name}"))
        .clone()
}

macro_rules! oracle {
    ($name:ident, $body:block) => {
        #[test]
        fn $name() {
            let got: Value = $body;
            assert_eq!(
                got,
                expected(stringify!($name)),
                "{} differs from upstream",
                stringify!($name)
            );
        }
    };
}

oracle!(fetch_order, {
    let g = chain(false, false);
    let out = block_on(g.engine.fetch("D")).unwrap();
    json!({ "log": g.log(), "out": to_json(&out) })
});

oracle!(fetch_inputs, {
    let g = chain(false, false);
    let inputs = block_on(g.engine.fetch_inputs("D")).unwrap();
    json!({ "log": g.log(), "inputs": to_json(&inputs) })
});

oracle!(cached, {
    let g = chain(false, false);
    block_on(g.engine.fetch("D")).unwrap();
    block_on(g.engine.fetch("D")).unwrap();
    json!({ "log": g.log() })
});

oracle!(reset_before_await, {
    let g = chain(false, false);
    let pending = g.engine.fetch("D");
    g.engine.reset(Some("D")).unwrap();
    json!({ "error": message(block_on(pending)) })
});

oracle!(two_failures, {
    let g = chain(true, true);
    let error = message(block_on(g.engine.fetch("D")));
    json!({ "error": error, "log": g.log() })
});

oracle!(reset_empty, {
    let g = chain(false, false);
    block_on(g.engine.fetch("D")).unwrap();
    g.engine.reset(Some("")).unwrap();
    g.log.borrow_mut().clear();
    block_on(g.engine.fetch("D")).unwrap();
    json!({ "log": g.log() })
});

oracle!(reset_middle, {
    let g = chain(false, false);
    block_on(g.engine.fetch("D")).unwrap();
    g.engine.reset(Some("B")).unwrap();
    g.log.borrow_mut().clear();
    block_on(g.engine.fetch("D")).unwrap();
    json!({ "log": g.log() })
});

oracle!(diamond, {
    let g = Graph::new();
    g.attach();
    let a = g.mk("A", &[], false);
    let b = g.mk("B", &["i"], false);
    let c = g.mk("C", &["i"], false);
    let d = g.mk("D", &["b", "c"], false);
    g.add(&[&a, &b, &c, &d]);
    g.link(&a, &b, "i");
    g.link(&a, &c, "i");
    g.link(&b, &d, "b");
    g.link(&c, &d, "c");
    block_on(g.engine.fetch("D")).unwrap();
    json!({ "log": g.log() })
});

oracle!(fan_in, {
    let g = Graph::new();
    g.attach();
    let a = g.mk("A", &[], false);
    let b = g.mk("B", &[], false);
    let c = g.mk("C", &[], false);
    let d = g.mk("D", &["i"], false);
    g.add(&[&a, &b, &c, &d]);
    for n in [&b, &a, &c] {
        g.link(n, &d, "i");
    }
    let inputs = block_on(g.engine.fetch_inputs("D")).unwrap();
    json!({ "log": g.log(), "inputs": to_json(&inputs) })
});

oracle!(missing_outputs, {
    let g = Graph::new();
    g.attach();
    let mut node = Node::new("X");
    node.id = "X".into();
    node.add_output("o", Output::new(Socket::new("s"))).unwrap();
    let x = Rc::new(TestNode {
        node,
        log: g.log.clone(),
        fail: false,
        returns: Some(keymap! { "other" => json!(1) }),
    });
    g.add(&[&x]);
    json!({ "error": message(block_on(g.engine.fetch("X"))) })
});

oracle!(reset_stops_at_missing_setup, {
    let g = Graph::new();
    let a = g.mk("A", &[], false);
    let b = g.mk("B", &["i"], false);
    let c = g.mk("C", &["i"], false);
    g.add(&[&b]);
    g.attach();
    g.add(&[&a, &c]);
    g.link(&a, &b, "i");
    g.link(&a, &c, "i");
    block_on(g.engine.fetch("C")).unwrap();
    let error = message(g.engine.reset(Some("A")));
    g.log.borrow_mut().clear();
    block_on(g.engine.fetch("C")).unwrap();
    block_on(g.engine.fetch("A")).unwrap();
    json!({ "error": error, "log": g.log() })
});

struct Plain(String);
impl NodeBase for Plain {
    fn id(&self) -> &str {
        &self.0
    }
}
struct Link(String, String, String);
impl ConnectionBase for Link {
    fn id(&self) -> &str {
        &self.0
    }
    fn source(&self) -> &str {
        &self.1
    }
    fn target(&self) -> &str {
        &self.2
    }
}

fn plain(id: &str) -> Rc<dyn NodeBase> {
    Rc::new(Plain(id.into()))
}

fn link(id: &str, s: &str, t: &str) -> Rc<dyn ConnectionBase> {
    Rc::new(Link(id.into(), s.into(), t.into()))
}

oracle!(editor_events, {
    let editor = NodeEditor::<BaseSchemes>::new();
    let log: Log = Rc::default();
    let l = log.clone();
    editor.add_pipe(move |ctx| {
        l.borrow_mut().push(ctx.type_name().into());
        Some(ctx)
    });
    block_on(editor.add_node(plain("1"))).unwrap();
    block_on(editor.add_node(plain("2"))).unwrap();
    block_on(editor.add_connection(link("c", "1", "2"))).unwrap();
    block_on(editor.remove_connection("c")).unwrap();
    block_on(editor.add_connection(link("d", "2", "1"))).unwrap();
    block_on(editor.clear()).unwrap();
    json!({ "log": *log.borrow() })
});

oracle!(cancelled_clear, {
    let editor = NodeEditor::<BaseSchemes>::new();
    let log: Log = Rc::default();
    let l = log.clone();
    editor.add_pipe(move |ctx| {
        l.borrow_mut().push(ctx.type_name().into());
        (!matches!(ctx, Root::Clear)).then_some(ctx)
    });
    block_on(editor.add_node(plain("1"))).unwrap();
    let result = block_on(editor.clear()).unwrap();
    let nodes: Vec<String> = editor
        .get_nodes()
        .iter()
        .map(|n| n.id().to_owned())
        .collect();
    json!({ "result": result, "log": *log.borrow(), "nodes": nodes })
});

oracle!(concurrent_removes, {
    let editor = NodeEditor::<BaseSchemes>::new();
    let log: Log = Rc::default();
    let l = log.clone();
    editor.add_async_pipe(move |ctx: Root<BaseSchemes>| {
        let l = l.clone();
        async move {
            if let Root::NodeRemove(n) | Root::NodeRemoved(n) = &ctx {
                Yield::once().await;
                l.borrow_mut()
                    .push(format!("{}:{}", ctx.type_name(), n.id()));
            }
            Some(ctx)
        }
    });
    for i in 1..=6 {
        block_on(editor.add_node(plain(&i.to_string()))).unwrap();
    }
    block_on(join_all(["1", "2", "3"].map(|id| editor.remove_node(id))));
    let nodes: Vec<String> = editor
        .get_nodes()
        .iter()
        .map(|n| n.id().to_owned())
        .collect();
    json!({ "log": *log.borrow(), "nodes": nodes })
});

oracle!(key_order, {
    let n = Node::<Socket>::new("n");
    for k in ["b", "2", "a", "10", "1", "01"] {
        n.add_input(k, Input::new(Socket::new("s"))).unwrap();
    }
    let first = n.inputs().key_list();
    n.remove_input("b");
    n.add_input("b", Input::new(Socket::new("s"))).unwrap();
    n.remove_input("2");
    n.add_input("2", Input::new(Socket::new("s"))).unwrap();
    json!({ "first": first, "after": n.inputs().key_list() })
});

oracle!(control_order, {
    use ruggine::classic::ClassicScheme;
    let editor = NodeEditor::<ClassicScheme>::new();
    let flow = ControlFlow::new(&editor);
    let log: Log = Rc::default();
    let mk = |id: &str| {
        let mut n = Node::new(id);
        n.id = id.into();
        n.add_input("in", Input::new(Socket::new("s"))).unwrap();
        n.add_output("out", Output::new(Socket::new("s"))).unwrap();
        n
    };
    let [a, b, c, d] = ["a", "b", "c", "d"].map(mk);
    for (s, t) in [(&a, &b), (&a, &c), (&b, &d)] {
        let link = Connection::new(s, "out", t, "in").unwrap();
        block_on(editor.add_connection(Rc::new(link))).unwrap();
    }
    for n in [&a, &b, &c, &d] {
        let (log, id) = (log.clone(), n.id.clone());
        let setup = ControlFlowNodeSetup::new(
            || vec!["in".into()],
            || vec!["out".into()],
            move |input, forward| {
                log.borrow_mut()
                    .push(format!("{id}:{}", input.unwrap_or("null")));
                forward("out")?;
                log.borrow_mut().push(format!("{id}:end"));
                Ok(())
            },
        );
        flow.add(n, setup).unwrap();
    }
    flow.execute("a", None).unwrap();
    json!({ "log": *log.borrow() })
});
