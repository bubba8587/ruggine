// [[B1]]
//! Upstream's scheme types: what the editor needs from a node and a connection.

use std::marker::PhantomData;

/// Upstream's `NodeId`.
pub type NodeId = String;
/// Upstream's `ConnectionId`.
pub type ConnectionId = String;

/// What the editor needs from a node, upstream's `NodeBase`. Its id must not change while it is in
/// an editor.
pub trait NodeBase {
    fn id(&self) -> &str;
}

/// What the editor needs from a connection, upstream's `ConnectionBase`. Its id must not change
/// while it is in an editor.
pub trait ConnectionBase {
    fn id(&self) -> &str;
    fn source(&self) -> &str;
    fn target(&self) -> &str;
}

/// Upstream's `{ Node, Connection }` scheme. Either type may be a trait object.
pub trait Schemes: 'static {
    /// The node type.
    type Node: NodeBase + ?Sized + 'static;
    /// The connection type.
    type Connection: ConnectionBase + ?Sized + 'static;
}

/// Upstream's `GetSchemes<Node, Connection>`, as a type that implements [`Schemes`].
pub struct GetSchemes<N: ?Sized, C: ?Sized>(PhantomData<fn() -> (*const N, *const C)>);

impl<N, C> Schemes for GetSchemes<N, C>
where
    N: NodeBase + ?Sized + 'static,
    C: ConnectionBase + ?Sized + 'static,
{
    type Node = N;
    type Connection = C;
}

/// Upstream's `BaseSchemes`: any node, any connection.
pub type BaseSchemes = GetSchemes<dyn NodeBase, dyn ConnectionBase>;
