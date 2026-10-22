// [[B1]] upstreamIsSpec
//! Upstream's `Signal` and `Scope`: a chain of pipes, and scopes that forward their signals to
//! the child scopes they `use`. Mapping notes: `tree/specs/port.md` § Scopes.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::future::Future;
use std::rc::{Rc, Weak};

use crate::Error;
use crate::future::LocalBoxFuture;

type SyncPipe<T> = Rc<dyn Fn(T) -> Option<T>>;
type AsyncPipe<T> = Rc<dyn Fn(T) -> LocalBoxFuture<'static, Option<T>>>;

enum Pipe<T> {
    Sync(SyncPipe<T>),
    Async(AsyncPipe<T>),
}

impl<T> Clone for Pipe<T> {
    fn clone(&self) -> Self {
        match self {
            Self::Sync(f) => Self::Sync(f.clone()),
            Self::Async(f) => Self::Async(f.clone()),
        }
    }
}

type Slot<T> = Option<(PipeId, Pipe<T>)>;

/// Names one pipe on a signal, to remove it later.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PipeId(u64);

/// A middleware chain. A pipe returns the context to pass on, or `None` to stop the chain.
pub struct Signal<T> {
    /// A removed pipe leaves `None` while an emit is walking the list, so positions don't shift.
    pipes: RefCell<Vec<Slot<T>>>,
    next: Cell<u64>,
    emitting: Cell<usize>,
}

struct Emitting<'a>(&'a Cell<usize>);

impl Drop for Emitting<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}

impl<T: 'static> Signal<T> {
    pub fn new() -> Self {
        Self {
            pipes: RefCell::new(Vec::new()),
            next: Cell::new(0),
            emitting: Cell::new(0),
        }
    }

    fn push(&self, pipe: Pipe<T>) -> PipeId {
        let id = PipeId(self.next.get());
        self.next.set(id.0 + 1);
        self.pipes.borrow_mut().push(Some((id, pipe)));
        id
    }

    pub fn add_pipe(&self, pipe: impl Fn(T) -> Option<T> + 'static) -> PipeId {
        self.push(Pipe::Sync(Rc::new(pipe)))
    }

    pub fn add_async_pipe<F>(&self, pipe: impl Fn(T) -> F + 'static) -> PipeId
    where
        F: Future<Output = Option<T>> + 'static,
    {
        self.push(Pipe::Async(Rc::new(move |ctx| Box::pin(pipe(ctx)))))
    }

    /// Takes the pipe out of the chain; an emit in progress that hasn't reached it skips it.
    /// `false` if it was already gone. Upstream has no way to remove a pipe.
    pub fn remove_pipe(&self, id: PipeId) -> bool {
        let mut pipes = self.pipes.borrow_mut();
        let Some(slot) = pipes
            .iter_mut()
            .find(|p| p.as_ref().is_some_and(|(i, _)| *i == id))
        else {
            return false;
        };
        *slot = None;
        if self.emitting.get() == 0 {
            pipes.retain(Option::is_some);
        }
        true
    }

    /// Runs the context through every pipe in order; a pipe added mid-emit still runs, as in
    /// upstream's live array walk.
    pub async fn emit(&self, context: T) -> Option<T> {
        self.emitting.set(self.emitting.get() + 1);
        let _emitting = Emitting(&self.emitting);
        let mut current = context;
        let mut i = 0;
        loop {
            let slot = self.pipes.borrow().get(i).cloned();
            i += 1;
            current = match slot {
                None => return Some(current),
                Some(None) => continue,
                Some(Some((_, Pipe::Sync(f)))) => f(current)?,
                Some(Some((_, Pipe::Async(f)))) => f(current).await?,
            };
        }
    }
}

impl<T: 'static> Default for Signal<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// How a child scope's signal type takes in its parent's signals and hands them back.
/// Upstream checks this at the type level (`CanAssignSignal`); here it is a conversion.
pub trait Accepts<P>: Sized {
    fn from_parent(signal: P) -> Self;
    /// `Err` when a child pipe turned the parent's signal into one the parent can't carry; the
    /// parent's chain then stops.
    fn into_parent(self) -> Result<P, Self>;
}

impl<T> Accepts<T> for T {
    fn from_parent(signal: T) -> Self {
        signal
    }

    fn into_parent(self) -> Result<T, Self> {
        Ok(self)
    }
}

/// What a child scope keeps of its parent: a weak reference, so the parent's pipes can own the
/// child without a cycle.
#[derive(Clone)]
pub struct ParentRef(Weak<dyn Any>);

