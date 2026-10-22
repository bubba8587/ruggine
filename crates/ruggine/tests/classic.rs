// [[B1]] upstreamIsSpec: rete/test/presets/classic.test.ts
use std::rc::Rc;

use ruggine::Error;
use ruggine::classic::{
    Connection, Control, Input, InputControl, InputControlOptions, Node, Output, Socket,
};

fn socket(name: &str) -> Socket {
    Socket::new(name)
}

#[test]
fn node_has_an_id_and_a_label() {
    let node = Node::<Socket>::new("A");
    assert_eq!(node.id.len(), 16);
    assert_eq!(node.label(), "A");
}

#[test]
fn adds_an_input() {
    let node = Node::new("A");
    let input = Rc::new(Input::new(socket("a")));
    node.add_input("a", input.clone()).unwrap();
    assert!(node.has_input("a"));
    assert!(Rc::ptr_eq(&node.input("a").unwrap(), &input));
}

#[test]
fn refuses_a_second_input_under_one_key() {
    let node = Node::new("A");
    node.add_input("a", Input::new(socket("a"))).unwrap();
    let err = node.add_input("a", Input::new(socket("a"))).unwrap_err();
    assert_eq!(err.to_string(), "input with key 'a' already added");
}

#[test]
fn removes_an_input() {
    let node = Node::new("A");
    node.add_input("a", Input::new(socket("a"))).unwrap();
    node.remove_input("a");
    assert!(!node.has_input("a"));
}

#[test]
fn adds_an_output() {
    let node = Node::new("A");
    let output = Rc::new(Output::new(socket("a")));
    node.add_output("a", output.clone()).unwrap();
    assert!(node.has_output("a"));
    assert!(Rc::ptr_eq(&node.output("a").unwrap(), &output));
}

#[test]
fn refuses_a_second_output_under_one_key() {
    let node = Node::new("A");
    node.add_output("a", Output::new(socket("a"))).unwrap();
    let err = node.add_output("a", Output::new(socket("a"))).unwrap_err();
    assert_eq!(err.to_string(), "output with key 'a' already added");
}

#[test]
fn removes_an_output() {
    let node = Node::new("A");
    node.add_output("a", Output::new(socket("a"))).unwrap();
    node.remove_output("a");
    assert!(!node.has_output("a"));
}

#[test]
fn connection_needs_the_target_input() {
    let a = Node::new("A");
    let b = Node::<Socket>::new("B");
    a.add_output("a", Output::new(socket("a"))).unwrap();
    let err = Connection::new(&a, "a", &b, "b").unwrap_err();
    assert_eq!(err, Error::NoTargetInput("b".into()));
}

#[test]
fn connection_needs_the_source_output() {
    let a = Node::<Socket>::new("A");
    let b = Node::new("B");
    b.add_input("b", Input::new(socket("b"))).unwrap();
    let err = Connection::new(&a, "a", &b, "b").unwrap_err();
    assert_eq!(err, Error::NoSourceOutput("a".into()));
}

#[test]
fn connection_links_the_nodes() {
    let a = Node::new("A");
    let b = Node::new("B");
    a.add_output("a", Output::new(socket("b"))).unwrap();
    b.add_input("b", Input::new(socket("a"))).unwrap();
    let c = Connection::new(&a, "a", &b, "b").unwrap();
    assert_eq!(
        (c.source.as_str(), c.target.as_str()),
        (a.id.as_str(), b.id.as_str())
    );
    assert_eq!(
        (c.source_output.as_str(), c.target_input.as_str()),
        ("a", "b")
    );
}

#[test]
fn adds_a_control_to_a_node() {
    let node = Node::<Socket>::new("A");
    node.add_control("ctrl", Control::new()).unwrap();
    assert!(node.has_control("ctrl"));
}

#[test]
fn refuses_a_second_control_under_one_key() {
    let node = Node::<Socket>::new("A");
    node.add_control("ctrl", Control::new()).unwrap();
    assert!(node.add_control("ctrl", Control::new()).is_err());
}

#[test]
fn removes_a_control_from_a_node() {
    let node = Node::<Socket>::new("A");
    node.add_control("ctrl", Control::new()).unwrap();
    node.remove_control("ctrl");
    assert!(!node.has_control("ctrl"));
}

#[test]
fn adds_a_control_to_an_input() {
    let input = Input::new(socket("a"));
    input.add_control(Control::new()).unwrap();
    assert!(input.control().is_some());
}

