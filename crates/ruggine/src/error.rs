// [[B1]]
//! Everything upstream `rete` throws, with upstream's messages.

use std::fmt;

/// Each variant's `Display` is upstream's message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// A node with that id is already in the editor.
    NodeAlreadyAdded,
    /// A connection with that id is already in the editor.
    ConnectionAlreadyAdded,
    /// No node with that id.
    NodeNotFound,
    /// No connection with that id.
    ConnectionNotFound,
    /// The node already has an input under that key.
    InputAlreadyAdded(String),
    /// The node already has an output under that key.
    OutputAlreadyAdded(String),
    /// The node already has a control under that key.
    ControlAlreadyAdded(String),
    /// The input already has a control.
    InputControlAlreadyAdded,
    /// A connection's source node has no output under that key.
    NoSourceOutput(String),
    /// A connection's target node has no input under that key.
    NoTargetInput(String),
    /// The scope has no parent, or its parent is gone.
    NoParent,
    /// The parent is not of the asked-for type.
    ParentTypeMismatch,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NodeAlreadyAdded => f.write_str("node has already been added"),
            Self::ConnectionAlreadyAdded => f.write_str("connection has already been added"),
            Self::NodeNotFound => f.write_str("cannot find node"),
            Self::ConnectionNotFound => f.write_str("cannot find connection"),
            Self::InputAlreadyAdded(key) => write!(f, "input with key '{key}' already added"),
            Self::OutputAlreadyAdded(key) => write!(f, "output with key '{key}' already added"),
            Self::ControlAlreadyAdded(key) => write!(f, "control with key '{key}' already added"),
            Self::InputControlAlreadyAdded => f.write_str("control already added for this input"),
            Self::NoSourceOutput(key) => {
                write!(f, "source node doesn't have output with a key {key}")
            }
            Self::NoTargetInput(key) => {
                write!(f, "target node doesn't have input with a key {key}")
            }
            Self::NoParent => f.write_str("cannot find parent"),
            Self::ParentTypeMismatch => f.write_str("actual parent is not instance of type"),
        }
    }
}

impl std::error::Error for Error {}
