// [[B1]] upstreamIsSpec: rete/test/index.test.ts
mod common;

use std::rc::Rc;

use common::{Yield, block_on};
use ruggine::future::{join_all, now_or_never};
use ruggine::{BaseSchemes, ConnectionBase, Error, NodeBase, NodeEditor, Root};

struct N {
    id: String,
}

impl NodeBase for N {
    fn id(&self) -> &str {
        &self.id
    }
}

struct C {
    id: String,
    source: String,
    target: String,
}

impl ConnectionBase for C {
    fn id(&self) -> &str {
        &self.id
    }
    fn source(&self) -> &str {
        &self.source
    }
    fn target(&self) -> &str {
        &self.target
    }
}

fn node(id: &str) -> Rc<dyn NodeBase> {
    Rc::new(N { id: id.into() })
}

fn conn(id: &str, source: &str, target: &str) -> Rc<dyn ConnectionBase> {
    Rc::new(C {
        id: id.into(),
        source: source.into(),
        target: target.into(),
    })
}

fn run<F: std::future::Future>(f: F) -> F::Output {
    now_or_never(f).expect("a synchronous graph finishes on the first poll")
}

type Editor = NodeEditor<BaseSchemes>;

/// An editor whose every event waits a turn, so concurrent calls interleave as upstream's do
/// (each of its `emit`s awaits).
fn yielding_editor() -> Editor {
    let editor = Editor::new();
    editor.add_async_pipe(|ctx| async move {
        Yield::once().await;
        Some(ctx)
    });
    editor
}

#[test]
fn add_node_adds_a_node() {
    let editor = Editor::new();
    let data = node("1");
    assert_eq!(run(editor.add_node(data.clone())), Ok(true));
    let nodes = editor.get_nodes();
    assert_eq!(nodes.len(), 1);
    assert!(Rc::ptr_eq(&nodes[0], &data));
}

#[test]
fn add_node_refuses_a_duplicate_id() {
    let editor = Editor::new();
    let data = node("1");
    run(editor.add_node(data.clone())).unwrap();
    assert_eq!(run(editor.add_node(data)), Err(Error::NodeAlreadyAdded));
}

#[test]
fn add_connection_adds_a_connection() {
    let editor = Editor::new();
    let data = conn("1", "1", "2");
    run(editor.add_node(node("1"))).unwrap();
    run(editor.add_node(node("2"))).unwrap();
    assert_eq!(run(editor.add_connection(data.clone())), Ok(true));
    let connections = editor.get_connections();
    assert_eq!(connections.len(), 1);
    assert!(Rc::ptr_eq(&connections[0], &data));
}

#[test]
fn add_connection_refuses_a_duplicate_id() {
    let editor = Editor::new();
    let data = conn("1", "1", "2");
    run(editor.add_node(node("1"))).unwrap();
    run(editor.add_node(node("2"))).unwrap();
    run(editor.add_connection(data.clone())).unwrap();
    assert_eq!(
        run(editor.add_connection(data)),
        Err(Error::ConnectionAlreadyAdded)
    );
}

#[test]
fn remove_node_removes_a_node() {
    let editor = Editor::new();
    run(editor.add_node(node("1"))).unwrap();
    run(editor.remove_node("1")).unwrap();
    assert!(editor.get_nodes().is_empty());
}

#[test]
fn remove_node_removes_the_specified_nodes() {
    let editor = yielding_editor();
    let ids: Vec<String> = (1..=10).map(|i| i.to_string()).collect();
    block_on(join_all(ids.iter().map(|id| editor.add_node(node(id)))));
    block_on(join_all(ids[..5].iter().map(|id| editor.remove_node(id))));
    let remain: Vec<String> = editor
        .get_nodes()
        .iter()
        .map(|n| n.id().to_owned())
        .collect();
    block_on(editor.clear()).unwrap();
    assert_eq!(remain, ["6", "7", "8", "9", "10"]);
}

#[test]
fn remove_connection_removes_a_connection() {
    let editor = Editor::new();
    run(editor.add_node(node("1"))).unwrap();
    run(editor.add_node(node("2"))).unwrap();
    run(editor.add_connection(conn("1", "1", "2"))).unwrap();
    run(editor.remove_connection("1")).unwrap();
    assert!(editor.get_connections().is_empty());
}

