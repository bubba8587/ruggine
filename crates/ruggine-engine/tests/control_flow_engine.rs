// [[B1]] upstreamIsSpec: rete-engine/test/control-flow-engine.test.ts
mod common;

use std::cell::Cell;
use std::rc::Rc;

use common::run;
use ruggine::classic::{ClassicNode, Connection, Input, Node, Output, Socket};
use ruggine::{GetSchemes, NodeBase, NodeEditor};
use ruggine_engine::{ControlFlowEngine, ControlFlowNode, Error, Forward};

struct TestNode {
    node: Node,
    calls: Cell<u32>,
}

impl TestNode {
    fn new(label: &str) -> Rc<Self> {
        let node = Node::new(label);
        node.add_input("input", Input::new(Socket::new("input")))
            .unwrap();
        node.add_output("output", Output::new(Socket::new("output")))
            .unwrap();
        Rc::new(Self {
            node,
            calls: Cell::new(0),
        })
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

impl ControlFlowNode for TestNode {
    fn execute(&self, _input: Option<&str>, forward: Forward<'_>) -> Result<(), Error> {
        self.calls.set(self.calls.get() + 1);
        forward("output")
    }
}

#[test]
fn executes_a_sequence_of_nodes() {
    let editor = NodeEditor::<GetSchemes<TestNode, Connection>>::new();
    let engine = ControlFlowEngine::new();
    editor.use_scope(&engine).unwrap();
    let node1 = TestNode::new("label1");
    let node2 = TestNode::new("label2");
    run(editor.add_node(node1.clone())).unwrap();
    run(editor.add_node(node2.clone())).unwrap();
    let c = Connection::new(&*node1, "output", &*node2, "input").unwrap();
    run(editor.add_connection(Rc::new(c))).unwrap();

    engine.execute(node1.id(), None).unwrap();

    assert_eq!((node1.calls.get(), node2.calls.get()), (1, 1));
}
