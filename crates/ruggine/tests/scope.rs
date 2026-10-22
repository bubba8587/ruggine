// [[B1]] upstreamIsSpec: rete/test/scope.test.ts
use std::cell::RefCell;
use std::rc::Rc;

use ruggine::future::now_or_never;
use ruggine::{Accepts, Error, Scope};

#[derive(Clone, Debug, PartialEq)]
struct Parent {
    parent: String,
}

#[derive(Clone, Debug, PartialEq)]
struct Child {
    child: i32,
}

/// The child scope's signals: its own plus its parent's (upstream's `Child | Parent`).
#[derive(Clone, Debug, PartialEq)]
enum ChildSignal {
    Child(Child),
    Parent(Parent),
}

impl Accepts<Parent> for ChildSignal {
    fn from_parent(signal: Parent) -> Self {
        Self::Parent(signal)
    }

    fn into_parent(self) -> Result<Parent, Self> {
        match self {
            Self::Parent(p) => Ok(p),
            other => Err(other),
        }
    }
}

fn parent(s: &str) -> Parent {
    Parent { parent: s.into() }
}

fn run<F: std::future::Future>(f: F) -> F::Output {
    now_or_never(f).expect("finishes on the first poll")
}

/// A pipe that records every call and answers with `answer(input)`.
fn spy<T: Clone + 'static>(
    answer: impl Fn(T) -> Option<T> + 'static,
) -> (Rc<RefCell<Vec<T>>>, impl Fn(T) -> Option<T> + 'static) {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let log = calls.clone();
    (calls, move |ctx: T| {
        log.borrow_mut().push(ctx.clone());
        answer(ctx)
    })
}

#[test]
fn has_no_parent_by_default() {
    let scope = Scope::<Parent>::new("test");
    assert!(!scope.has_parent());
}

#[test]
fn sets_a_parent_scope() {
    use ruggine::ScopeHandle;
    let p = Scope::<Parent>::new("parent");
    let child = Scope::<ChildSignal>::new("child");
    child.set_parent(p.parent_ref());
    assert!(child.parent_scope::<Scope<Parent>>().unwrap().ptr_eq(&p));
}

#[test]
fn uses_a_nested_scope() {
    let p = Scope::<Parent>::new("parent");
    let child = Scope::<ChildSignal>::new("child");
    p.use_scope(&child).unwrap();
    assert!(child.has_parent());
    assert!(child.parent_scope::<Scope<Parent>>().unwrap().ptr_eq(&p));
}

// Upstream's "throws when using a non-Scope instance" is a compile error here: `use_scope`
// only takes a `Plugin`.

#[test]
fn parent_scope_without_a_parent_fails() {
    let scope = Scope::<Parent>::new("test");
    let err = scope.parent_scope::<Scope<Parent>>().err().unwrap();
    assert_eq!(err, Error::NoParent);
    assert_eq!(err.to_string(), "cannot find parent");
}

#[test]
fn parent_scope_with_the_wrong_type_fails() {
    let p = Scope::<Parent>::new("parent");
    let child = Scope::<ChildSignal>::new("child");
    p.use_scope(&child).unwrap();
    let err = child.parent_scope::<Scope<Child>>().err().unwrap();
    assert_eq!(err, Error::ParentTypeMismatch);
    assert_eq!(err.to_string(), "actual parent is not instance of type");
}

#[test]
fn a_dropped_parent_is_gone() {
    let child = Scope::<ChildSignal>::new("child");
    {
        let p = Scope::<Parent>::new("parent");
        p.use_scope(&child).unwrap();
        assert!(child.has_parent());
    }
    assert!(!child.has_parent());
}

#[test]
fn emits_a_signal() {
    let scope = Scope::<Parent>::new("test");
    let (calls, pipe) = spy(Some);
    scope.add_pipe(pipe);
    run(scope.emit(parent("test")));
    assert_eq!(*calls.borrow(), [parent("test")]);
}

#[test]
fn returns_the_result_of_the_pipe() {
    let scope = Scope::<Parent>::new("test");
    scope.add_pipe(|_| Some(parent("test-result")));
    assert_eq!(run(scope.emit(parent("test"))), Some(parent("test-result")));
}

