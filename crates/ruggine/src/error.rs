// [[B1]]
//! Everything upstream `rete` throws, with upstream's messages.

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    NodeAlreadyAdded,
    ConnectionAlreadyAdded,
    NodeNotFound,
    ConnectionNotFound,
    InputAlreadyAdded(String),
    OutputAlreadyAdded(String),
    ControlAlreadyAdded(String),
    InputControlAlreadyAdded,
    NoSourceOutput(String),
    NoTargetInput(String),
    NoParent,
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
