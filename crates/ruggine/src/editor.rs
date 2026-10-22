// [[B5]] sharedHandles, [[B7]] idsByLookup
//! Upstream's `NodeEditor` and its `Root` signals. Mapping notes: `tree/specs/port.md` § Editor.

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use crate::Error;
use crate::scope::{ParentRef, Plugin, Scope, ScopeHandle, use_under};
use crate::types::{ConnectionBase, NodeBase, Schemes};

/// The signals a `NodeEditor` emits; each variant is upstream's `type` string in CamelCase.
pub enum Root<S: Schemes> {
    NodeCreate(Rc<S::Node>),
    NodeCreated(Rc<S::Node>),
    NodeRemove(Rc<S::Node>),
    NodeRemoved(Rc<S::Node>),
    ConnectionCreate(Rc<S::Connection>),
    ConnectionCreated(Rc<S::Connection>),
    ConnectionRemove(Rc<S::Connection>),
    ConnectionRemoved(Rc<S::Connection>),
    Clear,
    ClearCancelled,
    Cleared,
}

impl<S: Schemes> Root<S> {
    /// Upstream's `type` field.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::NodeCreate(_) => "nodecreate",
            Self::NodeCreated(_) => "nodecreated",
            Self::NodeRemove(_) => "noderemove",
            Self::NodeRemoved(_) => "noderemoved",
            Self::ConnectionCreate(_) => "connectioncreate",
            Self::ConnectionCreated(_) => "connectioncreated",
            Self::ConnectionRemove(_) => "connectionremove",
            Self::ConnectionRemoved(_) => "connectionremoved",
            Self::Clear => "clear",
            Self::ClearCancelled => "clearcancelled",
            Self::Cleared => "cleared",
        }
    }
}

impl<S: Schemes> Clone for Root<S> {
    fn clone(&self) -> Self {
        match self {
            Self::NodeCreate(n) => Self::NodeCreate(n.clone()),
            Self::NodeCreated(n) => Self::NodeCreated(n.clone()),
            Self::NodeRemove(n) => Self::NodeRemove(n.clone()),
            Self::NodeRemoved(n) => Self::NodeRemoved(n.clone()),
            Self::ConnectionCreate(c) => Self::ConnectionCreate(c.clone()),
            Self::ConnectionCreated(c) => Self::ConnectionCreated(c.clone()),
            Self::ConnectionRemove(c) => Self::ConnectionRemove(c.clone()),
            Self::ConnectionRemoved(c) => Self::ConnectionRemoved(c.clone()),
            Self::Clear => Self::Clear,
            Self::ClearCancelled => Self::ClearCancelled,
            Self::Cleared => Self::Cleared,
        }
    }
}

impl<S: Schemes> std::fmt::Debug for Root<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.type_name())
    }
}

/// Items in insertion order beside an id index.
struct Store<T: ?Sized> {
    items: Vec<Rc<T>>,
    ids: Vec<String>,
    index: HashMap<String, usize>,
}

impl<T: ?Sized> Store<T> {
    fn new() -> Self {
        Self {
            items: Vec::new(),
            ids: Vec::new(),
            index: HashMap::new(),
        }
    }

    fn get(&self, id: &str) -> Option<Rc<T>> {
        self.index.get(id).map(|&i| self.items[i].clone())
    }

    fn push(&mut self, id: String, item: Rc<T>) {
        self.index.insert(id.clone(), self.items.len());
        self.ids.push(id);
        self.items.push(item);
    }

    /// Removes `item` itself, wherever it sits now; `false` if it is no longer stored.
    fn remove(&mut self, id: &str, item: &Rc<T>) -> bool {
        let Some(&at) = self.index.get(id) else {
            return false;
        };
        if !Rc::ptr_eq(&self.items[at], item) {
            return false;
        }
        self.index.remove(id);
        self.ids.remove(at);
        self.items.remove(at);
        for (i, id) in self.ids.iter().enumerate().skip(at) {
            self.index.insert(id.clone(), i);
        }
        true
    }
}

pub struct EditorInner<S: Schemes> {
    scope: Scope<Root<S>>,
    nodes: RefCell<Store<S::Node>>,
    connections: RefCell<Store<S::Connection>>,
}

/// Upstream's `NodeEditor`, as a cheap-clone handle.
pub struct NodeEditor<S: Schemes> {
    inner: Rc<EditorInner<S>>,
}

impl<S: Schemes> Clone for NodeEditor<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<S: Schemes> Default for NodeEditor<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: Schemes> NodeEditor<S> {
    pub fn new() -> Self {
        Self {
            inner: Rc::new(EditorInner {
                scope: Scope::new("NodeEditor"),
                nodes: RefCell::new(Store::new()),
                connections: RefCell::new(Store::new()),
            }),
        }
    }

    pub fn scope(&self) -> &Scope<Root<S>> {
        &self.inner.scope
    }

    pub fn add_pipe(&self, pipe: impl Fn(Root<S>) -> Option<Root<S>> + 'static) {
        self.inner.scope.add_pipe(pipe);
    }

