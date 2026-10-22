// [[B1]] upstreamIsSpec: rete-engine/test/utils/cancellable.test.ts
mod common;

use std::cell::Cell;
use std::rc::Rc;
use std::task::Poll;

use common::{Controlled, poll, run};
use ruggine_engine::{Cancellable, Error};

#[test]
fn runs_a_sequence_of_steps() {
    let c = Cancellable::new(|commit| async move {
        let n = 1;
        commit.check()?;
        let n = n + 1;
        commit.check()?;
        let n = n * 2;
        commit.check()?;
        Ok(n)
    });
    assert_eq!(run(c), Ok(4));
}

#[test]
fn runs_a_sequence_of_async_steps() {
    let c = Cancellable::new(|commit| async move {
        let n = std::future::ready(1).await;
        commit.check()?;
        let n = std::future::ready(n + 1).await;
        commit.check()?;
        let n = std::future::ready(n * 2).await;
        commit.check()?;
        Ok(n)
    });
    assert_eq!(run(c), Ok(4));
}

fn three_steps(
    first: Controlled<i32>,
    second: Controlled<i32>,
    calls: Rc<Cell<u32>>,
) -> Cancellable<i32> {
    Cancellable::new(move |commit| async move {
        let n = first.await;
        commit.check()?;
        calls.set(calls.get() + 1);
        let _ = n + 1;
        commit.check()?;
        let n = second.await;
        commit.check()?;
        Ok(n)
    })
}

#[test]
fn waits_on_nested_steps() {
    let (first, second) = (Controlled::new(), Controlled::new());
    let calls = Rc::new(Cell::new(0));
    let mut c = three_steps(first.clone(), second.clone(), calls.clone());
    assert_eq!(poll(&mut c), Poll::Pending);
    assert_eq!(calls.get(), 0);
    first.resolve(1);
    assert_eq!(poll(&mut c), Poll::Pending);
    assert_eq!(calls.get(), 1);
    second.resolve(3);
    assert_eq!(poll(&mut c), Poll::Ready(Ok(3)));
}

#[test]
fn cancel_fails_the_sequence_once_the_running_step_ends() {
    let (first, second) = (Controlled::new(), Controlled::new());
    let calls = Rc::new(Cell::new(0));
    let mut c = three_steps(first.clone(), second.clone(), calls.clone());
    assert_eq!(poll(&mut c), Poll::Pending);
    first.resolve(1);
    assert_eq!(poll(&mut c), Poll::Pending);
    assert_eq!(calls.get(), 1);
    c.cancel();
    second.resolve(3);
    assert_eq!(poll(&mut c), Poll::Ready(Err(Error::Cancelled)));
}

// Beyond upstream's suite.

#[test]
fn clones_share_one_run() {
    let calls = Rc::new(Cell::new(0));
    let counter = calls.clone();
    let c = Cancellable::new(move |_| async move {
        counter.set(counter.get() + 1);
        Ok(7)
    });
    let other = c.clone();
    assert_eq!(run(c), Ok(7));
    assert_eq!(run(other), Ok(7));
    assert_eq!(calls.get(), 1);
}

#[test]
fn cancelling_after_the_end_changes_nothing() {
    let c = Cancellable::new(|_| async { Ok(1) });
    let other = c.clone();
    assert_eq!(run(c.clone()), Ok(1));
    c.cancel();
    assert_eq!(run(other), Ok(1));
}
