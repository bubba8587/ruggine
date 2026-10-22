// [[B1]] upstreamIsSpec
//! The few future tools the port needs, written against `std` alone so no runtime is implied.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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

static NEXT_TASK: AtomicU64 = AtomicU64::new(1);
const ROOT: u64 = 0;

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// How wakeups reach a [`Scheduled`] driver, from any thread.
struct Remote {
    alive: AtomicBool,
    woken: Mutex<Vec<u64>>,
    driver: Mutex<Option<Waker>>,
}

impl Remote {
    /// `false` when the driver is gone and the caller must find another way to be polled.
    fn notify(&self, task: u64) -> bool {
        if !self.alive.load(Ordering::Acquire) {
            return false;
        }
        lock(&self.woken).push(task);
        if let Some(driver) = lock(&self.driver).as_ref() {
            driver.wake_by_ref();
        }
        true
    }
}

struct RootWake(Arc<Remote>);

impl Wake for RootWake {
    fn wake(self: Arc<Self>) {
        self.0.notify(ROOT);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.notify(ROOT);
    }
}

/// A shared future's waker: it marks the future as needing a poll, then hands it to the driver
/// running it, or else wakes everyone awaiting it so one of them polls it.
struct Wakers {
    task: u64,
    needs_poll: AtomicBool,
    awaiters: Mutex<Vec<Waker>>,
    driver: Mutex<Option<Arc<Remote>>>,
}

impl Wakers {
    fn new() -> Self {
        Self {
            task: NEXT_TASK.fetch_add(1, Ordering::Relaxed),
            needs_poll: AtomicBool::new(true),
            awaiters: Mutex::new(Vec::new()),
            driver: Mutex::new(None),
        }
    }

    fn register(&self, waker: &Waker) {
        let mut list = lock(&self.awaiters);
        if !list.iter().any(|w| w.will_wake(waker)) {
            list.push(waker.clone());
        }
    }

    fn wake_awaiters(&self) {
        let list = std::mem::take(&mut *lock(&self.awaiters));
        for waker in list {
            waker.wake();
        }
    }
}

impl Wake for Wakers {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.needs_poll.store(true, Ordering::Release);
        let driver = lock(&self.driver).clone();
        if let Some(driver) = driver {
            if driver.notify(self.task) {
                return;
            }
            *lock(&self.driver) = None;
        }
        self.wake_awaiters();
    }
}

trait Task {
    fn task_id(&self) -> u64;
    /// `false` if it was already queued.
    fn mark_queued(&self) -> bool;
    fn run(&self, remote: &Arc<Remote>) -> bool;
    /// Hands the task back when its driver goes away: no longer queued there, and whoever awaits
    /// it is woken to poll it themselves.
    fn release(&self);
}

/// The queue a [`Scheduled`] driver runs shared futures from, so that a chain of futures each
/// awaiting the next is polled one link at a time instead of one stack frame per link.
struct Scheduler {
    remote: Arc<Remote>,
    ready: RefCell<VecDeque<Rc<dyn Task>>>,
    tasks: RefCell<HashMap<u64, Rc<dyn Task>>>,
}

impl Scheduler {
    fn enqueue(&self, task: Rc<dyn Task>) {
        if task.mark_queued() {
            self.tasks.borrow_mut().insert(task.task_id(), task.clone());
            self.ready.borrow_mut().push_back(task);
        }
    }
}

thread_local! {
    static CURRENT: RefCell<Option<Rc<Scheduler>>> = const { RefCell::new(None) };
}

fn current_scheduler() -> Option<Rc<Scheduler>> {
    CURRENT.with(|c| c.borrow().clone())
}

struct Installed;

impl Installed {
    fn new(scheduler: Rc<Scheduler>) -> Self {
        CURRENT.with(|c| *c.borrow_mut() = Some(scheduler));
        Self
    }
}

impl Drop for Installed {
    fn drop(&mut self) {
        CURRENT.with(|c| *c.borrow_mut() = None);
    }
}

/// Runs `future` with every [`Shared`] it awaits, directly or through others, taken from a
/// queue, so stack depth stays flat however long the chain of shared futures is. Inside another
/// `Scheduled`, it joins that one's queue.
pub fn scheduled<F: Future>(future: F) -> Scheduled<F> {
    Scheduled {
        future: Box::pin(future),
        scheduler: Rc::new(Scheduler {
            remote: Arc::new(Remote {
                alive: AtomicBool::new(true),
                woken: Mutex::new(Vec::new()),
                driver: Mutex::new(None),
            }),
            ready: RefCell::new(VecDeque::new()),
            tasks: RefCell::new(HashMap::new()),
        }),
    }
}

pub struct Scheduled<F: Future> {
    future: Pin<Box<F>>,
    scheduler: Rc<Scheduler>,
}

impl<F: Future> Unpin for Scheduled<F> {}

impl<F: Future> Drop for Scheduled<F> {
    fn drop(&mut self) {
        self.scheduler.remote.alive.store(false, Ordering::Release);
        self.scheduler.ready.borrow_mut().clear();
        let tasks: Vec<_> = self
            .scheduler
            .tasks
            .borrow_mut()
            .drain()
            .map(|(_, t)| t)
            .collect();
        for task in tasks {
            task.release();
        }
    }
}

