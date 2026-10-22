// [[B1]]
//! Everything upstream `rete-engine` throws, with upstream's messages.

use std::fmt;
use std::rc::Rc;

#[derive(Clone, Debug)]
pub enum Error {
    /// Upstream's `Cancelled`: `reset` ran while the fetch was in flight.
    Cancelled,
    AlreadyProcessed,
    NotInitialized,
    /// A node's data lacks outputs its setup requires.
    MissingOutputs {
        node_id: String,
        expected: Vec<String>,
        got: Vec<String>,
    },
    /// `reset` met a node with no setup.
    NoSetup,
    /// Names the engine, as upstream's message does.
    NotAttached(&'static str),
    /// The editor an engine reads from has been dropped.
    EditorDropped,
    UnknownInput,
    UnknownOutput,
    CacheExists,
    Core(ruggine::Error),
    /// Whatever a node's own work failed with.
    Node(Rc<dyn std::error::Error>),
}

impl Error {
    pub fn node(error: impl std::error::Error + 'static) -> Self {
        Self::Node(Rc::new(error))
    }

    pub fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled)
    }
}

impl From<ruggine::Error> for Error {
    fn from(error: ruggine::Error) -> Self {
        Self::Core(error)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => f.write_str("cancelled"),
            Self::AlreadyProcessed => f.write_str("already processed"),
            Self::NotInitialized => f.write_str("node is not initialized"),
            Self::MissingOutputs {
                node_id,
                expected,
                got,
            } => write!(
                f,
                "dataflow node \"{node_id}\" doesn't return all of required properties. Expected \"{}\". Got \"{}\"",
                expected.join("\", \""),
                got.join("\", \"")
            ),
            Self::NoSetup => f.write_str("setup"),
            Self::NotAttached(engine) => write!(f, "{engine} isn't attached to NodeEditor"),
            Self::EditorDropped => f.write_str("the NodeEditor has been dropped"),
            Self::UnknownInput => f.write_str("inputs don't have a key"),
            Self::UnknownOutput => f.write_str("outputs don't have a key"),
            Self::CacheExists => f.write_str("cache already exists"),
            Self::Core(e) => e.fmt(f),
            Self::Node(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Core(e) => Some(e),
            Self::Node(e) => Some(&**e),
            _ => None,
        }
    }
}

impl PartialEq for Error {
    /// Node errors compare by identity; everything else by value.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Node(a), Self::Node(b)) => Rc::ptr_eq(a, b),
            (Self::Core(a), Self::Core(b)) => a == b,
            (
                Self::MissingOutputs {
                    node_id,
                    expected,
                    got,
                },
                Self::MissingOutputs {
                    node_id: n2,
                    expected: e2,
                    got: g2,
                },
            ) => node_id == n2 && expected == e2 && got == g2,
            _ => std::mem::discriminant(self) == std::mem::discriminant(other),
        }
    }
}
