// [[B1]] upstreamIsSpec
//! Upstream's `DataflowEngine`: a plugin that registers every node the editor adds with a
//! [`Dataflow`] and caches each node's data until `reset`.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use ruggine::classic::{ClassicConnection, ClassicNode};
use ruggine::future::{LocalBoxFuture, settle};
use ruggine::{
    ConnectionBase, NodeBase, NodeEditor, ParentRef, Plugin, Root, Schemes, Scope, WeakNodeEditor,
};

use crate::Error;
use crate::cache::Cache;
use crate::cancellable::Cancellable;
use crate::dataflow::{DataFuture, Dataflow, DataflowNodeSetup, Inputs, Keys, Outputs};
use ruggine::fxhash::FxHashSet;

/// A node that computes outputs from its inputs.
pub trait DataflowNode<V> {
    fn data(&self, inputs: Inputs<V>) -> LocalBoxFuture<'_, Result<Outputs<V>, Error>>;
}

/// Which of a node's inputs and outputs take part in the dataflow.
pub struct PortSelection {
    /// The inputs that take part.
    pub inputs: Keys,
    /// The outputs that take part.
    pub outputs: Keys,
}

type Configure<N> = Box<dyn Fn(&Rc<N>) -> PortSelection>;

struct EngineInner<S: Schemes, V: Clone + 'static> {
    scope: Scope<Root<S>>,
    configure: Option<Configure<S::Node>>,
    editor: RefCell<Option<WeakNodeEditor<S>>>,
    dataflow: RefCell<Option<Dataflow<S, V>>>,
    cache: Cache<String, Cancellable<Rc<Outputs<V>>>>,
}

/// Upstream's `DataflowEngine`, as a cheap-clone handle. Attach it with
/// `editor.use_scope(&engine)` before adding nodes.
pub struct DataflowEngine<S: Schemes, V: Clone + 'static> {
    inner: Rc<EngineInner<S, V>>,
}

impl<S: Schemes, V: Clone + 'static> Clone for DataflowEngine<S, V> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<S, V> Default for DataflowEngine<S, V>
where
    S: Schemes,
    S::Node: ClassicNode + DataflowNode<V>,
    S::Connection: ClassicConnection,
    V: Clone + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<S, V> DataflowEngine<S, V>
