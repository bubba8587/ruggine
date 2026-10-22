// [[B1]] upstreamIsSpec: rete-engine/test/dataflow.test.ts
mod common;

use std::rc::Rc;

use common::run;
use ruggine::classic::{ClassicScheme, Connection, Input, Node, Output, Socket};
use ruggine::{NodeEditor, keymap};
use ruggine_engine::{Dataflow, DataflowNodeSetup, Error, FetchInputs, Outputs};

type Flow = Dataflow<ClassicScheme, String>;

fn setup() -> (NodeEditor<ClassicScheme>, Flow) {
    let editor = NodeEditor::new();
    let flow = Dataflow::new(&editor);
    (editor, flow)
}

fn empty() -> DataflowNodeSetup<String> {
    DataflowNodeSetup::new(Vec::new, Vec::new, |_| async {
        Ok(Rc::new(Outputs::new()))
    })
}

fn keys(keys: &'static [&'static str]) -> impl Fn() -> Vec<String> {
    move || keys.iter().map(|k| k.to_string()).collect()
}

#[test]
fn adds_a_node() {
    let (_editor, flow) = setup();
    let node = Node::<Socket>::new("label");
    flow.add(&node, empty()).unwrap();
    assert!(flow.has(&node.id));
}

#[test]
fn refuses_a_node_already_added() {
    let (_editor, flow) = setup();
    let node = Node::<Socket>::new("label");
    flow.add(&node, empty()).unwrap();
    let err = flow.add(&node, empty()).unwrap_err();
    assert_eq!(err.to_string(), "already processed");
}

#[test]
fn removes_a_node() {
    let (_editor, flow) = setup();
    let node = Node::<Socket>::new("label");
    flow.add(&node, empty()).unwrap();
    flow.remove(&node.id);
    assert!(!flow.has(&node.id));
}

#[test]
fn fetches_a_nodes_inputs() {
    let (editor, flow) = setup();
    let node1 = Node::new("label1");
    let node2 = Node::new("label2");
    node1
        .add_output("out", Output::new(Socket::new("output")))
        .unwrap();
    node2
        .add_input("in", Input::new(Socket::new("input")))
        .unwrap();
    flow.add(
        &node1,
        DataflowNodeSetup::new(Vec::new, keys(&["out"]), |_| async {
            Ok(Rc::new(keymap! { "out" => "my data".to_owned() }))
        }),
    )
    .unwrap();
    flow.add(
        &node2,
        DataflowNodeSetup::new(
            keys(&["in"]),
            Vec::new,
            |fetch_inputs: FetchInputs<String>| async move {
                let inputs = fetch_inputs().await?;
                Ok(Rc::new(keymap! { "in" => inputs["in"][0].clone() }))
            },
        ),
    )
    .unwrap();
    let c = Connection::new(&node1, "out", &node2, "in").unwrap();
    run(editor.add_connection(Rc::new(c))).unwrap();

    let inputs = run(flow.fetch_inputs(&node2.id)).unwrap();
    assert_eq!(inputs, keymap! { "in" => vec!["my data".to_owned()] });
}

#[test]
fn fetches_a_nodes_outputs() {
    let (_editor, flow) = setup();
    let node = Node::new("label");
    node.add_output("out", Output::new(Socket::new("output")))
        .unwrap();
    flow.add(
        &node,
        DataflowNodeSetup::new(Vec::new, keys(&["out"]), |_| async {
            Ok(Rc::new(keymap! { "out" => "data".to_owned() }))
        }),
    )
    .unwrap();
    let outputs = run(flow.fetch(&node.id)).unwrap();
    assert_eq!(*outputs, keymap! { "out" => "data".to_owned() });
}

#[test]
fn fetching_inputs_of_an_unknown_node_fails() {
    let (_editor, flow) = setup();
    let err = run(flow.fetch_inputs("1")).unwrap_err();
    assert_eq!(err.to_string(), "node is not initialized");
}

#[test]
fn fetching_outputs_of_an_unknown_node_fails() {
    let (_editor, flow) = setup();
    let err = run(flow.fetch("1")).unwrap_err();
    assert_eq!(err, Error::NotInitialized);
}

#[test]
fn data_missing_a_required_output_fails() {
    let (_editor, flow) = setup();
    let node = Node::new("label");
    node.add_output("out", Output::new(Socket::new("output")))
        .unwrap();
    flow.add(
        &node,
        DataflowNodeSetup::new(Vec::new, keys(&["out"]), |_| async {
            Ok(Rc::new(keymap! { "other" => "x".to_owned() }))
        }),
    )
    .unwrap();
    let err = run(flow.fetch(&node.id)).unwrap_err();
    assert_eq!(
        err.to_string(),
        format!(
            "dataflow node \"{}\" doesn't return all of required properties. Expected \"out\". Got \"other\"",
            node.id
        )
    );
}

// Beyond upstream's suite.

#[test]
fn inputs_collect_one_value_per_connection_in_connection_order() {
    let (editor, flow) = setup();
    let a = Node::new("a");
    let b = Node::new("b");
    let sink = Node::new("sink");
    for n in [&a, &b] {
        n.add_output("out", Output::new(Socket::new("s"))).unwrap();
    }
    sink.add_input(
        "in",
        Input::new(Socket::new("s")).with_multiple_connections(true),
    )
    .unwrap();
    for (n, v) in [(&a, "A"), (&b, "B")] {
        flow.add(
            n,
            DataflowNodeSetup::new(Vec::new, keys(&["out"]), move |_| async move {
                Ok(Rc::new(keymap! { "out" => v.to_owned() }))
            }),
        )
        .unwrap();
    }
    flow.add(
        &sink,
        DataflowNodeSetup::new(keys(&["in"]), Vec::new, |_| async {
            Ok(Rc::new(Outputs::new()))
        }),
    )
    .unwrap();
    for n in [&b, &a] {
        let c = Connection::new(n, "out", &sink, "in").unwrap();
        run(editor.add_connection(Rc::new(c))).unwrap();
    }
    let inputs = run(flow.fetch_inputs(&sink.id)).unwrap();
    assert_eq!(
        inputs,
        keymap! { "in" => vec!["B".to_owned(), "A".to_owned()] }
    );
}

#[test]
fn a_failing_predecessor_fails_the_fetch() {
    #[derive(Debug)]
    struct Boom;
    impl std::fmt::Display for Boom {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("boom")
        }
    }
    impl std::error::Error for Boom {}

    let (editor, flow) = setup();
    let a = Node::new("a");
    let sink = Node::new("sink");
    a.add_output("out", Output::new(Socket::new("s"))).unwrap();
    sink.add_input("in", Input::new(Socket::new("s"))).unwrap();
    flow.add(
        &a,
        DataflowNodeSetup::new(Vec::new, keys(&["out"]), |_| async {
            Err(Error::node(Boom))
        }),
    )
    .unwrap();
    flow.add(
        &sink,
        DataflowNodeSetup::new(
            keys(&["in"]),
            Vec::new,
            |fetch_inputs: FetchInputs<String>| async move {
                fetch_inputs().await?;
                Ok(Rc::new(Outputs::new()))
            },
        ),
    )
    .unwrap();
    let c = Connection::new(&a, "out", &sink, "in").unwrap();
    run(editor.add_connection(Rc::new(c))).unwrap();
    assert_eq!(run(flow.fetch(&sink.id)).unwrap_err().to_string(), "boom");
}