#[test]
fn returns_none_if_the_pipe_returns_none() {
    let scope = Scope::<&str>::new("test");
    scope.add_pipe(|_| None);
    assert_eq!(run(scope.emit("test")), None);
}

#[test]
fn a_childs_emit_ignores_the_parents_pipes() {
    let p = Scope::<Parent>::new("parent");
    let child = Scope::<ChildSignal>::new("child");
    p.add_pipe(|_| Some(parent("test-parent")));
    p.use_scope(&child).unwrap();
    let out = run(child.emit(ChildSignal::Child(Child { child: 1 })));
    assert_eq!(out, Some(ChildSignal::Child(Child { child: 1 })));
}

#[test]
fn a_childs_emit_runs_its_own_pipes() {
    let p = Scope::<Parent>::new("parent");
    let child = Scope::<ChildSignal>::new("child");
    p.use_scope(&child).unwrap();
    child.add_pipe(|_| Some(ChildSignal::Child(Child { child: 1 })));
    let out = run(child.emit(ChildSignal::Child(Child { child: 2 })));
    assert_eq!(out, Some(ChildSignal::Child(Child { child: 1 })));
}

#[test]
fn transfers_signals_from_parent_to_child() {
    let p = Scope::<Parent>::new("parent");
    let child = Scope::<ChildSignal>::new("child");
    p.add_pipe(|_| Some(parent("test-parent")));
    let (calls, pipe) = spy(Some);
    child.add_pipe(pipe);
    p.use_scope(&child).unwrap();
    run(p.emit(parent("test-parent")));
    assert_eq!(
        *calls.borrow(),
        [ChildSignal::Parent(parent("test-parent"))]
    );
}

#[test]
fn a_parent_pipe_returning_none_stops_the_child() {
    let p = Scope::<Parent>::new("parent");
    let child = Scope::<ChildSignal>::new("child");
    p.add_pipe(|_| None);
    let (calls, pipe) = spy(Some);
    child.add_pipe(pipe);
    p.use_scope(&child).unwrap();
    run(p.emit(parent("test-parent")));
    assert!(calls.borrow().is_empty());
}

// Beyond upstream's suite.

#[test]
fn a_child_pipe_can_rewrite_the_parents_signal() {
    let p = Scope::<Parent>::new("parent");
    let child = Scope::<ChildSignal>::new("child");
    child.add_pipe(|_| Some(ChildSignal::Parent(parent("rewritten"))));
    p.use_scope(&child).unwrap();
    assert_eq!(run(p.emit(parent("x"))), Some(parent("rewritten")));
}

#[test]
fn a_child_signal_the_parent_cant_carry_stops_the_parent() {
    let p = Scope::<Parent>::new("parent");
    let child = Scope::<ChildSignal>::new("child");
    child.add_pipe(|_| Some(ChildSignal::Child(Child { child: 0 })));
    p.use_scope(&child).unwrap();
    assert_eq!(run(p.emit(parent("x"))), None);
}

#[test]
fn async_pipes_run_in_order_with_sync_ones() {
    let scope = Scope::<Vec<&'static str>>::new("test");
    scope.add_pipe(|mut v| {
        v.push("a");
        Some(v)
    });
    scope.add_async_pipe(|mut v: Vec<&'static str>| async move {
        v.push("b");
        Some(v)
    });
    scope.add_pipe(|mut v| {
        v.push("c");
        Some(v)
    });
    assert_eq!(run(scope.emit(Vec::new())), Some(vec!["a", "b", "c"]));
}

#[test]
fn a_pipe_added_during_emit_runs_in_that_emit() {
    let scope = Scope::<u32>::new("test");
    let handle = scope.clone();
    scope.add_pipe(move |n| {
        if n == 0 {
            handle.add_pipe(|n| Some(n + 10));
        }
        Some(n + 1)
    });
    assert_eq!(run(scope.emit(0)), Some(11));
}

#[test]
fn a_parent_keeps_a_used_child_alive() {
    let p = Scope::<Parent>::new("parent");
    let calls = {
        let child = Scope::<ChildSignal>::new("child");
        let (calls, pipe) = spy(Some);
        child.add_pipe(pipe);
        p.use_scope(&child).unwrap();
        calls
    };
    run(p.emit(parent("x")));
    assert_eq!(*calls.borrow(), [ChildSignal::Parent(parent("x"))]);
}
