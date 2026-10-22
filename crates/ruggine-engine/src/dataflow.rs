// [[B4]] asyncNoRuntime, [[B6]] jsKeyOrder
//! Upstream's `Dataflow`: pulls a node's inputs from its predecessors, recursively. Mapping
//! notes: `tree/specs/port.md` § Dataflow.

use std::cell::RefCell;
use std::collections::HashMap;
use std::future::Future;
use std::rc::Rc;

use ruggine::classic::ClassicConnection;
use ruggine::future::{LocalBoxFuture, try_join_all};
use ruggine::{ConnectionBase, KeyMap, NodeBase, NodeEditor, Schemes, WeakNodeEditor};

use crate::Error;

/// A node's outputs by key.
pub type Outputs<V> = KeyMap<V>;
/// A node's fetched inputs: one value per connection into each key, in connection order. A key
/// with no connections is absent.
pub type Inputs<V> = KeyMap<Vec<V>>;

pub type DataFuture<V> = LocalBoxFuture<'static, Result<Rc<Outputs<V>>, Error>>;
/// Fetches the node's inputs when called; may be called more than once.
pub type FetchInputs<V> = Box<dyn Fn() -> LocalBoxFuture<'static, Result<Inputs<V>, Error>>>;
pub type Keys = Box<dyn Fn() -> Vec<String>>;

/// How the dataflow processes one node.
pub struct DataflowNodeSetup<V> {
    pub inputs: Keys,
    pub outputs: Keys,
    pub data: Box<dyn Fn(FetchInputs<V>) -> DataFuture<V>>,
}

impl<V: 'static> DataflowNodeSetup<V> {
    pub fn new<F>(
        inputs: impl Fn() -> Vec<String> + 'static,
        outputs: impl Fn() -> Vec<String> + 'static,
        data: impl Fn(FetchInputs<V>) -> F + 'static,
    ) -> Self
    where
        F: Future<Output = Result<Rc<Outputs<V>>, Error>> + 'static,
    {
        Self {
            inputs: Box::new(inputs),
            outputs: Box::new(outputs),
            data: Box::new(move |fetch| Box::pin(data(fetch))),
        }
    }
}

struct DataflowInner<S: Schemes, V> {
    editor: WeakNodeEditor<S>,
    setups: RefCell<HashMap<String, Rc<DataflowNodeSetup<V>>>>,
}

/// Upstream's `Dataflow`, as a cheap-clone handle that holds its editor weakly.
pub struct Dataflow<S: Schemes, V> {
    inner: Rc<DataflowInner<S, V>>,
}

impl<S: Schemes, V> Clone for Dataflow<S, V> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<S, V> Dataflow<S, V>
where
    S: Schemes,
    S::Connection: ClassicConnection,
    V: Clone + 'static,
{
    pub fn new(editor: &NodeEditor<S>) -> Self {
        Self {
            inner: Rc::new(DataflowInner {
                editor: editor.downgrade(),
                setups: RefCell::new(HashMap::new()),
            }),
        }
    }

    pub fn add(&self, node: &S::Node, setup: DataflowNodeSetup<V>) -> Result<(), Error> {
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

    pub fn setup(&self, node_id: &str) -> Option<Rc<DataflowNodeSetup<V>>> {
        self.inner.setups.borrow().get(node_id).cloned()
    }

    pub fn has(&self, node_id: &str) -> bool {
        self.inner.setups.borrow().contains_key(node_id)
    }

    fn editor(&self) -> Result<NodeEditor<S>, Error> {
        self.inner.editor.upgrade().ok_or(Error::EditorDropped)
    }

    /// The node's inputs, fetching every predecessor (but not running the node itself).
    pub fn fetch_inputs(&self, node_id: &str) -> LocalBoxFuture<'static, Result<Inputs<V>, Error>> {
        let this = self.clone();
        let node_id = node_id.to_owned();
        Box::pin(async move {
            let setup = this.setup(&node_id).ok_or(Error::NotInitialized)?;
            let input_keys = (setup.inputs)();
            let connections: Vec<_> = this
                .editor()?
                .get_connections()
                .into_iter()
                .filter(|c| {
                    c.target() == node_id && input_keys.iter().any(|k| k == c.target_input())
                })
                .collect();
            let fetched = try_join_all(connections.iter().map(|c| this.fetch(c.source()))).await?;
            let mut inputs = Inputs::new();
            for (c, source_data) in connections.iter().zip(fetched) {
                if inputs.get(c.target_input()).is_none() {
                    inputs.insert(c.target_input(), Vec::new());
                }
                if let Some(value) = source_data.get(c.source_output()) {
                    let slot = inputs.get_mut(c.target_input());
                    slot.expect("inserted above").push(value.clone());
                }
            }
            Ok(inputs)
        })
    }

    /// The node's outputs, running every predecessor it needs first.
    pub fn fetch(&self, node_id: &str) -> DataFuture<V> {
        let this = self.clone();
        let node_id = node_id.to_owned();
        Box::pin(async move {
            let setup = this.setup(&node_id).ok_or(Error::NotInitialized)?;
            let output_keys = (setup.outputs)();
            let fetch_inputs: FetchInputs<V> = {
                let this = this.clone();
                let node_id = node_id.clone();
                Box::new(move || this.fetch_inputs(&node_id))
            };
            let data = (setup.data)(fetch_inputs).await?;
            if !output_keys.iter().all(|k| data.contains_key(k)) {
                return Err(Error::MissingOutputs {
                    node_id,
                    expected: output_keys,
                    got: data.key_list(),
                });
            }
            Ok(data)
        })
    }
}
