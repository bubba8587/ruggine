// [[A1]], [[B1]]
//! Ruggine's engines: a Rust port of [rete-engine](https://github.com/retejs/engine) (upstream
//! 2.1.1 in `upstream/rete-engine/`). Mapping notes: `tree/specs/port.md`.
//!
//! ```
//! use std::rc::Rc;
//! use ruggine::classic::{ClassicNode, Connection, Input, Node, Output, Socket};
//! use ruggine::future::{now_or_never, ready, LocalBoxFuture};
//! use ruggine::{keymap, GetSchemes, NodeBase, NodeEditor};
//! use ruggine_engine::{DataflowEngine, DataflowNode, Error, Inputs, Outputs};
//!
//! /// Adds 1 to whatever arrives on `in`.
//! struct Inc(Node);
//!
//! impl NodeBase for Inc {
//!     fn id(&self) -> &str { &self.0.id }
//! }
//! impl ClassicNode for Inc {
//!     type Socket = Socket;
//!     fn classic(&self) -> &Node { &self.0 }
//! }
//! impl DataflowNode<f64> for Inc {
//!     fn data(&self, inputs: Inputs<f64>) -> LocalBoxFuture<'_, Result<Outputs<f64>, Error>> {
//!         let x = inputs.get("in").and_then(|v| v.first().copied()).unwrap_or(0.0);
//!         ready(Ok(keymap! { "out" => x + 1.0 }))
//!     }
//! }
//!
//! fn inc() -> Rc<Inc> {
//!     let node = Node::new("inc");
//!     node.add_input("in", Input::new(Socket::new("number"))).unwrap();
//!     node.add_output("out", Output::new(Socket::new("number"))).unwrap();
//!     Rc::new(Inc(node))
//! }
//!
//! type Schemes = GetSchemes<Inc, Connection>;
//! let editor = NodeEditor::<Schemes>::new();
//! let engine = DataflowEngine::<Schemes, f64>::new();
//! editor.use_scope(&engine).unwrap();
//!
//! let (a, b) = (inc(), inc());
//! // Every node here is synchronous, so each future is done on its first poll; an app with
//! // async nodes awaits these on its own executor instead.
//! now_or_never(editor.add_node(a.clone())).unwrap()?;
//! now_or_never(editor.add_node(b.clone())).unwrap()?;
//! let link = Connection::new(&*a, "out", &*b, "in")?;
//! now_or_never(editor.add_connection(Rc::new(link))).unwrap()?;
//!
//! let out = now_or_never(engine.fetch(b.id())).unwrap().unwrap();
//! assert_eq!(out["out"], 2.0);
//!
//! engine.reset(Some(a.id())).unwrap();
//! # Ok::<(), ruggine::Error>(())
//! ```

pub mod cache;
pub mod cancellable;
pub mod control_flow;
pub mod control_flow_engine;
pub mod dataflow;
pub mod dataflow_engine;
pub mod error;

pub use cache::Cache;
pub use cancellable::{Cancellable, Commit};
pub use control_flow::{ControlFlow, ControlFlowNodeSetup, Forward};
pub use control_flow_engine::{ControlFlowEngine, ControlFlowNode};
pub use dataflow::{Dataflow, DataflowNodeSetup, FetchInputs, Inputs, Outputs};
pub use dataflow_engine::{DataflowEngine, DataflowNode, PortSelection};
pub use error::Error;
