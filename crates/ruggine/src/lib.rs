// [[A1]], [[B1]]
//! Ruggine: a Rust port of the [rete](https://github.com/retejs/rete) node editor core
//! (upstream 2.0.6 in `upstream/rete/`). Mapping notes: `tree/specs/port.md`.
//!
//! ```
//! use std::rc::Rc;
//! use ruggine::classic::{ClassicScheme, Connection, Input, Node, Output, Socket};
//! use ruggine::future::now_or_never;
//! use ruggine::{NodeBase, NodeEditor, Root};
//!
//! let editor = NodeEditor::<ClassicScheme>::new();
//! // Every change passes through the pipes; returning None cancels it.
//! let guard = editor.add_pipe(|signal| match &signal {
//!     Root::NodeCreate(node) if node.label() == "forbidden" => None,
//!     _ => Some(signal),
//! });
//!
//! let a = Rc::new(Node::new("a"));
//! a.add_output("out", Output::new(Socket::new("number")))?;
//! let b = Rc::new(Node::new("b"));
//! b.add_input("in", Input::new(Socket::new("number")))?;
//! now_or_never(editor.add_node(a.clone())).unwrap()?;
//! now_or_never(editor.add_node(b.clone())).unwrap()?;
//! let link = Connection::new(&*a, "out", &*b, "in")?;
//! now_or_never(editor.add_connection(Rc::new(link))).unwrap()?;
//! assert_eq!(editor.get_connections_to(b.id()).len(), 1);
//!
//! let added = now_or_never(editor.add_node(Rc::new(Node::new("forbidden")))).unwrap()?;
//! assert!(!added);
//! editor.remove_pipe(guard);
//! # Ok::<(), ruggine::Error>(())
//! ```

pub mod classic;
pub mod editor;
pub mod error;
pub mod future;
pub mod fxhash;
pub mod keymap;
pub mod scope;
pub mod types;
pub mod uid;

pub use editor::{NodeEditor, Root, WeakNodeEditor};
pub use error::Error;
pub use keymap::KeyMap;
pub use scope::{Accepts, ParentRef, PipeId, Plugin, Scope, ScopeHandle, Signal};
pub use types::{BaseSchemes, ConnectionBase, ConnectionId, GetSchemes, NodeBase, NodeId, Schemes};
pub use uid::get_uid;
