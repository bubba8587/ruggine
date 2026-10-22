// [[B4]] asyncNoRuntime
//! The few future tools the port needs, written against `std` alone so no runtime is implied.

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Wake, Waker};

pub type LocalBoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// A boxed future that is already finished, for synchronous implementations of async hooks.
pub fn ready<'a, T: 'a>(value: T) -> LocalBoxFuture<'a, T> {
    Box::pin(std::future::ready(value))
}

/// Polls `future` once with a waker that does nothing. A graph whose pipes and nodes are all
/// synchronous always comes back `Some`.
pub fn now_or_never<F: Future>(future: F) -> Option<F::Output> {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => Some(value),
        Poll::Pending => None,
    }
}

/// Upstream's `Promise.all`: every future runs, outputs come back in input order.
pub fn join_all<F: Future>(futures: impl IntoIterator<Item = F>) -> JoinAll<F> {
    let slots = futures
        .into_iter()
        .map(|f| Slot::Running(Box::pin(f)))
        .collect();
    JoinAll { slots }
}

/// `Promise.all` over fallible futures: the first error to arrive wins and the rest are dropped.
pub fn try_join_all<T, E, F: Future<Output = Result<T, E>>>(
    futures: impl IntoIterator<Item = F>,
) -> TryJoinAll<F> {
    TryJoinAll {
        inner: join_all(futures),
    }
}

enum Slot<F: Future> {
    Running(Pin<Box<F>>),
    Done(F::Output),
    Taken,
}

pub struct JoinAll<F: Future> {
    slots: Vec<Slot<F>>,
}

impl<F: Future> Unpin for JoinAll<F> {}

impl<F: Future> JoinAll<F> {
    fn poll_slots(
        &mut self,
        cx: &mut Context<'_>,
        mut on_ready: impl FnMut(&F::Output) -> bool,
    ) -> Poll<bool> {
        let mut pending = false;
        for slot in &mut self.slots {
            if let Slot::Running(f) = slot {
                match f.as_mut().poll(cx) {
                    Poll::Ready(out) => {
                        let stop = on_ready(&out);
                        *slot = Slot::Done(out);
                        if stop {
                            return Poll::Ready(true);
                        }
                    }
                    Poll::Pending => pending = true,
                }
            }
        }
        if pending {
            Poll::Pending
        } else {
            Poll::Ready(false)
        }
    }

    fn take(&mut self) -> Vec<F::Output> {
        self.slots
            .iter_mut()
            .map(|slot| match std::mem::replace(slot, Slot::Taken) {
                Slot::Done(out) => out,
                _ => unreachable!("JoinAll output taken before every future finished"),
            })
            .collect()
    }
}

impl<F: Future> Future for JoinAll<F> {
    type Output = Vec<F::Output>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.poll_slots(cx, |_| false) {
            Poll::Ready(_) => Poll::Ready(self.take()),
            Poll::Pending => Poll::Pending,
        }
    }
}

pub struct TryJoinAll<F: Future> {
    inner: JoinAll<F>,
}

impl<F: Future> Unpin for TryJoinAll<F> {}

impl<T, E, F: Future<Output = Result<T, E>>> Future for TryJoinAll<F> {
    type Output = Result<Vec<T>, E>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let failed = match self.inner.poll_slots(cx, |out| out.is_err()) {
            Poll::Ready(failed) => failed,
            Poll::Pending => return Poll::Pending,
        };
        if failed {
            let failed_slot = self
                .inner
                .slots
                .iter_mut()
                .find(|slot| matches!(slot, Slot::Done(Err(_))));
            if let Some(slot) = failed_slot {
                if let Slot::Done(Err(e)) = std::mem::replace(slot, Slot::Taken) {
                    return Poll::Ready(Err(e));
                }
            }
        }
        self.inner
            .take()
            .into_iter()
            .collect::<Result<_, _>>()
            .into()
    }
}

#[derive(Default)]
struct Wakers(Mutex<Vec<Waker>>);

impl Wakers {
    fn register(&self, waker: &Waker) {
        let mut list = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if !list.iter().any(|w| w.will_wake(waker)) {
            list.push(waker.clone());
        }
    }
}

impl Wake for Wakers {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        let list = std::mem::take(&mut *self.0.lock().unwrap_or_else(|e| e.into_inner()));
        for waker in list {
            waker.wake();
        }
    }
}

enum SharedState<F: Future> {
    Pending(Pin<Box<F>>),
    Polling,
    Done(F::Output),
}

/// A future many callers can await, like a JavaScript promise held in several places. It runs
/// once; every clone gets a clone of the output. Awaiting it from inside its own work never
/// finishes, as with a promise that awaits itself.
pub struct Shared<F: Future> {
    state: Rc<RefCell<SharedState<F>>>,
    wakers: Arc<Wakers>,
}

impl<F: Future> Clone for Shared<F> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            wakers: self.wakers.clone(),
        }
    }
}

impl<F: Future> Shared<F>
where
    F::Output: Clone,
{
    pub fn new(future: F) -> Self {
        Self {
            state: Rc::new(RefCell::new(SharedState::Pending(Box::pin(future)))),
            wakers: Arc::default(),
        }
    }

    /// The output, once some caller has driven the future to the end.
    pub fn peek(&self) -> Option<F::Output> {
        match &*self.state.borrow() {
            SharedState::Done(out) => Some(out.clone()),
            _ => None,
        }
    }

    pub fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.state, &other.state)
    }
}

impl<F: Future> Unpin for Shared<F> {}

impl<F: Future> Future for Shared<F>
where
    F::Output: Clone,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut future = {
            let mut state = self.state.borrow_mut();
            match std::mem::replace(&mut *state, SharedState::Polling) {
                SharedState::Done(out) => {
                    let value = out.clone();
                    *state = SharedState::Done(out);
                    return Poll::Ready(value);
                }
                SharedState::Polling => {
                    *state = SharedState::Polling;
                    self.wakers.register(cx.waker());
                    return Poll::Pending;
                }
                SharedState::Pending(future) => future,
            }
        };
        self.wakers.register(cx.waker());
        let waker = Waker::from(self.wakers.clone());
        match future.as_mut().poll(&mut Context::from_waker(&waker)) {
            Poll::Ready(out) => {
                *self.state.borrow_mut() = SharedState::Done(out.clone());
                self.wakers.wake_by_ref();
                Poll::Ready(out)
            }
            Poll::Pending => {
                *self.state.borrow_mut() = SharedState::Pending(future);
                Poll::Pending
            }
        }
    }
}
