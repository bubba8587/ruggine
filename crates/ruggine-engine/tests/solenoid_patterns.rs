// [[A3]] portEverything: Solenoid's uses of rete (src/graph/graphCompute.ts, nodes/composite.ts),
// written against Ruggine.
mod common;

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

use common::{Controlled, block_on, poll, run};
use ruggine::classic::{ClassicNode, Connection, Input, Node, Output, Socket};
use ruggine::future::LocalBoxFuture;
use ruggine::{GetSchemes, NodeBase, NodeEditor, keymap};
use ruggine_engine::{Cancellable, DataflowEngine, DataflowNode, Error, Inputs, Outputs};

#[derive(Clone, Debug, PartialEq)]
enum Value {
    Num(f64),
    Err(String),
}

trait SolNode: ClassicNode<Socket = Socket> + DataflowNode<Value> {
    fn runs(&self) -> u32;
}

type Schemes = GetSchemes<dyn SolNode, Connection>;
type Editor = NodeEditor<Schemes>;
type Engine = DataflowEngine<Schemes, Value>;

/// Sums its inputs plus a constant; an error input passes straight through, as Solenoid's
/// error values do.
struct Sum {
    node: Node,
    add: f64,
    runs: Cell<u32>,
    gate: RefCell<Option<Controlled<()>>>,
}

impl NodeBase for Sum {
    fn id(&self) -> &str {
        &self.node.id
    }
}
impl ClassicNode for Sum {
    type Socket = Socket;
    fn classic(&self) -> &Node {
        &self.node
    }
}
impl DataflowNode<Value> for Sum {
    fn data(&self, inputs: Inputs<Value>) -> LocalBoxFuture<'_, Result<Outputs<Value>, Error>> {
        self.runs.set(self.runs.get() + 1);
        let gate = self.gate.borrow().clone();
        Box::pin(async move {
            if let Some(gate) = gate {
                gate.await;
            }
            let mut total = self.add;
            for v in inputs.get("in").into_iter().flatten() {
                match v {
                    Value::Num(n) => total += n,
                    err => return Ok(keymap! { "out" => err.clone() }),
                }
            }
            Ok(keymap! { "out" => Value::Num(total) })
        })
    }
}
impl SolNode for Sum {
    fn runs(&self) -> u32 {
        self.runs.get()
    }
}

fn sum(id: &str, add: f64) -> Rc<Sum> {
    let mut node = Node::new("Sum");
    node.id = id.into();
    node.add_input(
        "in",
        Input::new(Socket::new("n")).with_multiple_connections(true),
    )
    .unwrap();
    node.add_output("out", Output::new(Socket::new("n")))
        .unwrap();
    Rc::new(Sum {
        node,
        add,
        runs: Cell::new(0),
        gate: RefCell::new(None),
    })
}

fn graph() -> (Editor, Engine) {
    let editor = Editor::new();
    let engine = Engine::new();
    editor.use_scope(&engine).unwrap();
    (editor, engine)
}

fn link(editor: &Editor, from: &str, to: &str) {
    let (a, b) = (editor.get_node(from).unwrap(), editor.get_node(to).unwrap());
    let c = Connection::new(&*a, "out", &*b, "in").unwrap();
    run(editor.add_connection(Rc::new(c))).unwrap();
}

fn add(editor: &Editor, node: Rc<dyn SolNode>) {
    run(editor.add_node(node)).unwrap();
}

/// graphCompute.ts `downstreamClosure` + `invalidate`: clear the cache of everything a change
/// reaches, through every connection.
fn invalidate(editor: &Editor, engine: &Engine, changed: &str) -> HashSet<String> {
    let mut cone = HashSet::from([changed.to_owned()]);
    let mut queue = vec![changed.to_owned()];
    while let Some(id) = queue.pop() {
        for c in editor.get_connections_from(&id) {
            if cone.insert(c.target.clone()) {
                queue.push(c.target.clone());
            }
        }
    }
    for id in &cone {
        engine.cache().delete(id.as_str());
    }
    cone
}

/// graphCompute.ts `seedLoopErrors`: a node on a cycle gets an error result without running.
fn seed_loop_errors(editor: &Editor, engine: &Engine, members: &[&str]) {
    for id in members {
        let node = editor.get_node(id).unwrap();
        let outputs: Outputs<Value> = node
            .classic()
            .output_keys()
            .iter()
            .map(|k| (k.clone(), Value::Err("#CIRC!".into())))
            .collect();
        let seeded = Cancellable::resolved(Ok(Rc::new(outputs)));
        if engine
            .cache()
            .add((*id).to_owned(), seeded.clone())
            .is_err()
        {
            engine.cache().patch((*id).to_owned(), seeded);
        }
    }
}

