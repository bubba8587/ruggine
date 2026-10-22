#![allow(dead_code)]
// [[B4]]

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

pub fn run<F: Future>(f: F) -> F::Output {
    ruggine::future::now_or_never(f).expect("finishes on the first poll")
}

/// Polls `f` once.
pub fn poll<F: Future + Unpin>(f: &mut F) -> Poll<F::Output> {
    Pin::new(f).poll(&mut Context::from_waker(Waker::noop()))
}

struct Slot<T> {
    value: Option<T>,
    waker: Option<Waker>,
}

/// Upstream's `createControlledPromise`: a future that finishes when the test resolves it.
pub struct Controlled<T>(Rc<RefCell<Slot<T>>>);

impl<T> Clone for Controlled<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> Controlled<T> {
    pub fn new() -> Self {
        Self(Rc::new(RefCell::new(Slot {
            value: None,
            waker: None,
        })))
    }

    pub fn resolve(&self, value: T) {
        let mut slot = self.0.borrow_mut();
        slot.value = Some(value);
        if let Some(w) = slot.waker.take() {
            w.wake();
        }
    }
}

impl<T> Future for Controlled<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        let mut slot = self.0.borrow_mut();
        match slot.value.take() {
            Some(v) => Poll::Ready(v),
            None => {
                slot.waker = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
}
