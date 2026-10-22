// [[B1]] upstreamIsSpec: replays tools/oracle/fuzz.cjs's random operation sequences and compares
// each trace with upstream's in fixtures/oracle-fuzz.json (regenerate: node tools/oracle/run.mjs).
mod common;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use common::Yield;
use ruggine::classic::{ClassicNode, Connection, Input, Node, Output, Socket};
use ruggine::future::LocalBoxFuture;
use ruggine::{ConnectionBase, GetSchemes, NodeBase, NodeEditor, keymap};
use ruggine_engine::{DataflowEngine, DataflowNode, Error, Inputs, Outputs};
use serde_json::Value;

type Trace = Rc<RefCell<Vec<String>>>;

/// Drives `f` the way a JavaScript host would: between its polls the background queue runs, as
/// the event loop runs microtasks.
fn block_on<F: std::future::Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    let mut background = std::pin::pin!(ruggine::future::background());
    let waker = std::task::Waker::noop();
    let mut cx = std::task::Context::from_waker(waker);
    for _ in 0..1_000_000 {
        if let std::task::Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
        let _ = background.as_mut().poll(&mut cx);
    }
    panic!("future never finished");
}

struct FuzzNode {
    node: Node,
    yields: u64,
    fail: bool,
    trace: Trace,
}

impl NodeBase for FuzzNode {
    fn id(&self) -> &str {
        &self.node.id
    }
}

impl ClassicNode for FuzzNode {
    type Socket = Socket;
    fn classic(&self) -> &Node {
        &self.node
    }
}

#[derive(Debug)]
struct Fail(String);
impl std::fmt::Display for Fail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Fail {}

impl DataflowNode<String> for FuzzNode {
    fn data(&self, inputs: Inputs<String>) -> LocalBoxFuture<'_, Result<Outputs<String>, Error>> {
        let id = self.node.id.clone();
        self.trace.borrow_mut().push(format!("data {id}"));
        let (yields, fail) = (self.yields, self.fail);
        Box::pin(async move {
            for _ in 0..yields {
                Yield::once().await;
            }
            if fail {
                return Err(Error::node(Fail(format!("fail {id}"))));
            }
            let list = |k| {
                inputs
                    .get(k)
                    .map(|v: &Vec<String>| v.join(","))
                    .unwrap_or_default()
            };
            Ok(keymap! { "o" => format!("{id}({}|{})", list("a"), list("b")) })
        })
    }
}

type Schemes = GetSchemes<FuzzNode, Connection>;

fn outcome<T>(trace: &Trace, label: String, result: Result<T, String>, show: impl Fn(T) -> String) {
    let line = match result {
        Ok(v) => format!("{label} -> {}", show(v)),
        Err(e) => format!("{label} !! {e}"),
    };
    trace.borrow_mut().push(line);
}

