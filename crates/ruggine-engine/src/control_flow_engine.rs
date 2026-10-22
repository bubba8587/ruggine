// [[B1]] upstreamIsSpec
//! Upstream's `ControlFlowEngine`: a plugin that registers every node the editor adds with a
//! [`ControlFlow`].

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use ruggine::classic::{ClassicConnection, ClassicNode};
use ruggine::{NodeBase, NodeEditor, ParentRef, Plugin, Root, Schemes, Scope};

use crate::Error;
use crate::control_flow::{ControlFlow, ControlFlowNodeSetup, Forward};
use crate::dataflow_engine::PortSelection;

/// A node that runs when control reaches it; `input` is `None` when the run starts here.
pub trait ControlFlowNode {
    fn execute(&self, input: Option<&str>, forward: Forward<'_>) -> Result<(), Error>;
}

type Configure<N> = Box<dyn Fn(&Rc<N>) -> PortSelection>;

struct EngineInner<S: Schemes> {
    scope: Scope<Root<S>>,
    configure: Option<Configure<S::Node>>,
    control_flow: RefCell<Option<ControlFlow<S>>>,
}

/// Upstream's `ControlFlowEngine`, as a cheap-clone handle. Attach it with
/// `editor.use_scope(&engine)` before adding nodes.
pub struct ControlFlowEngine<S: Schemes> {
    inner: Rc<EngineInner<S>>,
}

impl<S: Schemes> Clone for ControlFlowEngine<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<S> Default for ControlFlowEngine<S>
where
    S: Schemes,
    S::Node: ClassicNode + ControlFlowNode,
    S::Connection: ClassicConnection,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<S> ControlFlowEngine<S>
where
    S: Schemes,
    S::Node: ClassicNode + ControlFlowNode,
    S::Connection: ClassicConnection,
{
    /// Every input and output of each node takes part.
    pub fn new() -> Self {
        Self::build(None)
    }

    /// `configure` picks, per node, which inputs and outputs carry control.
    pub fn with_configure(configure: impl Fn(&Rc<S::Node>) -> PortSelection + 'static) -> Self {
        Self::build(Some(Box::new(configure)))
    }

    fn build(configure: Option<Configure<S::Node>>) -> Self {
        let inner = Rc::new_cyclic(|weak: &Weak<EngineInner<S>>| {
            let scope = Scope::new("control-flow-engine");
            let engine = weak.clone();
            scope.add_pipe(move |context: Root<S>| {
                if let Some(inner) = engine.upgrade() {
                    let this = Self { inner };
                    match &context {
                        Root::NodeCreated(node) => this.add(node.clone()),
                        Root::NodeRemoved(node) => this.remove(node.id()),
                        _ => {}
                    }
                }
                Some(context)
            });
            EngineInner {
                scope,
                configure,
                control_flow: RefCell::new(None),
            }
        });
        Self { inner }
    }

    /// Upstream's `engine.controlflow`, once attached.
    pub fn control_flow(&self) -> Option<ControlFlow<S>> {
        self.inner.control_flow.borrow().clone()
    }

    fn attached(&self) -> Result<ControlFlow<S>, Error> {
        self.control_flow()
            .ok_or(Error::NotAttached("ControlFlowEngine"))
    }

    fn add(&self, node: Rc<S::Node>) {
        let Ok(control_flow) = self.attached() else {
            return;
        };
        let ports = match &self.inner.configure {
            Some(configure) => configure(&node),
            None => {
                let (a, b) = (node.clone(), node.clone());
                PortSelection {
                    inputs: Box::new(move || a.classic().input_keys()),
                    outputs: Box::new(move || b.classic().output_keys()),
                }
            }
        };
        let target = node.clone();
        let setup = ControlFlowNodeSetup {
            inputs: ports.inputs,
            outputs: ports.outputs,
            execute: Box::new(move |input, forward| target.execute(input, forward)),
        };
        let _ = control_flow.add(&node, setup);
    }

    fn remove(&self, node_id: &str) {
        if let Ok(control_flow) = self.attached() {
            control_flow.remove(node_id);
        }
    }

    /// Starts a run at `node_id`, as if control arrived on `input`.
    pub fn execute(&self, node_id: &str, input: Option<&str>) -> Result<(), Error> {
        self.attached()?.execute(node_id, input)
    }
}

impl<S> Plugin<Root<S>> for ControlFlowEngine<S>
where
    S: Schemes,
    S::Node: ClassicNode + ControlFlowNode,
    S::Connection: ClassicConnection,
{
    type Signal = Root<S>;

    fn scope(&self) -> &Scope<Root<S>> {
        &self.inner.scope
    }

    fn set_parent(&self, parent: ParentRef) -> Result<(), ruggine::Error> {
        self.inner.scope.set_parent(parent);
        let editor = self.inner.scope.parent_scope::<NodeEditor<S>>()?;
        *self.inner.control_flow.borrow_mut() = Some(ControlFlow::new(&editor));
        Ok(())
    }
}