#[test]
fn remove_connection_removes_the_specified_connections() {
    let editor = yielding_editor();
    let ids: Vec<String> = (1..=10).map(|i| i.to_string()).collect();
    block_on(join_all(
        ids.iter()
            .map(|id| editor.add_node(node(&format!("s{id}")))),
    ));
    block_on(join_all(
        ids.iter()
            .map(|id| editor.add_node(node(&format!("t{id}")))),
    ));
    block_on(join_all(ids.iter().map(|id| {
        editor.add_connection(conn(id, &format!("s{id}"), &format!("t{id}")))
    })));
    block_on(join_all(
        ids[..5].iter().map(|id| editor.remove_connection(id)),
    ));
    let remain: Vec<String> = editor
        .get_connections()
        .iter()
        .map(|c| c.id().to_owned())
        .collect();
    block_on(editor.clear()).unwrap();
    assert_eq!(remain, ["6", "7", "8", "9", "10"]);
}

#[test]
fn clear_removes_all_nodes_and_connections() {
    let editor = Editor::new();
    run(editor.add_node(node("1"))).unwrap();
    run(editor.add_node(node("2"))).unwrap();
    run(editor.add_connection(conn("1", "1", "2"))).unwrap();
    run(editor.clear()).unwrap();
    assert!(editor.get_nodes().is_empty());
    assert!(editor.get_connections().is_empty());
}

// Beyond upstream's suite: the event contract its docs state.

#[test]
fn emits_create_then_created_and_a_pipe_can_cancel() {
    let editor = Editor::new();
    let seen = Rc::new(std::cell::RefCell::new(Vec::new()));
    let log = seen.clone();
    editor.add_pipe(move |ctx| {
        log.borrow_mut().push(ctx.type_name());
        match &ctx {
            Root::NodeCreate(n) if n.id() == "blocked" => None,
            _ => Some(ctx),
        }
    });
    assert_eq!(run(editor.add_node(node("1"))), Ok(true));
    assert_eq!(run(editor.add_node(node("blocked"))), Ok(false));
    assert!(editor.get_node("blocked").is_none());
    assert_eq!(*seen.borrow(), ["nodecreate", "nodecreated", "nodecreate"]);
}

#[test]
fn a_cancelled_clear_emits_clearcancelled_and_keeps_everything() {
    let editor = Editor::new();
    let seen = Rc::new(std::cell::RefCell::new(Vec::new()));
    let log = seen.clone();
    editor.add_pipe(move |ctx| {
        log.borrow_mut().push(ctx.type_name());
        (!matches!(ctx, Root::Clear)).then_some(ctx)
    });
    run(editor.add_node(node("1"))).unwrap();
    assert_eq!(run(editor.clear()), Ok(false));
    assert_eq!(editor.get_nodes().len(), 1);
    assert_eq!(
        *seen.borrow(),
        ["nodecreate", "nodecreated", "clear", "clearcancelled"]
    );
}

#[test]
fn clear_removes_connections_before_nodes() {
    let editor = Editor::new();
    let seen = Rc::new(std::cell::RefCell::new(Vec::new()));
    run(editor.add_node(node("1"))).unwrap();
    run(editor.add_node(node("2"))).unwrap();
    run(editor.add_connection(conn("c", "1", "2"))).unwrap();
    let log = seen.clone();
    editor.add_pipe(move |ctx| {
        log.borrow_mut().push(ctx.type_name());
        Some(ctx)
    });
    run(editor.clear()).unwrap();
    assert_eq!(
        *seen.borrow(),
        [
            "clear",
            "connectionremove",
            "connectionremoved",
            "noderemove",
            "noderemoved",
            "noderemove",
            "noderemoved",
            "cleared"
        ]
    );
}

#[test]
fn missing_ids_fail_with_upstream_messages() {
    let editor = Editor::new();
    let err = run(editor.remove_node("x")).unwrap_err();
    assert_eq!(err.to_string(), "cannot find node");
    let err = run(editor.remove_connection("x")).unwrap_err();
    assert_eq!(err.to_string(), "cannot find connection");
}

