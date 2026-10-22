// [[B1]] upstreamIsSpec: rete-engine/test/dataflow-engine.test.ts
mod common;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::task::Poll;

use common::{Controlled, poll, run};
use ruggine::classic::{ClassicNode, Connection, Input, Node, Output, Socket};
use ruggine::future::LocalBoxFuture;
use ruggine::{GetSchemes, NodeBase, NodeEditor, keymap};
use ruggine_engine::{DataflowEngine, DataflowNode, Error, Inputs, Outputs};

/// Upstream's test node, with a call counter standing in for `jest.spyOn(node, 'data')`.
struct TestNode {
    node: Node,
    produces: String,
    calls: Cell<u32>,
    seen: RefCell<Vec<Inputs<String>>>,
    gate: RefCell<Option<Controlled<()>>>,
}

impl TestNode {
    fn new(label: &str, produces: &str) -> Rc<Self> {
        let node = Node::new(label);
        node.add_input(
            "input",
            Input::new(Socket::new("input")).with_multiple_connections(true),
        )
        .unwrap();
        node.add_output("output", Output::new(Socket::new("output")))
            .unwrap();
        Rc::new(Self {
            node,
            produces: produces.into(),
            calls: Cell::new(0),
            seen: RefCell::new(Vec::new()),
            gate: RefCell::new(None),
        })
    }

    fn id(&self) -> String {
        self.node.id.clone()
    }
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

impl DataflowNode<String> for TestNode {
    fn data(&self, inputs: Inputs<String>) -> LocalBoxFuture<'_, Result<Outputs<String>, Error>> {
        self.calls.set(self.calls.get() + 1);
        self.seen.borrow_mut().push(inputs);
        let gate = self.gate.borrow().clone();
        Box::pin(async move {
            if let Some(gate) = gate {
                gate.await;
            }
            Ok(keymap! { "output" => self.produces.clone() })
        })
    }
}

type Schemes = GetSchemes<TestNode, Connection>;

fn setup() -> (NodeEditor<Schemes>, DataflowEngine<Schemes, String>) {
    let editor = NodeEditor::new();
    let engine = DataflowEngine::new();
    editor.use_scope(&engine).unwrap();
    (editor, engine)
}

fn connect(editor: &NodeEditor<Schemes>, a: &TestNode, b: &TestNode) {
    let c = Connection::new(a, "output", b, "input").unwrap();
    run(editor.add_connection(Rc::new(c))).unwrap();
}

fn three(editor: &NodeEditor<Schemes>) -> [Rc<TestNode>; 3] {
    let nodes = [
        TestNode::new("label1", "data1"),
        TestNode::new("label2", "data2"),
        TestNode::new("label3", "data3"),
    ];
    for n in &nodes {
        run(editor.add_node(n.clone())).unwrap();
    }
    nodes
}

#[test]
fn collects_input_data_from_the_predecessors() {
    let (editor, engine) = setup();
    let [n1, n2, n3] = three(&editor);
    connect(&editor, &n1, &n3);
    connect(&editor, &n2, &n3);
    assert_eq!((n1.calls.get(), n2.calls.get(), n3.calls.get()), (0, 0, 0));

    let inputs = run(engine.fetch_inputs(&n3.id())).unwrap();

    assert_eq!((n1.calls.get(), n2.calls.get(), n3.calls.get()), (1, 1, 0));
    assert_eq!(
        inputs,
        keymap! { "input" => vec!["data1".to_owned(), "data2".to_owned()] }
    );
}

#[test]
fn fetches_data_from_the_leaf_node() {
    let (editor, engine) = setup();
    let [n1, n2, n3] = three(&editor);
    connect(&editor, &n1, &n3);
    connect(&editor, &n2, &n3);

    let out = run(engine.fetch(&n3.id())).unwrap();

    assert_eq!((n1.calls.get(), n2.calls.get(), n3.calls.get()), (1, 1, 1));
    assert_eq!(*out, keymap! { "output" => "data3".to_owned() });
    assert_eq!(
        n3.seen.borrow()[0],
        keymap! { "input" => vec!["data1".to_owned(), "data2".to_owned()] }
    );
}

#[test]
fn caches_the_data() {
    let (editor, engine) = setup();
    let node = TestNode::new("label", "data");
    run(editor.add_node(node.clone())).unwrap();
    let first = run(engine.fetch(&node.id())).unwrap();
    assert_eq!(node.calls.get(), 1);
    let second = run(engine.fetch(&node.id())).unwrap();
    assert_eq!(node.calls.get(), 1);
    assert!(Rc::ptr_eq(&first, &second));
}

#[test]
fn reset_clears_the_cache() {
    let (editor, engine) = setup();
    let node = TestNode::new("label", "data");
    run(editor.add_node(node.clone())).unwrap();
    run(engine.fetch(&node.id())).unwrap();
    assert_eq!(node.calls.get(), 1);
    engine.reset(Some(&node.id())).unwrap();
    run(engine.fetch(&node.id())).unwrap();
    assert_eq!(node.calls.get(), 2);
}

