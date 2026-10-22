// [[B1]] upstreamIsSpec
use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use ruggine::future::{LocalBoxFuture, Shared, join_all, now_or_never, try_join_all};

/// Pending until opened, then ready with its value; opening wakes whoever polled it.
#[derive(Clone)]
struct Gate(Rc<Cell<bool>>, i32, Rc<RefCell<Option<Waker>>>);

impl Gate {
    fn new(value: i32) -> Self {
        Self(Rc::new(Cell::new(false)), value, Rc::default())
    }

    fn open(&self) {
        self.0.set(true);
        if let Some(w) = self.2.borrow_mut().take() {
            w.wake();
        }
    }
}

impl Future for Gate {
    type Output = i32;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<i32> {
        if self.0.get() {
            Poll::Ready(self.1)
        } else {
            *self.2.borrow_mut() = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

fn poll<F: Future + Unpin>(f: &mut F) -> Poll<F::Output> {
    Pin::new(f).poll(&mut Context::from_waker(Waker::noop()))
}

#[test]
fn join_all_keeps_input_order_whatever_finishes_first() {
    let first = Gate::new(1);
    let gate = first.clone();
    let futures: Vec<LocalBoxFuture<'_, i32>> =
        vec![Box::pin(first), Box::pin(std::future::ready(2))];
    let mut all = join_all(futures);
    assert!(poll(&mut all).is_pending());
    gate.open();
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

#[test]
fn a_shared_future_that_panicked_panics_for_every_awaiter() {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    let shared = Shared::new(Box::pin(async {
        if true {
            panic!("boom");
        }
        0
    }) as LocalBoxFuture<'static, i32>);
    let first = catch_unwind(AssertUnwindSafe(|| now_or_never(shared.clone())));
    assert!(first.is_err());
    let again = catch_unwind(AssertUnwindSafe(|| now_or_never(shared.clone())));
    assert!(again.is_err(), "a later await must not hang");
}

#[test]
fn a_long_chain_of_shared_futures_runs_on_a_flat_stack() {
    type Link = Shared<LocalBoxFuture<'static, u64>>;
    let mut prev: Option<Link> = None;
    for _ in 0..200_000 {
        let before = prev.clone();
        prev = Some(Shared::new(Box::pin(async move {
            match before {
                Some(p) => p.await + 1,
                None => 0,
            }
        }) as LocalBoxFuture<'static, u64>));
    }
    let last = prev.unwrap();
    assert_eq!(now_or_never(last), Some(199_999));
}

#[test]
fn work_a_dropped_poller_started_is_finished_by_the_next() {
    let gate = Gate::new(5);
    let opener = gate.clone();
    let base = Shared::new(Box::pin(gate) as LocalBoxFuture<'static, i32>);
    let (b1, b2) = (base.clone(), base.clone());
    let first = Shared::new(Box::pin(async move { b1.await + 1 }) as LocalBoxFuture<'static, i32>);
    let second = Shared::new(Box::pin(async move { b2.await + 2 }) as LocalBoxFuture<'static, i32>);

    let mut poller = first;
    assert!(poll(&mut poller).is_pending());
    drop(poller);

    opener.open();
    assert_eq!(now_or_never(second.clone()), Some(7));
    assert_eq!(now_or_never(base), Some(5));
}

#[test]
fn work_queued_beside_a_panic_still_runs_later() {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    let boom = Shared::new(Box::pin(async {
        if true {
            panic!("boom");
        }
        0
    }) as LocalBoxFuture<'static, i32>);
    let queued = Shared::new(Box::pin(async { 3 }) as LocalBoxFuture<'static, i32>);
    let both = join_all(vec![boom, queued.clone()]);
    // `boom` runs first and panics while `queued` waits in the same driver's queue.
    let crashed = catch_unwind(AssertUnwindSafe(|| now_or_never(both)));
    assert!(crashed.is_err());
    assert_eq!(now_or_never(queued), Some(3));
}

#[test]
fn a_spawned_shared_future_runs_without_being_awaited() {
    let ran = Rc::new(Cell::new(false));
    let flag = ran.clone();
    let _spawned = Shared::spawn(Box::pin(async move {
        flag.set(true);
    }) as LocalBoxFuture<'static, ()>);
    assert!(!ran.get());
    let other = Shared::new(Box::pin(async { 1 }) as LocalBoxFuture<'static, i32>);
    assert_eq!(now_or_never(other), Some(1));
    assert!(ran.get(), "the queue runs dry whenever it runs");
}

#[test]
fn a_blocking_await_inside_queued_work_finishes() {
    let inner = Shared::spawn(Box::pin(async { 2 }) as LocalBoxFuture<'static, i32>);
    let outer = Shared::new(Box::pin(async move { now_or_never(inner).map(|v| v + 1) })
        as LocalBoxFuture<'static, Option<i32>>);
    assert_eq!(now_or_never(outer), Some(Some(3)));
}

#[test]
fn a_poll_from_outside_the_queue_is_woken_when_shared_work_is_poisoned() {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::Wake;
    struct Count(AtomicUsize);
    impl Wake for Count {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let gate = Gate::new(0);
    let opener = gate.clone();
    let boom = Shared::new(Box::pin(async move {
        if gate.await == 0 {
            panic!("boom");
        }
        0
    }) as LocalBoxFuture<'static, i32>);
    let count = Arc::new(Count(AtomicUsize::new(0)));
    let waker = Waker::from(count.clone());
    let mut waiting = boom.clone();
    assert!(
        Pin::new(&mut waiting)
            .poll(&mut Context::from_waker(&waker))
            .is_pending()
    );
    opener.open();
    let crashed = catch_unwind(AssertUnwindSafe(|| now_or_never(boom.clone())));
    assert!(crashed.is_err());
    assert!(
        count.0.load(Ordering::SeqCst) > 0,
        "the other waiter hears about it"
    );
}
