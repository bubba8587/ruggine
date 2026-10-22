// [[A1]], [[B1]]
//! Ruggine's engines: a Rust port of [rete-engine](https://github.com/retejs/engine) (upstream
//! 2.1.1 in `upstream/rete-engine/`). Mapping notes: `tree/specs/port.md`.

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
