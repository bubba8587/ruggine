// [[A3]] portEverything: upstream's `configure` argument, which picks each node's ports and lets
// the engines run nodes that aren't classic-preset nodes.
mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::run;
use ruggine::classic::Connection;
use ruggine::future::{LocalBoxFuture, ready};
use ruggine::{GetSchemes, NodeBase, NodeEditor, keymap};
use ruggine_engine::{
    ControlFlowEngine, ControlFlowNode, DataflowEngine, DataflowNode, Error, Forward, Inputs,
    Outputs, PortSelection,
};

/// A node with no classic ports: its keys live in plain lists the configure reads.
struct Bare {
    id: String,
    inputs: Vec<String>,
    outputs: Vec<String>,
    log: Rc<RefCell<Vec<String>>>,
}

impl Bare {
    fn new(
        id: &str,
        inputs: &[&str],
        outputs: &[&str],
        log: &Rc<RefCell<Vec<String>>>,
    ) -> Rc<Self> {
        Rc::new(Self {
            id: id.into(),
            inputs: inputs.iter().map(|k| k.to_string()).collect(),
            outputs: outputs.iter().map(|k| k.to_string()).collect(),
            log: log.clone(),
        })
    }
}

impl NodeBase for Bare {
    fn id(&self) -> &str {
        &self.id
    }
}

impl DataflowNode<f64> for Bare {
    fn data(&self, inputs: Inputs<f64>) -> LocalBoxFuture<'_, Result<Outputs<f64>, Error>> {
        self.log.borrow_mut().push(self.id.clone());
        let sum: f64 = inputs.values().flatten().sum();
        let mut out = Outputs::new();
        for key in &self.outputs {
            out.insert(key, sum + 1.0);
        }
        ready(Ok(out))
    }
}

impl ControlFlowNode for Bare {
    fn execute(&self, _input: Option<&str>, forward: Forward<'_>) -> Result<(), Error> {
        self.log.borrow_mut().push(self.id.clone());
        for key in &self.outputs {
            forward(key)?;
        }
        Ok(())
    }
}

type Schemes = GetSchemes<Bare, Connection>;

fn ports(node: &Rc<Bare>) -> PortSelection {
    let (a, b) = (node.clone(), node.clone());
    PortSelection {
        inputs: Box::new(move || a.inputs.clone().into()),
        outputs: Box::new(move || b.outputs.clone().into()),
    }
}

fn connect(editor: &NodeEditor<Schemes>, from: &str, out: &str, to: &str, input: &str) {
    let c = Connection {
        id: format!("{from}.{out}->{to}.{input}"),
        source: from.into(),
        target: to.into(),
        source_output: out.into(),
        target_input: input.into(),
    };
    assert!(run(editor.add_connection(Rc::new(c))).unwrap());
}

#[test]
fn a_dataflow_runs_nodes_that_are_not_classic() {
    let log = Rc::default();
    let editor = NodeEditor::<Schemes>::new();
    let engine = DataflowEngine::with_configure(ports);
    editor.use_scope(&engine).unwrap();
    run(editor.add_node(Bare::new("a", &[], &["x"], &log))).unwrap();
    run(editor.add_node(Bare::new("b", &["in"], &["y"], &log))).unwrap();
    connect(&editor, "a", "x", "b", "in");

    let out = run(engine.fetch("b")).unwrap();
    assert_eq!(*out, keymap! { "y" => 2.0 });
    assert_eq!(*log.borrow(), ["a", "b"]);
}

#[test]
fn ports_the_configure_leaves_out_carry_nothing() {
    let log = Rc::default();
    let editor = NodeEditor::<Schemes>::new();
    // `b` declares one input to the dataflow; its "side" input is wired but ignored.
    let engine = DataflowEngine::with_configure(|node: &Rc<Bare>| {
        let n = node.clone();
        PortSelection {
            inputs: Box::new(move || n.inputs.iter().filter(|k| *k != "side").cloned().collect()),
            ..ports(node)
        }
    });
    editor.use_scope(&engine).unwrap();
    run(editor.add_node(Bare::new("a", &[], &["x"], &log))).unwrap();
    run(editor.add_node(Bare::new("s", &[], &["x"], &log))).unwrap();
    run(editor.add_node(Bare::new("b", &["in", "side"], &["y"], &log))).unwrap();
    connect(&editor, "a", "x", "b", "in");
    connect(&editor, "s", "x", "b", "side");

    assert_eq!(*run(engine.fetch("b")).unwrap(), keymap! { "y" => 2.0 });
    assert_eq!(*log.borrow(), ["a", "b"]);
}

#[test]
fn a_reset_follows_only_the_outputs_the_configure_picks() {
    let log = Rc::default();
    let editor = NodeEditor::<Schemes>::new();
    let engine = DataflowEngine::with_configure(|node: &Rc<Bare>| {
        let n = node.clone();
        PortSelection {
            outputs: Box::new(move || {
                n.outputs
                    .iter()
                    .filter(|k| *k != "aside")
                    .cloned()
                    .collect()
            }),
            ..ports(node)
        }
    });
    editor.use_scope(&engine).unwrap();
    run(editor.add_node(Bare::new("a", &[], &["x", "aside"], &log))).unwrap();
    run(editor.add_node(Bare::new("b", &["in"], &["y"], &log))).unwrap();
    run(editor.add_node(Bare::new("c", &["in"], &["y"], &log))).unwrap();
    connect(&editor, "a", "x", "b", "in");
    connect(&editor, "a", "aside", "c", "in");
    run(engine.fetch("b")).unwrap();
    run(engine.fetch("c")).unwrap();

    engine.reset(Some("a")).unwrap();
    assert!(engine.cache().get("b").is_none());
    assert!(engine.cache().get("c").is_some());
}

#[test]
fn a_control_flow_runs_nodes_that_are_not_classic() {
    let log = Rc::default();
    let editor = NodeEditor::<Schemes>::new();
    let engine = ControlFlowEngine::with_configure(ports);
    editor.use_scope(&engine).unwrap();
    run(editor.add_node(Bare::new("a", &[], &["then"], &log))).unwrap();
    run(editor.add_node(Bare::new("b", &["exec"], &[], &log))).unwrap();
    connect(&editor, "a", "then", "b", "exec");

    engine.execute("a", None).unwrap();
    assert_eq!(*log.borrow(), ["a", "b"]);
}
