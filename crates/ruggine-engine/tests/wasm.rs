// [[B2]] browserAndDesktop: run with
// `cargo test --target wasm32-unknown-unknown --test wasm` and wasm-bindgen-test-runner.
#![cfg(target_arch = "wasm32")]

use std::rc::Rc;

use js_sys::Promise;
use ruggine::classic::{ClassicNode, Connection, Input, Node, Output, Socket};
use ruggine::future::LocalBoxFuture;
use ruggine::{GetSchemes, NodeBase, NodeEditor, keymap};
use ruggine_engine::{DataflowEngine, DataflowNode, Error, Inputs, Outputs};
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::wasm_bindgen_test;

/// A node whose data waits on a real JavaScript promise.
struct Awaits {
    node: Node,
    value: f64,
}

impl NodeBase for Awaits {
    fn id(&self) -> &str {
        &self.node.id
    }
}

impl ClassicNode for Awaits {
    type Socket = Socket;
    fn classic(&self) -> &Node {
        &self.node
    }
}

impl DataflowNode<f64> for Awaits {
    fn data(&self, inputs: Inputs<f64>) -> LocalBoxFuture<'_, Result<Outputs<f64>, Error>> {
        let upstream: f64 = inputs.get("in").map_or(0.0, |v| v.iter().sum());
        let value = self.value;
        Box::pin(async move {
            let resolved = JsFuture::from(Promise::resolve(&js_sys::Number::from(value)))
                .await
                .unwrap();
            Ok(keymap! { "out" => upstream + resolved.as_f64().unwrap() })
        })
    }
}

fn node(value: f64) -> Rc<Awaits> {
    let node = Node::new("n");
    node.add_input(
        "in",
        Input::new(Socket::new("n")).with_multiple_connections(true),
    )
    .unwrap();
    node.add_output("out", Output::new(Socket::new("n")))
        .unwrap();
    Rc::new(Awaits { node, value })
}

#[wasm_bindgen_test]
async fn nodes_awaiting_js_promises_compute() {
    type S = GetSchemes<Awaits, Connection>;
    let editor = NodeEditor::<S>::new();
    let engine = DataflowEngine::<S, f64>::new();
    editor.use_scope(&engine).unwrap();
    let mut prev: Option<Rc<Awaits>> = None;
    for i in 0..500 {
        let n = node(i as f64);
        editor.add_node(n.clone()).await.unwrap();
        if let Some(p) = &prev {
            let c = Connection::new(&**p, "out", &*n, "in").unwrap();
            editor.add_connection(Rc::new(c)).await.unwrap();
        }
        prev = Some(n);
    }
    let out = engine.fetch(prev.unwrap().id()).await.unwrap();
    assert_eq!(out["out"], (0..500).sum::<i32>() as f64);
}
