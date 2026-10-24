// [[B1]] upstreamIsSpec
//! The few future tools the port needs, written against `std` alone so no runtime is implied.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Wake, Waker};

use crate::fxhash::FxHashMap as HashMap;

/// A boxed, single-threaded future.
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

/// See [`join_all`].
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

/// See [`try_join_all`].
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

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// How wakeups reach a thread's queue from anywhere: which tasks woke, and the outside pollers
/// waiting for the queue to make progress.
struct Remote {
    woken: Mutex<Vec<u64>>,
    parked: Mutex<Vec<Waker>>,
}

impl Remote {
    fn notify(&self, task: u64) {
        lock(&self.woken).push(task);
        self.wake_parked();
    }

    fn wake_parked(&self) {
        let parked = std::mem::take(&mut *lock(&self.parked));
        for waker in parked {
            waker.wake();
        }
    }

    fn park(&self, waker: &Waker) {
        let mut parked = lock(&self.parked);
        if !parked.iter().any(|w| w.will_wake(waker)) {
            parked.push(waker.clone());
        }
    }
}

/// A shared future's waker: it marks the future as needing a poll and queues it on its thread.
struct Wakers {
    task: u64,
    needs_poll: AtomicBool,
    awaiters: Mutex<Vec<Waker>>,
    home: Arc<Remote>,
}

impl Wakers {
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
        self.home.notify(self.task);
    }
}

trait Task {
    fn task_id(&self) -> u64;
    /// `false` if it was already queued.
    fn mark_queued(&self) -> bool;
    /// Polls the task once; `true` when it has finished.
    fn run(&self, queue: &Queue) -> bool;
}

/// One thread's queue of shared futures, standing in for JavaScript's microtask queue: work is
/// queued when created or woken and run first in, first out, one task at a time, so a chain of
/// futures each awaiting the next never nests on the stack.
struct Queue {
    remote: Arc<Remote>,
    ready: RefCell<VecDeque<Rc<dyn Task>>>,
    tasks: RefCell<HashMap<u64, Weak<dyn Task>>>,
    /// The task map's size after its last sweep; the next sweep waits until it has doubled.
    swept: Cell<usize>,
    /// The wakers of the tasks being run, innermost last.
    running: RefCell<Vec<Waker>>,
}

thread_local! {
    static QUEUE: Rc<Queue> = Rc::new(Queue {
        remote: Arc::new(Remote {
            woken: Mutex::new(Vec::new()),
            parked: Mutex::new(Vec::new()),
        }),
        ready: RefCell::new(VecDeque::new()),
        tasks: RefCell::new(HashMap::default()),
        swept: Cell::new(0),
        running: RefCell::new(Vec::new()),
    });
}

fn queue() -> Rc<Queue> {
    QUEUE.with(Rc::clone)
}

struct Running<'a>(&'a Queue);

impl Drop for Running<'_> {
    fn drop(&mut self) {
        self.0.running.borrow_mut().pop();
    }
}

impl Queue {
    fn enqueue(&self, task: Rc<dyn Task>) {
        if task.mark_queued() {
            let mut tasks = self.tasks.borrow_mut();
            if tasks.len() >= 1024.max(2 * self.swept.get()) {
                tasks.retain(|_, t| t.strong_count() > 0);
                self.swept.set(tasks.len());
            }
            tasks.insert(task.task_id(), Rc::downgrade(&task));
            drop(tasks);
            self.ready.borrow_mut().push_back(task);
            if self.running.borrow().is_empty() {
                self.remote.wake_parked();
            }
        }
    }

    /// Whether `waker` belongs to the task this queue is running: an await inside queued work,
    /// which only needs to queue what it awaits.
    fn in_task(&self, waker: &Waker) -> bool {
        self.running
            .borrow()
            .last()
            .is_some_and(|w| w.will_wake(waker))
    }

    fn take_woken(&self) {
        let woken = std::mem::take(&mut *lock(&self.remote.woken));
        for id in woken {
            let task = self.tasks.borrow().get(&id).and_then(Weak::upgrade);
            match task {
                Some(task) => self.enqueue(task),
                None => {
                    self.tasks.borrow_mut().remove(&id);
                }
            }
        }
    }

    /// Runs the next queued task; `false` if there was none.
    fn run_one(&self) -> bool {
        self.take_woken();
        let next = self.ready.borrow_mut().pop_front();
        let Some(task) = next else {
            return false;
        };
        if task.run(self) {
            self.tasks.borrow_mut().remove(&task.task_id());
        }
        true
    }

    /// Runs what is queued right now and nothing queued after: where JavaScript would resume a
    /// caller whose promise just settled, behind the microtasks already waiting.
    fn run_queued_now(&self) {
        self.take_woken();
        let waiting = self.ready.borrow().len();
        for _ in 0..waiting {
            self.run_one();
        }
    }

    /// Runs queued work until `done` holds (then [`Queue::run_queued_now`]) or nothing is left.
    fn drive(&self, done: &dyn Fn() -> bool) {
        while !done() {
            if !self.run_one() {
                return;
            }
        }
        self.run_queued_now();
    }
}

/// A future that never finishes and runs this thread's queue dry every time work arrives: what
/// a JavaScript host's event loop does for microtasks. Spawn it once on the host's executor
/// (`wasm_bindgen_futures::spawn_local(background())`) and work nobody awaits any more, such as a
/// sibling of a fetch that failed, carries on as it would in JavaScript. Without it, such work
/// waits for the next await from outside the queue.
pub fn background() -> Background {
    Background(())
}

/// See [`background`].
pub struct Background(());

