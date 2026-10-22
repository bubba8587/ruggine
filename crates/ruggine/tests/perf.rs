// [[B1]] upstreamIsSpec: rete/test/index.perf.ts
use std::rc::Rc;

use ruggine::future::{join_all, now_or_never};
use ruggine::{BaseSchemes, ConnectionBase, NodeBase, NodeEditor};

struct N(String);

impl NodeBase for N {
    fn id(&self) -> &str {
        &self.0
    }
}

struct C(String, String, String);

impl ConnectionBase for C {
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

fn run<F: std::future::Future>(f: F) -> F::Output {
    now_or_never(f).unwrap()
}

#[test]
fn removes_five_thousand_nodes() {
    let editor = NodeEditor::<BaseSchemes>::new();
    let ids: Vec<String> = (0..5_000).map(|i| format!("s{i}")).collect();
    run(join_all(
        ids.iter().map(|id| editor.add_node(Rc::new(N(id.clone())))),
    ));
    run(join_all(ids.iter().rev().map(|id| editor.remove_node(id))));
    assert!(editor.get_nodes().is_empty());
}

#[test]
fn removes_three_thousand_connections() {
    let editor = NodeEditor::<BaseSchemes>::new();
    let ids: Vec<usize> = (0..3_000).collect();
    for prefix in ["s", "t"] {
        run(join_all(ids.iter().map(|i| {
            editor.add_node(Rc::new(N(format!("{prefix}{i}"))))
        })));
    }
    run(join_all(ids.iter().map(|i| {
        editor.add_connection(Rc::new(C(
            format!("c{i}"),
            format!("s{i}"),
            format!("t{i}"),
        )))
    })));
    let connection_ids: Vec<String> = ids.iter().map(|i| format!("c{i}")).collect();
    run(join_all(
        connection_ids
            .iter()
            .rev()
            .map(|id| editor.remove_connection(id)),
    ));
    assert!(editor.get_connections().is_empty());
}