#[test]
fn a_cycle_seeded_with_errors_lets_everything_downstream_compute() {
    let (editor, engine) = graph();
    for (id, k) in [("a", 1.0), ("b", 2.0), ("c", 3.0)] {
        add(&editor, sum(id, k));
    }
    link(&editor, "a", "b");
    link(&editor, "b", "a");
    link(&editor, "b", "c");
    seed_loop_errors(&editor, &engine, &["a", "b"]);
    let out = run(engine.fetch("c")).expect("seeded loop members don't hang the fetch");
    assert_eq!(out["out"], Value::Err("#CIRC!".into()));
    assert!(
        engine.reset(Some("a")).is_ok(),
        "reset terminates on the cycle"
    );
}

#[test]
fn invalidating_a_change_recomputes_only_what_it_reaches() {
    let (editor, engine) = graph();
    let nodes: Vec<Rc<Sum>> = [("x", 1.0), ("y", 10.0), ("z", 100.0), ("w", 1000.0)]
        .into_iter()
        .map(|(id, k)| sum(id, k))
        .collect();
    for n in &nodes {
        add(&editor, n.clone());
    }
    link(&editor, "x", "y");
    link(&editor, "y", "z");
    for n in ["z", "w"] {
        run(engine.fetch(n)).unwrap();
    }
    let cone = invalidate(&editor, &engine, "y");
    assert_eq!(cone, HashSet::from(["y".into(), "z".into()]));
    for n in ["z", "w"] {
        run(engine.fetch(n)).unwrap();
    }
    let runs: Vec<u32> = nodes.iter().map(|n| n.runs()).collect();
    assert_eq!(runs, [1, 2, 2, 1]);
}

#[test]
fn a_pass_over_every_node_survives_a_reset_mid_pass() {
    let (editor, engine) = graph();
    let slow = sum("slow", 1.0);
    add(&editor, slow.clone());
    add(&editor, sum("after", 2.0));
    link(&editor, "slow", "after");
    let gate = Controlled::new();
    *slow.gate.borrow_mut() = Some(gate.clone());

    // graphCompute.ts `fetchAll`: fetch each node in turn, a Cancelled fetch counts as no value.
    let mut pass = Box::pin(async {
        let mut out = Vec::new();
        for node in editor.get_nodes() {
            match engine.fetch(node.id()).await {
                Ok(v) => out.push(Some(v["out"].clone())),
                Err(e) if e.is_cancelled() => out.push(None),
                Err(e) => panic!("{e}"),
            }
        }
        out
    });
    assert!(poll(&mut pass).is_pending());
    engine.reset(Some("slow")).unwrap();
    gate.resolve(());
    *slow.gate.borrow_mut() = None;
    let out = block_on(pass);
    assert_eq!(out, [None, Some(Value::Num(3.0))]);
}

/// nodes/composite.ts: a node whose data is a whole inner graph with its own editor and engine.
struct Composite {
    node: Node,
    inner_editor: Editor,
    inner_engine: Engine,
    runs: Cell<u32>,
}

impl NodeBase for Composite {
    fn id(&self) -> &str {
        &self.node.id
    }
}
impl ClassicNode for Composite {
    type Socket = Socket;
    fn classic(&self) -> &Node {
        &self.node
    }
}
impl DataflowNode<Value> for Composite {
    fn data(&self, _: Inputs<Value>) -> LocalBoxFuture<'_, Result<Outputs<Value>, Error>> {
        self.runs.set(self.runs.get() + 1);
        Box::pin(async move {
            self.inner_engine.reset(None)?;
            let inner = self.inner_engine.fetch("inner-out").await?;
            Ok(keymap! { "out" => inner["out"].clone() })
        })
    }
}
impl SolNode for Composite {
    fn runs(&self) -> u32 {
        self.runs.get()
    }
}

#[test]
fn a_composite_runs_its_inner_graph_inside_its_data() {
    let (inner_editor, inner_engine) = graph();
    add(&inner_editor, sum("inner-in", 5.0));
    add(&inner_editor, sum("inner-out", 1.0));
    link(&inner_editor, "inner-in", "inner-out");
    let mut node = Node::new("Composite");
    node.id = "comp".into();
    node.add_output("out", Output::new(Socket::new("n")))
        .unwrap();
    let comp = Rc::new(Composite {
        node,
        inner_editor,
        inner_engine,
        runs: Cell::new(0),
    });

    let (editor, engine) = graph();
    add(&editor, comp.clone());
    add(&editor, sum("outer", 100.0));
    link(&editor, "comp", "outer");
    assert_eq!(
        run(engine.fetch("outer")).unwrap()["out"],
        Value::Num(106.0)
    );

    let inner_in = comp.inner_editor.get_node("inner-in").unwrap();
    assert_eq!(inner_in.runs(), 1);
    engine.reset(Some("comp")).unwrap();
    assert_eq!(
        run(engine.fetch("outer")).unwrap()["out"],
        Value::Num(106.0)
    );
    assert_eq!((comp.runs(), inner_in.runs()), (2, 2));
}