impl Future for Background {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let queue = queue();
        if queue.in_task(cx.waker()) {
            return Poll::Pending;
        }
        queue.remote.park(cx.waker());
        while queue.run_one() {}
        Poll::Pending
    }
}

/// Polls `future` to the end of the queued work it sets off: when it finishes, every task it
/// queued and left behind runs too before this returns, as JavaScript runs its microtask queue
/// dry. Awaited inside queued work, it is just `future`.
pub fn settle<F: Future>(future: F) -> Settle<F> {
    Settle {
        future: Box::pin(future),
    }
}

/// See [`settle`].
pub struct Settle<F: Future> {
    future: Pin<Box<F>>,
}

impl<F: Future> Unpin for Settle<F> {}

impl<F: Future> Future for Settle<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        let queue = queue();
        if queue.in_task(cx.waker()) {
            return self.future.as_mut().poll(cx);
        }
        queue.remote.park(cx.waker());
        match self.future.as_mut().poll(cx) {
            Poll::Ready(out) => {
                queue.run_queued_now();
                Poll::Ready(out)
            }
            Poll::Pending => Poll::Pending,
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

/// Poisons the state if the poll unwinds, and wakes everyone waiting so they see it.
struct PoisonOnUnwind<'a, F: Future>(&'a RefCell<SharedState<F>>, &'a Wakers);

impl<F: Future> Drop for PoisonOnUnwind<'_, F> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            *self.0.borrow_mut() = SharedState::Poisoned;
            self.1.wake_awaiters();
        }
    }
}

struct SharedInner<F: Future> {
    state: RefCell<SharedState<F>>,
    wakers: Arc<Wakers>,
    waker: Waker,
    queued: Cell<bool>,
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

    fn run(&self, queue: &Queue) -> bool {
        self.queued.set(false);
        let mut future = {
            let mut state = self.state.borrow_mut();
            match std::mem::replace(&mut *state, SharedState::Polling) {
                SharedState::Pending(future) => future,
                other => {
                    let finished = matches!(other, SharedState::Done(_) | SharedState::Poisoned);
                    *state = other;
                    return finished;
                }
            }
        };
        self.wakers.needs_poll.store(false, Ordering::Release);
        queue.running.borrow_mut().push(self.waker.clone());
        let running = Running(queue);
        let guard = PoisonOnUnwind(&self.state, &self.wakers);
        let polled = future.as_mut().poll(&mut Context::from_waker(&self.waker));
        std::mem::forget(guard);
        drop(running);
        match polled {
            Poll::Ready(out) => {
                *self.state.borrow_mut() = SharedState::Done(out);
                self.wakers.wake_awaiters();
                true
            }
            Poll::Pending => {
                *self.state.borrow_mut() = SharedState::Pending(future);
                if self.wakers.needs_poll.load(Ordering::Acquire) {
                    // Woken while it ran, possibly by a nested drain that found it busy.
                    lock(&self.wakers.home.woken).push(self.wakers.task);
                }
                false
            }
        }
    }
}

/// A future many callers can await, like a JavaScript promise held in several places. It runs
/// once, on its thread's queue; every clone gets a clone of the output. Awaiting it from inside
/// its own work never finishes, as with a promise that awaits itself.
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
    /// Starts when first awaited.
    pub fn new(future: F) -> Self {
        Self::new_in(SharedState::Pending(Box::pin(future)))
    }

    fn new_in(state: SharedState<F>) -> Self {
        let wakers = Arc::new(Wakers {
            task: NEXT_TASK.fetch_add(1, Ordering::Relaxed),
            needs_poll: AtomicBool::new(true),
            awaiters: Mutex::new(Vec::new()),
            home: queue().remote.clone(),
        });
        Self {
            inner: Rc::new(SharedInner {
                state: RefCell::new(state),
                waker: Waker::from(wakers.clone()),
                wakers,
                queued: Cell::new(false),
            }),
        }
    }

    /// Already finished with `value`, like `Promise.resolve(value)`.
    pub fn ready(value: F::Output) -> Self {
        let shared = Self::new_in(SharedState::Done(value));
        shared
            .inner
            .wakers
            .needs_poll
            .store(false, Ordering::Release);
        shared
    }

    /// Queued at once, like a JavaScript promise: it runs the next time the thread's queue runs,
    /// awaited or not.
    pub fn spawn(future: F) -> Self {
        let shared = Self::new(future);
        queue().enqueue(shared.inner.clone());
        shared
    }

    /// The output, once the work has finished.
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

    /// Inside queued work this only queues the shared work; from anywhere else it runs the
    /// thread's queue until nothing is left, then looks again.
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let inner = &self.inner;
        let look = || match &*inner.state.borrow() {
            SharedState::Done(out) => Some(out.clone()),
            SharedState::Poisoned => panic!("a shared future panicked while running"),
            _ => None,
        };
        if let Some(out) = look() {
            return Poll::Ready(out);
        }
        inner.wakers.register(cx.waker());
        let queue = queue();
        if inner.wakers.needs_poll.load(Ordering::Acquire)
            && !matches!(*inner.state.borrow(), SharedState::Polling)
        {
            queue.enqueue(inner.clone());
        }
        if queue.in_task(cx.waker()) {
            return Poll::Pending;
        }
        queue.remote.park(cx.waker());
        queue.drive(&|| {
            matches!(
                *inner.state.borrow(),
                SharedState::Done(_) | SharedState::Poisoned
            )
        });
        match look() {
            Some(out) => Poll::Ready(out),
            None => Poll::Pending,
        }
    }
}
