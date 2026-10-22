// [[B4]] asyncNoRuntime
use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use ruggine::future::{LocalBoxFuture, Shared, join_all, now_or_never, try_join_all};

/// Pending until opened, then ready with its value.
#[derive(Clone)]
struct Gate(Rc<Cell<bool>>, i32);

impl Future for Gate {
    type Output = i32;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<i32> {
        if self.0.get() {
            Poll::Ready(self.1)
        } else {
            Poll::Pending
        }
    }
}

fn poll<F: Future + Unpin>(f: &mut F) -> Poll<F::Output> {
    Pin::new(f).poll(&mut Context::from_waker(Waker::noop()))
}

#[test]
fn join_all_keeps_input_order_whatever_finishes_first() {
    let open = Rc::new(Cell::new(false));
    let first = Gate(open.clone(), 1);
    let futures: Vec<LocalBoxFuture<'_, i32>> =
        vec![Box::pin(first), Box::pin(std::future::ready(2))];
    let mut all = join_all(futures);
    assert!(poll(&mut all).is_pending());
    open.set(true);
    assert_eq!(poll(&mut all), Poll::Ready(vec![1, 2]));
}

#[test]
fn try_join_all_fails_with_the_first_error() {
    let futures = vec![
        std::future::ready(Ok(1)),
        std::future::ready(Err("a")),
        std::future::ready(Err("b")),
    ];
    assert_eq!(now_or_never(try_join_all(futures)), Some(Err("a")));
    let futures = vec![
        std::future::ready(Ok::<_, ()>(1)),
        std::future::ready(Ok(2)),
    ];
    assert_eq!(now_or_never(try_join_all(futures)), Some(Ok(vec![1, 2])));
}

#[test]
fn shared_runs_once_for_every_clone() {
    let runs = Rc::new(Cell::new(0));
    let counter = runs.clone();
    let shared = Shared::new(Box::pin(async move {
        counter.set(counter.get() + 1);
        "done"
    }) as LocalBoxFuture<'static, &str>);
    let other = shared.clone();
    assert_eq!(now_or_never(shared.clone()), Some("done"));
    assert_eq!(now_or_never(other), Some("done"));
    assert_eq!(shared.peek(), Some("done"));
    assert_eq!(runs.get(), 1);
}

#[test]
fn a_shared_future_awaiting_itself_stays_pending() {
    type Slot = Rc<RefCell<Option<Shared<LocalBoxFuture<'static, i32>>>>>;
    let slot: Slot = Rc::new(RefCell::new(None));
    let inner = slot.clone();
    let shared = Shared::new(Box::pin(async move {
        let me = inner.borrow().clone().unwrap();
        me.await + 1
    }) as LocalBoxFuture<'static, i32>);
    *slot.borrow_mut() = Some(shared.clone());
    let mut s = shared.clone();
    assert!(poll(&mut s).is_pending());
    assert!(poll(&mut s).is_pending());
    slot.borrow_mut().take();
}