fn replay(ops: &[Value]) -> Vec<String> {
    let trace: Trace = Rc::default();
    let editor = NodeEditor::<Schemes>::new();
    let engine = DataflowEngine::<Schemes, String>::new();
    editor.use_scope(&engine).unwrap();
    let t = trace.clone();
    editor.add_pipe(move |ctx| {
        use ruggine::Root::*;
        let line = match &ctx {
            NodeCreate(n) | NodeCreated(n) | NodeRemove(n) | NodeRemoved(n) => {
                format!("evt {} {}", ctx.type_name(), n.id())
            }
            ConnectionCreate(c) | ConnectionCreated(c) | ConnectionRemove(c)
            | ConnectionRemoved(c) => format!("evt {} {}", ctx.type_name(), c.id()),
            _ => format!("evt {}", ctx.type_name()),
        };
        t.borrow_mut().push(line);
        Some(ctx)
    });
    let mut nodes: HashMap<String, Rc<FuzzNode>> = HashMap::new();
    let s = |v: &Value, k: &str| v[k].as_str().unwrap().to_owned();
    let err = |e: &dyn std::fmt::Display| e.to_string();
    for op in ops {
        match op["op"].as_str().unwrap() {
            "addNode" => {
                let id = s(op, "id");
                let mut node = Node::new(id.clone());
                node.id = id.clone();
                for k in ["a", "b"] {
                    let input = Input::new(Socket::new("s")).with_multiple_connections(true);
                    node.add_input(k, input).unwrap();
                }
                node.add_output("o", Output::new(Socket::new("s"))).unwrap();
                let n = Rc::new(FuzzNode {
                    node,
                    yields: op["yields"].as_u64().unwrap(),
                    fail: op["fail"].as_bool().unwrap(),
                    trace: trace.clone(),
                });
                nodes.insert(id.clone(), n.clone());
                let r = block_on(editor.add_node(n)).map_err(|e| err(&e));
                outcome(&trace, format!("addNode {id}"), r, |v| v.to_string());
            }
            "addConnection" => {
                let id = s(op, "id");
                let (a, b) = (&nodes[&s(op, "source")], &nodes[&s(op, "target")]);
                let mut c = Connection::new(&**a, "o", &**b, s(op, "input")).unwrap();
                c.id = id.clone();
                let r = block_on(editor.add_connection(Rc::new(c))).map_err(|e| err(&e));
                outcome(&trace, format!("addConnection {id}"), r, |v| v.to_string());
            }
            "removeConnection" => {
                let id = s(op, "id");
                let r = block_on(editor.remove_connection(&id)).map_err(|e| err(&e));
                outcome(&trace, format!("removeConnection {id}"), r, |v| {
                    v.to_string()
                });
            }
            "removeNode" => {
                let id = s(op, "id");
                let r = block_on(editor.remove_node(&id)).map_err(|e| err(&e));
                outcome(&trace, format!("removeNode {id}"), r, |v| v.to_string());
            }
            "fetch" => {
                let id = s(op, "id");
                let r = block_on(engine.fetch(&id)).map_err(|e| err(&e));
                outcome(&trace, format!("fetch {id}"), r, |o| o["o"].clone());
            }
            "fetchAll" => {
                let ids: Vec<String> = op["ids"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().to_owned())
                    .collect();
                let futures: Vec<_> = ids.iter().map(|id| engine.fetch(id)).collect();
                let all = block_on(ruggine::future::try_join_all(futures)).map_err(|e| err(&e));
                outcome(&trace, format!("fetchAll {}", ids.join(",")), all, |outs| {
                    outs.iter()
                        .map(|o| o["o"].clone())
                        .collect::<Vec<_>>()
                        .join(" ")
                });
            }
            "fetchInputs" => {
                let id = s(op, "id");
                let r = block_on(engine.fetch_inputs(&id)).map_err(|e| err(&e));
                outcome(&trace, format!("fetchInputs {id}"), r, |inputs| {
                    let fields: Vec<String> = inputs
                        .iter()
                        .map(|(k, v)| format!("{}:{}", Value::from(k), Value::from(v.clone())))
                        .collect();
                    format!("{{{}}}", fields.join(","))
                });
            }
            "reset" => {
                let id = op["id"].as_str();
                let r = engine.reset(id).map_err(|e| err(&e));
                outcome(&trace, format!("reset {}", id.unwrap_or("null")), r, |_| {
                    "ok".into()
                });
            }
            other => panic!("unknown op {other}"),
        }
    }
    let mut background = std::pin::pin!(ruggine::future::background());
    let _ = background
        .as_mut()
        .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()));
    trace.borrow_mut().push("end".into());
    trace.take()
}

fn suite(name: &str) -> Vec<Value> {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/oracle-fuzz.json"
    ))
    .expect("fixtures/oracle-fuzz.json");
    let all: Value = serde_json::from_str(&text).unwrap();
    all[name].as_array().unwrap().clone()
}

fn lines(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|l| l.as_str().unwrap().to_owned())
        .collect()
}

fn first_difference(a: &[String], b: &[String]) -> String {
    let at = a
        .iter()
        .zip(b)
        .position(|(x, y)| x != y)
        .unwrap_or(a.len().min(b.len()));
    format!(
        "line {at}: upstream {:?}, ruggine {:?}, after {:?}",
        a.get(at),
        b.get(at),
        &a[at.saturating_sub(4)..at.min(a.len())]
    )
}

