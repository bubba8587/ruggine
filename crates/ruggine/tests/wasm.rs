// [[B2]] browserAndDesktop: run with
// `cargo test --target wasm32-unknown-unknown --test wasm` and wasm-bindgen-test-runner.
#![cfg(target_arch = "wasm32")]

use std::rc::Rc;

use ruggine::future::now_or_never;
use ruggine::{BaseSchemes, NodeBase, NodeEditor, get_uid};
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn ids_come_from_the_hosts_crypto() {
    let (a, b) = (get_uid(), get_uid());
    assert_eq!(a.len(), 16);
    assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
    assert_ne!(a, b);
}

struct N(String);

impl NodeBase for N {
    fn id(&self) -> &str {
        &self.0
    }
}

#[wasm_bindgen_test]
fn the_editor_works() {
    let editor = NodeEditor::<BaseSchemes>::new();
    now_or_never(editor.add_node(Rc::new(N("a".into()))))
        .unwrap()
        .unwrap();
    assert_eq!(editor.get_nodes().len(), 1);
}
