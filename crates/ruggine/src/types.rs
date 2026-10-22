// [[B1]]
//! Upstream's scheme types: what the editor needs from a node and a connection.

use std::marker::PhantomData;

pub type NodeId = String;
pub type ConnectionId = String;

/// A node's id must not change while the node is in an editor.
pub trait NodeBase {
    fn id(&self) -> &str;
}

pub trait ConnectionBase {
    fn id(&self) -> &str;
    fn source(&self) -> &str;
    fn target(&self) -> &str;
}

/// Upstream's `{ Node, Connection }` scheme. Either type may be a trait object.
pub trait Schemes: 'static {
    type Node: NodeBase + ?Sized + 'static;
    type Connection: ConnectionBase + ?Sized + 'static;
}

pub struct GetSchemes<N: ?Sized, C: ?Sized>(PhantomData<fn() -> (*const N, *const C)>);

impl<N, C> Schemes for GetSchemes<N, C>
where
    N: NodeBase + ?Sized + 'static,
    C: ConnectionBase + ?Sized + 'static,
{
    type Node = N;
    type Connection = C;
}

pub type BaseSchemes = GetSchemes<dyn NodeBase, dyn ConnectionBase>;
