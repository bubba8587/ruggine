// [[B1]] upstreamIsSpec
#![allow(dead_code)]

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

/// Polls `f` until it finishes, for futures that only wait on each other.
pub fn block_on<F: Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    for _ in 0..1_000_000 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            return v;
        }
    }
    panic!("future never finished");
}

/// Upstream's `await Promise.resolve()`: pending once, then ready.
pub struct Yield(bool);

impl Yield {
    pub fn once() -> Self {
        Self(false)
    }
}

impl Future for Yield {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.0 {
            Poll::Ready(())
        } else {
            self.0 = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}
