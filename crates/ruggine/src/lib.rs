// [[A1]], [[B1]]
//! Ruggine: a Rust port of the [rete](https://github.com/retejs/rete) node editor core
//! (upstream 2.0.6 in `upstream/rete/`). Mapping notes: `tree/specs/port.md`.

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
pub use scope::{Accepts, ParentRef, Plugin, Scope, ScopeHandle, Signal};
pub use types::{BaseSchemes, ConnectionBase, ConnectionId, GetSchemes, NodeBase, NodeId, Schemes};
pub use uid::get_uid;