impl ParentRef {
    pub fn new<T: Any>(owner: &Rc<T>) -> Self {
        let owner: Rc<dyn Any> = owner.clone();
        Self(Rc::downgrade(&owner))
    }
}

/// A handle a child can find again as its parent ([`Scope::parent_scope`]).
pub trait ScopeHandle: Clone + 'static {
    fn parent_ref(&self) -> ParentRef;
    fn from_parent(owner: Rc<dyn Any>) -> Option<Self>;
}

/// A scope that can be attached under a parent scope whose signals are `P`. The parent keeps a
/// clone, so the plugin lives as long as its parent does.
pub trait Plugin<P>: Clone + 'static {
    type Signal: Accepts<P> + 'static;

    fn scope(&self) -> &Scope<Self::Signal>;

    /// Upstream's `setParent`, which plugins override to grab the editor.
    fn set_parent(&self, parent: ParentRef) -> Result<(), Error> {
        self.scope().set_parent(parent);
        Ok(())
    }
}

pub struct ScopeInner<T> {
    name: String,
    signal: Signal<T>,
    parent: RefCell<Option<ParentRef>>,
}

/// Upstream's `Scope`, as a cheap-clone handle.
pub struct Scope<T: 'static> {
    inner: Rc<ScopeInner<T>>,
}

impl<T> Clone for Scope<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T: 'static> Scope<T> {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            inner: Rc::new(ScopeInner {
                name: name.into(),
                signal: Signal::new(),
                parent: RefCell::new(None),
            }),
        }
    }

    pub fn name(&self) -> &str {
        &self.inner.name
    }

    pub fn signal(&self) -> &Signal<T> {
        &self.inner.signal
    }

    pub fn add_pipe(&self, pipe: impl Fn(T) -> Option<T> + 'static) -> PipeId {
        self.inner.signal.add_pipe(pipe)
    }

    pub fn add_async_pipe<F>(&self, pipe: impl Fn(T) -> F + 'static) -> PipeId
    where
        F: Future<Output = Option<T>> + 'static,
    {
        self.inner.signal.add_async_pipe(pipe)
    }

    pub fn remove_pipe(&self, id: PipeId) -> bool {
        self.inner.signal.remove_pipe(id)
    }

    pub async fn emit(&self, context: T) -> Option<T> {
        self.inner.signal.emit(context).await
    }

    /// Attaches `child` under this scope: the child learns its parent, and every signal this
    /// scope emits runs through the child's pipes at this point in the chain.
    pub fn use_scope<C: Plugin<T>>(&self, child: &C) -> Result<(), Error> {
        use_under(self, self.parent_ref(), child)
    }

    pub fn set_parent(&self, parent: ParentRef) {
        *self.inner.parent.borrow_mut() = Some(parent);
    }

    /// Whether a parent is set and still alive.
    pub fn has_parent(&self) -> bool {
        self.parent_any().is_ok()
    }

    /// Upstream's untyped `parentScope()`.
    pub fn parent_any(&self) -> Result<Rc<dyn Any>, Error> {
        self.inner
            .parent
            .borrow()
            .as_ref()
            .and_then(|p| p.0.upgrade())
            .ok_or(Error::NoParent)
    }

    /// Upstream's `parentScope(Type)`.
    pub fn parent_scope<P: ScopeHandle>(&self) -> Result<P, Error> {
        P::from_parent(self.parent_any()?).ok_or(Error::ParentTypeMismatch)
    }

    pub fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl<T: 'static> ScopeHandle for Scope<T> {
    fn parent_ref(&self) -> ParentRef {
        ParentRef::new(&self.inner)
    }

    fn from_parent(owner: Rc<dyn Any>) -> Option<Self> {
        owner
            .downcast::<ScopeInner<T>>()
            .ok()
            .map(|inner| Self { inner })
    }
}

impl<P: 'static, T: Accepts<P> + 'static> Plugin<P> for Scope<T> {
    type Signal = T;

    fn scope(&self) -> &Scope<T> {
        self
    }
}

/// `use` for any handle that owns `scope`: the child sees `parent` as its parent.
pub fn use_under<T: 'static, C: Plugin<T>>(
    scope: &Scope<T>,
    parent: ParentRef,
    child: &C,
) -> Result<(), Error> {
    child.set_parent(parent)?;
    let child = child.clone();
    scope.add_async_pipe(move |context: T| {
        let child = child.clone();
        async move {
            let signal = child.scope().emit(C::Signal::from_parent(context)).await?;
            signal.into_parent().ok()
        }
    });
    Ok(())
}