fn report(failures: &[String], total: usize) {
    assert!(
        failures.is_empty(),
        "{} of {total} cases differ from upstream (inspect one with SEED=<n> SUITE=<name> cargo test \
         -p ruggine-engine --test oracle_fuzz show -- --ignored --nocapture):\n{}",
        failures.len(),
        failures
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

fn kind(t: &[String], f: fn(&str) -> bool) -> Vec<String> {
    t.iter().filter(|l| f(l)).cloned().collect()
}

fn is_outcome(l: &str) -> bool {
    l.contains(" -> ") || l.contains(" !! ")
}

fn is_event(l: &str) -> bool {
    l.starts_with("evt ")
}

fn work(t: &[String]) -> Vec<String> {
    let mut w = kind(t, |l| l.starts_with("data "));
    w.sort();
    w
}

/// One operation at a time: every result and every editor event match in order, and the same
/// node work runs. Only the order of sibling inputs' work may differ (`tree/specs/port.md` §
/// Futures).
#[test]
fn sequential_random_cases_match_upstream() {
    let cases = suite("sequential");
    let mut failures = Vec::new();
    let mut exact = 0;
    for case in &cases {
        let expected = lines(&case["trace"]);
        let got = replay(case["ops"].as_array().unwrap());
        if got == expected {
            exact += 1;
            continue;
        }
        let seed = &case["seed"];
        for (what, f) in [
            ("results", is_outcome as fn(&str) -> bool),
            ("events", is_event),
        ] {
            if kind(&expected, f) != kind(&got, f) {
                let diff = first_difference(&kind(&expected, f), &kind(&got, f));
                failures.push(format!("seed {seed} {what}: {diff}"));
            }
        }
        if work(&expected) != work(&got) {
            failures.push(format!("seed {seed}: different node work"));
        }
    }
    report(&failures, cases.len());
    assert!(
        exact * 10 >= cases.len() * 9,
        "only {exact} cases match line for line"
    );
}

/// Several fetches in flight at once: every operation's result and every editor event match,
/// and so does the work nodes do, except as `tree/specs/port.md` § Futures allows: which of
/// several concurrent failures is reported, and work raced by a later reset.
#[test]
fn concurrent_random_cases_match_upstream_up_to_timing() {
    let cases = suite("concurrent");
    let mut failures = Vec::new();
    let mut work_checked = 0;
    for case in &cases {
        let ops = case["ops"].as_array().unwrap();
        let failing: Vec<String> = ops
            .iter()
            .filter(|o| o["op"] == "addNode" && o["fail"] == true)
            .map(|o| format!("fail {}", o["id"].as_str().unwrap()))
            .collect();
        let expected = lines(&case["trace"]);
        let got = replay(ops);
        let (want, have) = (kind(&expected, is_outcome), kind(&got, is_outcome));
        let outcomes_ok = want.len() == have.len()
            && want.iter().zip(&have).all(|(w, h)| {
                w == h
                    || (w.starts_with("fetchAll ")
                        && w.contains(" !! ")
                        && h.split(" !! ").next() == w.split(" !! ").next()
                        && h.split(" !! ").nth(1).is_some_and(|e| {
                            e == "node is not initialized" || failing.iter().any(|f| f == e)
                        }))
            });
        if !outcomes_ok {
            failures.push(format!(
                "seed {} results: {}",
                case["seed"],
                first_difference(&want, &have)
            ));
            continue;
        }
        if kind(&expected, is_event) != kind(&got, is_event) {
            let diff = first_difference(&kind(&expected, is_event), &kind(&got, is_event));
            failures.push(format!("seed {} events: {diff}", case["seed"]));
            continue;
        }
        let raced = want
            .iter()
            .any(|l| l.starts_with("fetchAll ") && l.contains(" !! "));
        if !raced {
            work_checked += 1;
            if work(&expected) != work(&got) {
                failures.push(format!("seed {}: different node work", case["seed"]));
            }
        }
    }
    report(&failures, cases.len());
    assert!(
        work_checked > 100,
        "only {work_checked} cases compared node work"
    );
}

#[test]
#[ignore]
fn show() {
    let seed: u64 = std::env::var("SEED").unwrap().parse().unwrap();
    let name = std::env::var("SUITE").unwrap_or_else(|_| "concurrent".into());
    let cases = suite(&name);
    let case = cases
        .iter()
        .find(|c| c["seed"].as_u64() == Some(seed))
        .unwrap();
    let expected = lines(&case["trace"]);
    let got = replay(case["ops"].as_array().unwrap());
    for i in 0..got.len().max(expected.len()) {
        let (a, b) = (
            expected.get(i).map_or("", String::as_str),
            got.get(i).map_or("", String::as_str),
        );
        println!("{} {a:<60} | {b}", if a == b { " " } else { "*" });
    }
}