#[test]
fn lookups_follow_removals() {
    let editor = Editor::new();
    for id in ["a", "b", "c", "d"] {
        run(editor.add_node(node(id))).unwrap();
    }
    run(editor.remove_node("b")).unwrap();
    assert!(editor.get_node("b").is_none());
    for id in ["a", "c", "d"] {
        assert_eq!(editor.get_node(id).unwrap().id(), id);
    }
    run(editor.add_node(node("b"))).unwrap();
    let order: Vec<String> = editor
        .get_nodes()
        .iter()
        .map(|n| n.id().to_owned())
        .collect();
    assert_eq!(order, ["a", "c", "d", "b"]);
}

#[test]
fn connections_by_node_follow_adds_and_removals() {
    let editor = Editor::new();
    for id in ["a", "b", "c"] {
        run(editor.add_node(node(id))).unwrap();
    }
    for (id, s, t) in [
        ("1", "a", "c"),
        ("2", "b", "c"),
        ("3", "a", "b"),
        ("4", "a", "c"),
    ] {
        run(editor.add_connection(conn(id, s, t))).unwrap();
    }
    let ids = |list: Vec<Rc<dyn ConnectionBase>>| -> Vec<String> {
        list.iter().map(|c| c.id().to_owned()).collect()
    };
    assert_eq!(ids(editor.get_connections_to("c")), ["1", "2", "4"]);
    assert_eq!(ids(editor.get_connections_from("a")), ["1", "3", "4"]);
    run(editor.remove_connection("1")).unwrap();
    assert_eq!(ids(editor.get_connections_to("c")), ["2", "4"]);
    assert_eq!(ids(editor.get_connections_from("a")), ["3", "4"]);
    run(editor.clear()).unwrap();
    assert!(editor.get_connections_to("c").is_empty());
    assert!(editor.get_connections_from("a").is_empty());
}

#[test]
fn order_and_lookups_survive_many_removals() {
    let editor = Editor::new();
    let ids: Vec<String> = (0..300).map(|i| i.to_string()).collect();
    for id in &ids {
        run(editor.add_node(node(id))).unwrap();
    }
    for (i, id) in ids.iter().enumerate() {
        if i % 3 != 0 {
            run(editor.remove_node(id)).unwrap();
        }
    }
    let kept: Vec<String> = ids.iter().step_by(3).cloned().collect();
    let order: Vec<String> = editor
        .get_nodes()
        .iter()
        .map(|n| n.id().to_owned())
        .collect();
    assert_eq!(order, kept);
    for id in &kept {
        assert_eq!(editor.get_node(id).unwrap().id(), id);
    }
    run(editor.add_node(node("new"))).unwrap();
    assert_eq!(editor.get_nodes().last().unwrap().id(), "new");
    assert!(editor.get_node("1").is_none());
}

#[test]
fn an_editor_pipe_can_be_removed() {
    let editor = Editor::new();
    let seen = Rc::new(std::cell::Cell::new(0));
    let count = seen.clone();
    let pipe = editor.add_pipe(move |ctx| {
        count.set(count.get() + 1);
        Some(ctx)
    });
    run(editor.add_node(node("1"))).unwrap();
    assert_eq!(seen.get(), 2);
    assert!(editor.remove_pipe(pipe));
    run(editor.add_node(node("2"))).unwrap();
    assert_eq!(seen.get(), 2);
}

#[test]
fn reorder_nodes_takes_a_full_order_or_nothing() {
    let editor = Editor::new();
    for id in ["a", "b", "c"] {
        run(editor.add_node(node(id))).unwrap();
    }
    let order =
        |e: &Editor| -> Vec<String> { e.get_nodes().iter().map(|n| n.id().to_owned()).collect() };
    assert!(editor.reorder_nodes(&["c", "a", "b"]));
    assert_eq!(order(&editor), ["c", "a", "b"]);
    assert!(!editor.reorder_nodes(&["b"]));
    assert!(!editor.reorder_nodes(&["a", "a", "b"]));
    assert!(!editor.reorder_nodes(&["a", "x", "b"]));
    assert_eq!(order(&editor), ["c", "a", "b"]);
    assert!(editor.get_node("a").is_some());
    run(editor.remove_node("a")).unwrap();
    run(editor.add_node(node("d"))).unwrap();
    assert_eq!(order(&editor), ["c", "b", "d"]);
}