#[test]
fn refuses_a_second_control_on_an_input() {
    let input = Input::new(socket("a"));
    input.add_control(Control::new()).unwrap();
    let err = input.add_control(Control::new()).unwrap_err();
    assert_eq!(err.to_string(), "control already added for this input");
}

#[test]
fn removes_a_control_from_an_input() {
    let input = Input::new(socket("a"));
    input.add_control(Control::new()).unwrap();
    input.remove_control();
    assert!(input.control().is_none());
}

// Beyond upstream's suite: defaults and behavior its source states.

#[test]
fn inputs_take_one_connection_and_outputs_many_by_default() {
    assert!(!Input::new(socket("a")).multiple_connections());
    assert!(Output::new(socket("a")).multiple_connections());
    assert!(
        !Output::new(socket("a"))
            .with_multiple_connections(false)
            .multiple_connections()
    );
    let input = Input::new(socket("a")).with_label("A");
    assert_eq!(input.label().as_deref(), Some("A"));
    assert!(input.show_control());
}

#[test]
fn a_socket_can_be_swapped_in_place() {
    let input = Input::new(socket("number"));
    input.set_socket(socket("text"));
    assert_eq!(input.socket().name, "text");
}

#[test]
fn ports_keep_javascript_key_order() {
    let node = Node::new("A");
    for key in ["b", "2", "a", "10", "1"] {
        node.add_input(key, Input::new(socket(key))).unwrap();
    }
    assert_eq!(node.inputs().key_list(), ["1", "2", "10", "b", "a"]);
    node.remove_input("b");
    node.add_input("b", Input::new(socket("b"))).unwrap();
    assert_eq!(node.inputs().key_list(), ["1", "2", "10", "a", "b"]);
}

#[test]
fn input_control_holds_a_value_and_reports_changes() {
    let changes = Rc::new(std::cell::RefCell::new(Vec::new()));
    let log = changes.clone();
    let control = InputControl::number(InputControlOptions {
        initial: Some(1.0),
        change: Some(Box::new(move |v| log.borrow_mut().push(v.copied()))),
        ..Default::default()
    });
    assert_eq!(control.value(), Some(1.0));
    assert!(!control.readonly());
    control.set_value(Some(2.5));
    control.set_value(None);
    assert_eq!(*changes.borrow(), [Some(2.5), None]);
}

#[test]
fn a_typed_control_comes_back_out_of_a_node() {
    let node = Node::<Socket>::new("A");
    let text = node
        .add_control(
            "t",
            InputControl::text(InputControlOptions {
                readonly: true,
                initial: Some("hello".to_owned()),
                change: None,
            }),
        )
        .unwrap();
    let back = node.control::<InputControl<String>>("t").unwrap();
    assert!(Rc::ptr_eq(&back, &text));
    assert!(back.readonly());
    assert!(node.control::<InputControl<f64>>("t").is_none());
    assert!(node.control::<Control>("t").is_none());
}

#[test]
fn a_change_callback_may_set_the_value_again() {
    let slot: Rc<std::cell::RefCell<Option<Rc<InputControl<f64>>>>> = Rc::default();
    let handle = slot.clone();
    let control = Rc::new(InputControl::number(InputControlOptions {
        change: Some(Box::new(move |v| {
            if v.is_some_and(|v| *v > 10.0) {
                let control = handle.borrow().clone().unwrap();
                control.set_value(Some(10.0));
            }
        })),
        ..Default::default()
    }));
    *slot.borrow_mut() = Some(control.clone());
    control.set_value(Some(42.0));
    assert_eq!(control.value(), Some(10.0));
    slot.borrow_mut().take();
}

#[test]
fn cached_key_lists_follow_port_changes() {
    let node = Node::new("A");
    node.add_input("a", Input::new(socket("s"))).unwrap();
    let first = node.input_keys();
    assert!(
        Rc::ptr_eq(&first, &node.input_keys()),
        "shared until a port changes"
    );
    node.add_input("b", Input::new(socket("s"))).unwrap();
    assert_eq!(&*node.input_keys(), ["a", "b"]);
    node.remove_input("a");
    assert_eq!(&*node.input_keys(), ["b"]);
    node.add_output("x", Output::new(socket("s"))).unwrap();
    assert_eq!(&*node.output_keys(), ["x"]);
    node.remove_output("x");
    assert!(node.output_keys().is_empty());
}