where
    S: Schemes,
    S::Node: ClassicNode + DataflowNode<V>,
    S::Connection: ClassicConnection,
    V: Clone + 'static,
{
    /// Every input and output of each node takes part.
    pub fn new() -> Self {
        Self::build(None)
    }

    /// `configure` picks, per node, which inputs and outputs take part.
    pub fn with_configure(configure: impl Fn(&Rc<S::Node>) -> PortSelection + 'static) -> Self {
        Self::build(Some(Box::new(configure)))
    }

    fn build(configure: Option<Configure<S::Node>>) -> Self {
        let inner = Rc::new_cyclic(|weak: &Weak<EngineInner<S, V>>| {
            let scope = Scope::new("dataflow-engine");
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
                editor: RefCell::new(None),
                dataflow: RefCell::new(None),
                cache: Cache::with_on_delete(|item: Option<Cancellable<_>>| {
                    if let Some(item) = item {
                        item.cancel();
                    }
                }),
            }
        });
        Self { inner }
    }

    fn attached(&self) -> Result<Dataflow<S, V>, Error> {
        self.dataflow().ok_or(Error::NotAttached("DataflowEngine"))
    }

    fn editor(&self) -> Result<NodeEditor<S>, Error> {
        let editor = self
            .inner
            .editor
            .borrow()
            .clone()
            .ok_or(Error::NotAttached("DataflowEngine"))?;
        editor.upgrade().ok_or(Error::EditorDropped)
    }

    fn add(&self, node: Rc<S::Node>) {
        let Ok(dataflow) = self.attached() else {
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
        let engine = Rc::downgrade(&self.inner);
        let data_node = node.clone();
        let setup = DataflowNodeSetup {
            inputs: ports.inputs,
            outputs: ports.outputs,
            data: Box::new(move |fetch_inputs| -> DataFuture<V> {
                let Some(inner) = engine.upgrade() else {
                    return Box::pin(std::future::ready(Err(Error::NotAttached(
                        "DataflowEngine",
                    ))));
                };
                let id = data_node.id().to_owned();
                if let Some(cached) = inner.cache.get(&id) {
                    return Box::pin(cached);
                }
                let node = data_node.clone();
                let cancellable = Cancellable::new(move |commit| async move {
                    let inputs = fetch_inputs().await?;
                    commit.check()?;
                    let outputs = node.data(inputs).await?;
                    commit.check()?;
                    Ok(Rc::new(outputs))
                });
                inner.cache.patch(id, cancellable.clone());
                Box::pin(cancellable)
            }),
        };
        let _ = dataflow.add(&node, setup);
    }

    fn remove(&self, node_id: &str) {
        if let Ok(dataflow) = self.attached() {
            dataflow.remove(node_id);
        }
    }

    /// The cache of each node's run, upstream's `engine.cache`: delete an entry to make the node
    /// run again, or patch in a [`Cancellable::resolved`] to give it a result without running.
    /// Deleting cancels a run in flight.
    pub fn cache(&self) -> &Cache<String, Cancellable<Rc<Outputs<V>>>> {
        &self.inner.cache
    }

    /// Upstream's `engine.dataflow`, once attached.
    pub fn dataflow(&self) -> Option<Dataflow<S, V>> {
        self.inner.dataflow.borrow().clone()
    }

    /// Clears the cached data of `node_id` and of every node downstream of it through the
    /// outputs it computes; `None` clears everything. A fetch in flight fails with
    /// [`Error::Cancelled`].
    pub fn reset(&self, node_id: Option<&str>) -> Result<(), Error> {
        let Some(start) = node_id.filter(|id| !id.is_empty()) else {
            self.inner.cache.clear();
            return Ok(());
        };
        let dataflow = self.attached()?;
        let editor = self.editor()?;
        let mut visited = FxHashSet::default();
        let mut work = vec![start.to_owned()];
        while let Some(id) = work.pop() {
            if !visited.insert(id.clone()) {
                continue;
            }
            let setup = dataflow.setup(&id).ok_or(Error::NoSetup)?;
            let output_keys = (setup.outputs)();
            self.inner.cache.delete(&id);
            let next: Vec<String> = editor
                .get_connections_from(&id)
                .iter()
                .filter(|c| output_keys.iter().any(|k| k == c.source_output()))
                .map(|c| c.target().to_owned())
                .collect();
            work.extend(next.into_iter().rev());
        }
        Ok(())
    }

    /// The node's inputs, fetching every predecessor.
    pub fn fetch_inputs(&self, node_id: &str) -> LocalBoxFuture<'static, Result<Inputs<V>, Error>> {
        match self.attached() {
            Ok(dataflow) => Box::pin(settle(dataflow.fetch_inputs(node_id))),
            Err(e) => Box::pin(std::future::ready(Err(e))),
        }
    }

    /// The node's outputs, from the cache when present.
    pub fn fetch(&self, node_id: &str) -> DataFuture<V> {
        match self.attached() {
            Ok(dataflow) => Box::pin(settle(dataflow.fetch(node_id))),
            Err(e) => Box::pin(std::future::ready(Err(e))),
        }
    }
}

impl<S, V> Plugin<Root<S>> for DataflowEngine<S, V>
where
    S: Schemes,
    S::Node: ClassicNode + DataflowNode<V>,
    S::Connection: ClassicConnection,
    V: Clone + 'static,
{
    type Signal = Root<S>;

    fn scope(&self) -> &Scope<Root<S>> {
        &self.inner.scope
    }

    fn set_parent(&self, parent: ParentRef) -> Result<(), ruggine::Error> {
        self.inner.scope.set_parent(parent);
        let editor = self.inner.scope.parent_scope::<NodeEditor<S>>()?;
        *self.inner.editor.borrow_mut() = Some(editor.downgrade());
        *self.inner.dataflow.borrow_mut() = Some(Dataflow::new(&editor));
        Ok(())
    }
}