#[test]
fn reset_clears_the_node_and_its_successors() {
    let (editor, engine) = setup();
    let [n1, n2, n3] = three(&editor);
    connect(&editor, &n1, &n2);
    connect(&editor, &n2, &n3);

    run(engine.fetch(&n3.id())).unwrap();
    assert_eq!((n1.calls.get(), n3.calls.get()), (1, 1));
    run(engine.fetch(&n3.id())).unwrap();
    assert_eq!((n1.calls.get(), n3.calls.get()), (1, 1));

    engine.reset(Some(&n2.id())).unwrap();
    run(engine.fetch(&n3.id())).unwrap();
    assert_eq!((n1.calls.get(), n2.calls.get(), n3.calls.get()), (1, 2, 2));
}

// Beyond upstream's suite: behavior its source states.

#[test]
fn reset_without_a_node_clears_everything() {
    let (editor, engine) = setup();
    let [n1, n2, _] = three(&editor);
    connect(&editor, &n1, &n2);
    run(engine.fetch(&n2.id())).unwrap();
    engine.reset(None).unwrap();
    run(engine.fetch(&n2.id())).unwrap();
    assert_eq!((n1.calls.get(), n2.calls.get()), (2, 2));
}

#[test]
fn reset_terminates_on_a_cycle() {
    let (editor, engine) = setup();
    let [n1, n2, _] = three(&editor);
    connect(&editor, &n1, &n2);
    connect(&editor, &n2, &n1);
    engine.reset(Some(&n1.id())).unwrap();
}

#[test]
fn reset_of_an_unknown_node_fails() {
    let (_editor, engine) = setup();
    assert_eq!(engine.reset(Some("nope")).unwrap_err().to_string(), "setup");
}

#[test]
fn a_removed_node_leaves_the_dataflow() {
    let (editor, engine) = setup();
    let node = TestNode::new("label", "data");
    run(editor.add_node(node.clone())).unwrap();
    run(editor.remove_node(&node.id())).unwrap();
    assert_eq!(
        run(engine.fetch(&node.id())).unwrap_err(),
        Error::NotInitialized
    );
}

#[test]
fn a_shared_predecessor_runs_once_per_fetch() {
    let (editor, engine) = setup();
    let [n1, n2, n3] = three(&editor);
    connect(&editor, &n1, &n2);
    connect(&editor, &n1, &n3);
    connect(&editor, &n2, &n3);
    run(engine.fetch(&n3.id())).unwrap();
    assert_eq!(n1.calls.get(), 1);
}

#[test]
fn reset_during_a_fetch_cancels_it() {
    let (editor, engine) = setup();
    let [n1, n2, _] = three(&editor);
    connect(&editor, &n1, &n2);
    let gate = Controlled::new();
    *n1.gate.borrow_mut() = Some(gate.clone());

    let mut fetch = engine.fetch(&n2.id());
    assert!(poll(&mut fetch).is_pending());
    engine.reset(Some(&n1.id())).unwrap();
    gate.resolve(());
    assert_eq!(poll(&mut fetch), Poll::Ready(Err(Error::Cancelled)));

    *n1.gate.borrow_mut() = None;
    let out = run(engine.fetch(&n2.id())).unwrap();
    assert_eq!(*out, keymap! { "output" => "data2".to_owned() });
}

#[test]
fn an_engine_not_attached_says_so() {
    let engine = DataflowEngine::<Schemes, String>::new();
    assert_eq!(
        run(engine.fetch("x")).unwrap_err(),
        Error::NotAttached("DataflowEngine")
    );
}

#[test]
fn editor_and_engine_dont_keep_each_other_alive() {
    let (editor, engine) = setup();
    let node = TestNode::new("label", "data");
    run(editor.add_node(node.clone())).unwrap();
    let weak_editor = editor.downgrade();
    drop(editor);
    assert!(weak_editor.upgrade().is_none());
    assert_eq!(
        run(engine.fetch(&node.id())).unwrap_err(),
        Error::EditorDropped
    );
}

#[test]
fn reset_during_a_nodes_own_data_cancels_its_fetch() {
    let (editor, engine) = setup();
    let node = TestNode::new("label", "data");
    run(editor.add_node(node.clone())).unwrap();
    let gate = Controlled::new();
    *node.gate.borrow_mut() = Some(gate.clone());

    let mut fetch = engine.fetch(&node.id());
    assert!(poll(&mut fetch).is_pending());
    assert_eq!(node.calls.get(), 1);
    engine.reset(Some(&node.id())).unwrap();
    gate.resolve(());
    assert_eq!(poll(&mut fetch), Poll::Ready(Err(Error::Cancelled)));
}

#[test]
fn a_long_chain_fetches_without_deep_recursion() {
    let (editor, engine) = setup();
    let mut prev: Option<Rc<TestNode>> = None;
    for i in 0..50_000 {
        let node = TestNode::new("n", &i.to_string());
        run(editor.add_node(node.clone())).unwrap();
        if let Some(p) = &prev {
            connect(&editor, p, &node);
        }
        prev = Some(node);
    }
    let last = prev.unwrap();
    let out = run(engine.fetch(&last.id())).unwrap();
    assert_eq!(out["output"], "49999");
    assert_eq!(
        *last.seen.borrow()[0].get("input").unwrap(),
        vec!["49998".to_owned()]
    );
}

#[test]
fn a_detached_engine_names_itself() {
    let engine = DataflowEngine::<Schemes, String>::new();
    let err = run(engine.fetch("x")).unwrap_err();
    assert_eq!(
        err.to_string(),
        "DataflowEngine isn't attached to NodeEditor"
    );
}
