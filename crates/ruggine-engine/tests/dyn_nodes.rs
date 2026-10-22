// [[B5]] sharedHandles: a scheme whose node type is a trait object, the shape Solenoid needs
// for its many node kinds.
mod common;

use std::rc::Rc;

use common::run;
use ruggine::classic::{ClassicNode, Connection, Input, Node, Output, Socket};
use ruggine::future::{LocalBoxFuture, ready};
use ruggine::{GetSchemes, NodeBase, NodeEditor, keymap};
use ruggine_engine::{DataflowEngine, DataflowNode, Error, Inputs, Outputs};

trait AppNode: ClassicNode<Socket = Socket> + DataflowNode<f64> {}
impl<T: ClassicNode<Socket = Socket> + DataflowNode<f64>> AppNode for T {}

struct Number {
    node: Node,
    value: f64,
}

struct Add {
    node: Node,
}

macro_rules! classic {
    ($t:ty) => {
        impl NodeBase for $t {
            fn id(&self) -> &str {
                &self.node.id
            }
        }
        impl ClassicNode for $t {
            type Socket = Socket;
            fn classic(&self) -> &Node {
                &self.node
            }
        }
    };
}
classic!(Number);
classic!(Add);

impl DataflowNode<f64> for Number {
    fn data(&self, _: Inputs<f64>) -> LocalBoxFuture<'_, Result<Outputs<f64>, Error>> {
        ready(Ok(keymap! { "value" => self.value }))
    }
}

impl DataflowNode<f64> for Add {
    fn data(&self, inputs: Inputs<f64>) -> LocalBoxFuture<'_, Result<Outputs<f64>, Error>> {
        let sum = |key| inputs.get(key).map_or(0.0, |v: &Vec<f64>| v.iter().sum());
        ready(Ok(keymap! { "value" => sum("a") + sum("b") }))
    }
}

fn number(value: f64) -> Rc<dyn AppNode> {
    let node = Node::new("Number");
    node.add_output("value", Output::new(Socket::new("number")))
        .unwrap();
    Rc::new(Number { node, value })
}

fn add() -> Rc<dyn AppNode> {
    let node = Node::new("Add");
    node.add_input("a", Input::new(Socket::new("number")))
        .unwrap();
    node.add_input("b", Input::new(Socket::new("number")))
        .unwrap();
    node.add_output("value", Output::new(Socket::new("number")))
        .unwrap();
    Rc::new(Add { node })
}

#[test]
fn mixed_node_kinds_compute_through_one_engine() {
    type Schemes = GetSchemes<dyn AppNode, Connection>;
    let editor = NodeEditor::<Schemes>::new();
    let engine = DataflowEngine::<Schemes, f64>::new();
    editor.use_scope(&engine).unwrap();

    let (x, y, sum) = (number(2.0), number(3.0), add());
    for n in [&x, &y, &sum] {
        run(editor.add_node(n.clone())).unwrap();
    }
    for (from, to) in [(&x, "a"), (&y, "b")] {
        let c = Connection::new(&**from, "value", &*sum, to).unwrap();
        run(editor.add_connection(Rc::new(c))).unwrap();
    }

    let out = run(engine.fetch(sum.id())).unwrap();
    assert_eq!(out["value"], 5.0);
}
