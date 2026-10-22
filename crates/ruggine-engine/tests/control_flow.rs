// [[B1]] upstreamIsSpec: rete-engine/test/control-flow.test.ts
mod common;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use common::run;
use ruggine::NodeEditor;
use ruggine::classic::{ClassicScheme, Connection, Input, Node, Output, Socket};
use ruggine_engine::{ControlFlow, ControlFlowNodeSetup, Error};

fn setup() -> (NodeEditor<ClassicScheme>, ControlFlow<ClassicScheme>) {
    let editor = NodeEditor::new();
    let flow = ControlFlow::new(&editor);
    (editor, flow)
}

fn idle() -> ControlFlowNodeSetup {
    ControlFlowNodeSetup::new(Vec::new, Vec::new, |_, _| Ok(()))
}

fn counter() -> (Rc<Cell<u32>>, Rc<Cell<u32>>) {
    let c = Rc::new(Cell::new(0));
    (c.clone(), c)
}

#[test]
fn adds_a_node() {
    let (_editor, flow) = setup();
    let node = Node::<Socket>::new("label");
    flow.add(&node, idle()).unwrap();
    assert!(flow.has(&node.id));
}

#[test]
fn removes_a_node() {
    let (_editor, flow) = setup();
    let node = Node::<Socket>::new("label");
    flow.add(&node, idle()).unwrap();
    flow.remove(&node.id);
    assert!(!flow.has(&node.id));
}

#[test]
fn executes_a_node() {
    let (_editor, flow) = setup();
    let node = Node::<Socket>::new("label");
    let (calls, count) = counter();
    flow.add(
        &node,
        ControlFlowNodeSetup::new(Vec::new, Vec::new, move |_, _| {
            count.set(count.get() + 1);
            Ok(())
        }),
    )
    .unwrap();
    assert_eq!(calls.get(), 0);
    flow.execute(&node.id, None).unwrap();
    assert_eq!(calls.get(), 1);
}

#[test]
fn executes_a_sequence_of_nodes() {
    let (editor, flow) = setup();
    let node1 = Node::new("label");
    let node2 = Node::new("label");
    node1
        .add_output("out", Output::new(Socket::new("number")))
        .unwrap();
    node2
        .add_input("in", Input::new(Socket::new("number")))
        .unwrap();
    let c = Connection::new(&node1, "out", &node2, "in").unwrap();
    run(editor.add_connection(Rc::new(c))).unwrap();

    let (calls1, count1) = counter();
    let (calls2, count2) = counter();
    flow.add(
        &node1,
        ControlFlowNodeSetup::new(
            Vec::new,
            || vec!["out".into()],
            move |_, forward| {
                count1.set(count1.get() + 1);
                forward("out")
            },
        ),
    )
    .unwrap();
    flow.add(
        &node2,
        ControlFlowNodeSetup::new(
            || vec!["in".into()],
            Vec::new,
            move |_, _| {
                count2.set(count2.get() + 1);
                Ok(())
            },
        ),
    )
    .unwrap();
    flow.execute(&node1.id, None).unwrap();
    assert_eq!((calls1.get(), calls2.get()), (1, 1));
}

#[test]
fn executing_an_unknown_node_fails() {
    let (_editor, flow) = setup();
    let node = Node::<Socket>::new("label");
    let err = flow.execute(&node.id, None).unwrap_err();
    assert_eq!(err.to_string(), "node is not initialized");
}

#[test]
fn adding_a_node_twice_fails() {
    let (_editor, flow) = setup();
    let node = Node::<Socket>::new("label");
    flow.add(&node, idle()).unwrap();
    assert_eq!(
        flow.add(&node, idle()).unwrap_err().to_string(),
        "already processed"
    );
}

#[test]
fn removing_an_unknown_node_is_fine() {
    let (_editor, flow) = setup();
    flow.remove("nope");
}

#[test]
fn an_unknown_input_fails() {
    let (_editor, flow) = setup();
    let node = Node::<Socket>::new("label");
    flow.add(&node, idle()).unwrap();
    let err = flow.execute(&node.id, Some("input")).unwrap_err();
    assert_eq!(err.to_string(), "inputs don't have a key");
}

#[test]
fn an_unknown_output_fails() {
    let (_editor, flow) = setup();
    let node = Node::<Socket>::new("label");
    flow.add(
        &node,
        ControlFlowNodeSetup::new(Vec::new, Vec::new, |_, forward| forward("output")),
    )
    .unwrap();
    let err = flow.execute(&node.id, None).unwrap_err();
    assert_eq!(err, Error::UnknownOutput);
    assert_eq!(err.to_string(), "outputs don't have a key");
}

// Beyond upstream's suite.

#[test]
fn control_runs_depth_first_and_returns_to_the_forwarder() {
    let (editor, flow) = setup();
    let order = Rc::new(RefCell::new(Vec::new()));
    let a = Node::new("a");
    let b = Node::new("b");
    let c = Node::new("c");
    a.add_output("out", Output::new(Socket::new("s"))).unwrap();
    b.add_input("in", Input::new(Socket::new("s"))).unwrap();
    c.add_input("in", Input::new(Socket::new("s"))).unwrap();
    for target in [&b, &c] {
        let link = Connection::new(&a, "out", target, "in").unwrap();
        run(editor.add_connection(Rc::new(link))).unwrap();
    }
    let log = order.clone();
    flow.add(
        &a,
        ControlFlowNodeSetup::new(
            Vec::new,
            || vec!["out".into()],
            move |input, forward| {
                log.borrow_mut().push(format!("a:{input:?}"));
                forward("out")?;
                log.borrow_mut().push("a:end".into());
                Ok(())
            },
        ),
    )
    .unwrap();
    for (n, name) in [(&b, "b"), (&c, "c")] {
        let log = order.clone();
        flow.add(
            n,
            ControlFlowNodeSetup::new(
                || vec!["in".into()],
                Vec::new,
                move |input, _| {
                    log.borrow_mut().push(format!("{name}:{input:?}"));
                    Ok(())
                },
            ),
        )
        .unwrap();
    }
    flow.execute(&a.id, None).unwrap();
    assert_eq!(
        *order.borrow(),
        ["a:None", "b:Some(\"in\")", "c:Some(\"in\")", "a:end"]
    );
}