impl<F: Future> Future for Scheduled<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        if current_scheduler().is_some() {
            return self.future.as_mut().poll(cx);
        }
        let scheduler = self.scheduler.clone();
        let remote = scheduler.remote.clone();
        *lock(&remote.driver) = Some(cx.waker().clone());
        let _installed = Installed::new(scheduler.clone());
        let root = Waker::from(Arc::new(RootWake(remote.clone())));
        let mut poll_root = true;
        let mut output = None;
        loop {
            if poll_root && output.is_none() {
                poll_root = false;
                if let Poll::Ready(out) = self.future.as_mut().poll(&mut Context::from_waker(&root))
                {
                    output = Some(out);
                }
            }
            let woken = std::mem::take(&mut *lock(&remote.woken));
            for task in woken {
                if task == ROOT {
                    poll_root = true;
                } else if let Some(t) = scheduler.tasks.borrow().get(&task).cloned() {
                    if t.mark_queued() {
                        scheduler.ready.borrow_mut().push_back(t);
                    }
                }
            }
            let next = scheduler.ready.borrow_mut().pop_front();
            match next {
                Some(task) => {
                    if task.run(&remote) {
                        scheduler.tasks.borrow_mut().remove(&task.task_id());
                    }
                }
                None if output.is_some() && lock(&remote.woken).is_empty() => {
                    return Poll::Ready(output.take().expect("checked above"));
                }
                None if poll_root && output.is_none() => {}
                None if lock(&remote.woken).is_empty() => return Poll::Pending,
                None => {}
            }
        }
    }
}

enum SharedState<F: Future> {
    Pending(Pin<Box<F>>),
    Polling,
    Done(F::Output),
    /// The work panicked; every later await panics too instead of waiting forever.
    Poisoned,
}

/// Poisons the state unless disarmed, so a panic mid-poll can't leave it `Polling`.
struct PoisonOnUnwind<'a, F: Future>(&'a RefCell<SharedState<F>>);

impl<F: Future> Drop for PoisonOnUnwind<'_, F> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            *self.0.borrow_mut() = SharedState::Poisoned;
        }
    }
}

struct SharedInner<F: Future> {
    state: RefCell<SharedState<F>>,
    wakers: Arc<Wakers>,
    queued: Cell<bool>,
}

impl<F: Future> SharedInner<F>
where
    F::Output: Clone,
{
    /// Polls the wrapped future once; `driver` gets its wakeups if it can.
    fn poll_inner(&self, driver: Option<Arc<Remote>>) -> Poll<F::Output> {
        let mut future = {
            let mut state = self.state.borrow_mut();
            match std::mem::replace(&mut *state, SharedState::Polling) {
                SharedState::Pending(future) => future,
                other => {
                    *state = other;
                    return Poll::Pending;
                }
            }
        };
        self.wakers.needs_poll.store(false, Ordering::Release);
        *lock(&self.wakers.driver) = driver;
        let waker = Waker::from(self.wakers.clone());
        let guard = PoisonOnUnwind(&self.state);
        let polled = future.as_mut().poll(&mut Context::from_waker(&waker));
        std::mem::forget(guard);
        match polled {
            Poll::Ready(out) => {
                *self.state.borrow_mut() = SharedState::Done(out.clone());
                *lock(&self.wakers.driver) = None;
                self.wakers.wake_awaiters();
                Poll::Ready(out)
            }
            Poll::Pending => {
                *self.state.borrow_mut() = SharedState::Pending(future);
                Poll::Pending
            }
        }
    }
}

impl<F: Future> Task for SharedInner<F>
where
    F::Output: Clone,
{
    fn task_id(&self) -> u64 {
        self.wakers.task
    }

    fn mark_queued(&self) -> bool {
        !self.queued.replace(true)
    }

    fn release(&self) {
        self.queued.set(false);
        *lock(&self.wakers.driver) = None;
        self.wakers.wake_awaiters();
    }

    fn run(&self, remote: &Arc<Remote>) -> bool {
        self.queued.set(false);
        self.poll_inner(Some(remote.clone())).is_ready()
            || matches!(*self.state.borrow(), SharedState::Done(_))
    }
}

/// A future many callers can await, like a JavaScript promise held in several places. It runs
/// once; every clone gets a clone of the output. Awaiting it from inside its own work never
/// finishes, as with a promise that awaits itself. Under [`scheduled`] it is polled from the
/// driver's queue rather than inside its awaiter.
pub struct Shared<F: Future> {
    inner: Rc<SharedInner<F>>,
}

impl<F: Future> Clone for Shared<F> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<F: Future + 'static> Shared<F>
where
    F::Output: Clone,
{
    pub fn new(future: F) -> Self {
        Self {
            inner: Rc::new(SharedInner {
                state: RefCell::new(SharedState::Pending(Box::pin(future))),
                wakers: Arc::new(Wakers::new()),
                queued: Cell::new(false),
            }),
        }
    }

    /// The output, once some caller has driven the future to the end.
    pub fn peek(&self) -> Option<F::Output> {
        match &*self.inner.state.borrow() {
            SharedState::Done(out) => Some(out.clone()),
            _ => None,
        }
    }

    pub fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl<F: Future> Unpin for Shared<F> {}

impl<F: Future + 'static> Future for Shared<F>
where
    F::Output: Clone,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let inner = &self.inner;
        match &*inner.state.borrow() {
            SharedState::Done(out) => return Poll::Ready(out.clone()),
            SharedState::Poisoned => panic!("a shared future panicked while running"),
            _ => {}
        }
        inner.wakers.register(cx.waker());
        if matches!(*inner.state.borrow(), SharedState::Polling)
            || !inner.wakers.needs_poll.load(Ordering::Acquire)
        {
            return Poll::Pending;
        }
        match current_scheduler() {
            Some(scheduler) => {
                scheduler.enqueue(inner.clone());
                Poll::Pending
            }
            None => inner.poll_inner(None),
        }
    }
}
