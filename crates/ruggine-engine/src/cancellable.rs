// [[B1]] upstreamIsSpec
//! Upstream's `createCancellblePromise`: steps run in order, and once `cancel` is called the
//! sequence fails with [`Error::Cancelled`] as soon as the running step finishes.

use std::cell::Cell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

use ruggine::future::{LocalBoxFuture, Shared};

use crate::Error;

/// Handed to the steps: `check` after each one is upstream's `commit`.
#[derive(Clone)]
pub struct Commit(Rc<Cell<bool>>);

impl Commit {
    pub fn check(&self) -> Result<(), Error> {
        if self.0.get() {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// A shared, cancellable result: every clone awaits the same work.
pub struct Cancellable<T: Clone + 'static> {
    shared: Shared<LocalBoxFuture<'static, Result<T, Error>>>,
    cancelled: Rc<Cell<bool>>,
}

impl<T: Clone + 'static> Clone for Cancellable<T> {
    fn clone(&self) -> Self {
        Self {
            shared: self.shared.clone(),
            cancelled: self.cancelled.clone(),
        }
    }
}

impl<T: Clone + 'static> Cancellable<T> {
    /// `steps` gets the [`Commit`] to check after each step it awaits.
    pub fn new<F>(steps: impl FnOnce(Commit) -> F) -> Self
    where
        F: Future<Output = Result<T, Error>> + 'static,
    {
        let cancelled = Rc::new(Cell::new(false));
        let future: LocalBoxFuture<'static, Result<T, Error>> =
            Box::pin(steps(Commit(cancelled.clone())));
        Self {
            shared: Shared::new(future),
            cancelled,
        }
    }

    /// No effect once the work has finished.
    pub fn cancel(&self) {
        self.cancelled.set(true);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.get()
    }

    pub fn ptr_eq(&self, other: &Self) -> bool {
        self.shared.ptr_eq(&other.shared)
    }
}

impl<T: Clone + 'static> Future for Cancellable<T> {
    type Output = Result<T, Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.shared).poll(cx)
    }
}