    pub fn add_async_pipe<F>(&self, pipe: impl Fn(Root<S>) -> F + 'static)
    where
        F: std::future::Future<Output = Option<Root<S>>> + 'static,
    {
        self.inner.scope.add_async_pipe(pipe);
    }

    pub async fn emit(&self, context: Root<S>) -> Option<Root<S>> {
        self.inner.scope.emit(context).await
    }

    /// Upstream's `editor.use(plugin)`; the plugin's parent is this editor.
    pub fn use_scope<C: Plugin<Root<S>>>(&self, child: &C) -> Result<(), Error> {
        use_under(&self.inner.scope, self.parent_ref(), child)
    }

    pub fn get_node(&self, id: &str) -> Option<Rc<S::Node>> {
        self.inner.nodes.borrow().get(id)
    }

    /// A copy of the node list, in the order nodes were added.
    pub fn get_nodes(&self) -> Vec<Rc<S::Node>> {
        self.inner.nodes.borrow().items.clone()
    }

    pub fn get_connection(&self, id: &str) -> Option<Rc<S::Connection>> {
        self.inner.connections.borrow().get(id)
    }

    /// A copy of the connection list, in the order connections were added.
    pub fn get_connections(&self) -> Vec<Rc<S::Connection>> {
        self.inner.connections.borrow().items.clone()
    }

    /// `Ok(false)` when a `nodecreate` pipe cancels it.
    pub async fn add_node(&self, data: Rc<S::Node>) -> Result<bool, Error> {
        let id = data.id().to_owned();
        if self.get_node(&id).is_some() {
            return Err(Error::NodeAlreadyAdded);
        }
        if self.emit(Root::NodeCreate(data.clone())).await.is_none() {
            return Ok(false);
        }
        if self.get_node(&id).is_some() {
            return Err(Error::NodeAlreadyAdded);
        }
        self.inner.nodes.borrow_mut().push(id, data.clone());
        self.emit(Root::NodeCreated(data)).await;
        Ok(true)
    }

    /// `Ok(false)` when a `connectioncreate` pipe cancels it.
    pub async fn add_connection(&self, data: Rc<S::Connection>) -> Result<bool, Error> {
        let id = data.id().to_owned();
        if self.get_connection(&id).is_some() {
            return Err(Error::ConnectionAlreadyAdded);
        }
        if self
            .emit(Root::ConnectionCreate(data.clone()))
            .await
            .is_none()
        {
            return Ok(false);
        }
        if self.get_connection(&id).is_some() {
            return Err(Error::ConnectionAlreadyAdded);
        }
        self.inner.connections.borrow_mut().push(id, data.clone());
        self.emit(Root::ConnectionCreated(data)).await;
        Ok(true)
    }

    /// `Ok(false)` when a `noderemove` pipe cancels it.
    pub async fn remove_node(&self, id: &str) -> Result<bool, Error> {
        let node = self.get_node(id).ok_or(Error::NodeNotFound)?;
        if self.emit(Root::NodeRemove(node.clone())).await.is_none() {
            return Ok(false);
        }
        self.inner.nodes.borrow_mut().remove(id, &node);
        self.emit(Root::NodeRemoved(node)).await;
        Ok(true)
    }

    /// `Ok(false)` when a `connectionremove` pipe cancels it.
    pub async fn remove_connection(&self, id: &str) -> Result<bool, Error> {
        let connection = self.get_connection(id).ok_or(Error::ConnectionNotFound)?;
        if self
            .emit(Root::ConnectionRemove(connection.clone()))
            .await
            .is_none()
        {
            return Ok(false);
        }
        self.inner.connections.borrow_mut().remove(id, &connection);
        self.emit(Root::ConnectionRemoved(connection)).await;
        Ok(true)
    }

    /// Removes every connection, then every node, one event each. `Ok(false)` when a `clear`
    /// pipe cancels it, after `clearcancelled` is emitted.
    pub async fn clear(&self) -> Result<bool, Error> {
        if self.emit(Root::Clear).await.is_none() {
            self.emit(Root::ClearCancelled).await;
            return Ok(false);
        }
        for connection in self.get_connections() {
            self.remove_connection(connection.id()).await?;
        }
        for node in self.get_nodes() {
            self.remove_node(node.id()).await?;
        }
        self.emit(Root::Cleared).await;
        Ok(true)
    }

    pub fn downgrade(&self) -> WeakNodeEditor<S> {
        WeakNodeEditor(Rc::downgrade(&self.inner))
    }

    pub fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl<S: Schemes> ScopeHandle for NodeEditor<S> {
    fn parent_ref(&self) -> ParentRef {
        ParentRef::new(&self.inner)
    }

    fn from_parent(owner: Rc<dyn Any>) -> Option<Self> {
        owner
            .downcast::<EditorInner<S>>()
            .ok()
            .map(|inner| Self { inner })
    }
}

/// An editor reference that doesn't keep the editor alive.
pub struct WeakNodeEditor<S: Schemes>(Weak<EditorInner<S>>);

impl<S: Schemes> Clone for WeakNodeEditor<S> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<S: Schemes> WeakNodeEditor<S> {
    pub fn upgrade(&self) -> Option<NodeEditor<S>> {
        self.0.upgrade().map(|inner| NodeEditor { inner })
    }
}
