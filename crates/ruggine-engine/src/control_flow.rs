// [[B1]]
//! Upstream's `ControlFlow`: a node runs, and passes control on through the outputs it
//! forwards. Mapping notes: `tree/specs/port.md` § Control flow.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use ruggine::classic::ClassicConnection;
use ruggine::{ConnectionBase, NodeBase, NodeEditor, Schemes, WeakNodeEditor};

use crate::Error;
use crate::dataflow::Keys;

/// Passes control on through the named output.
pub type Forward<'a> = &'a dyn Fn(&str) -> Result<(), Error>;
type Execute = Box<dyn Fn(Option<&str>, Forward<'_>) -> Result<(), Error>>;

/// How the control flow runs one node: `execute` gets the input control arrived on (`None`
/// when the run starts here).
pub struct ControlFlowNodeSetup {
    pub inputs: Keys,
    pub outputs: Keys,
    pub execute: Execute,
}

impl ControlFlowNodeSetup {
    pub fn new(
        inputs: impl Fn() -> Vec<String> + 'static,
        outputs: impl Fn() -> Vec<String> + 'static,
        execute: impl Fn(Option<&str>, Forward<'_>) -> Result<(), Error> + 'static,
    ) -> Self {
        Self {
            inputs: Box::new(inputs),
            outputs: Box::new(outputs),
            execute: Box::new(execute),
        }
    }
}

struct ControlFlowInner<S: Schemes> {
    editor: WeakNodeEditor<S>,
    setups: RefCell<HashMap<String, Rc<ControlFlowNodeSetup>>>,
}

/// Upstream's `ControlFlow`, as a cheap-clone handle that holds its editor weakly.
pub struct ControlFlow<S: Schemes> {
    inner: Rc<ControlFlowInner<S>>,
}

impl<S: Schemes> Clone for ControlFlow<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<S> ControlFlow<S>
where
    S: Schemes,
    S::Connection: ClassicConnection,
{
    pub fn new(editor: &NodeEditor<S>) -> Self {
        Self {
            inner: Rc::new(ControlFlowInner {
                editor: editor.downgrade(),
                setups: RefCell::new(HashMap::new()),
            }),
        }
    }

    pub fn add(&self, node: &S::Node, setup: ControlFlowNodeSetup) -> Result<(), Error> {
        let mut setups = self.inner.setups.borrow_mut();
        if setups.contains_key(node.id()) {
            return Err(Error::AlreadyProcessed);
        }
        setups.insert(node.id().to_owned(), Rc::new(setup));
        Ok(())
    }

    pub fn remove(&self, node_id: &str) {
        self.inner.setups.borrow_mut().remove(node_id);
    }

    pub fn has(&self, node_id: &str) -> bool {
        self.inner.setups.borrow().contains_key(node_id)
    }

    /// Runs the node, and every node it forwards to, depth first and synchronously. An empty
    /// `input` counts as none, as upstream's truthiness check has it.
    pub fn execute(&self, node_id: &str, input: Option<&str>) -> Result<(), Error> {
        let setup = self
            .inner
            .setups
            .borrow()
            .get(node_id)
            .cloned()
            .ok_or(Error::NotInitialized)?;
        if let Some(input) = input.filter(|i| !i.is_empty()) {
            if !(setup.inputs)().iter().any(|k| k == input) {
                return Err(Error::UnknownInput);
            }
        }
        let forward = |output: &str| -> Result<(), Error> {
            if !(setup.outputs)().iter().any(|k| k == output) {
                return Err(Error::UnknownOutput);
            }
            let editor = self.inner.editor.upgrade().ok_or(Error::EditorDropped)?;
            let targets: Vec<_> = editor
                .get_connections()
                .into_iter()
                .filter(|c| c.source() == node_id && c.source_output() == output)
                .collect();
            for c in targets {
                self.execute(c.target(), Some(c.target_input()))?;
            }
            Ok(())
        };
        (setup.execute)(input, &forward)
    }
}
