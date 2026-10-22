// [[A1]] rustRete
//! The workloads `tools/bench/compare.mjs` times on Ruggine; `tools/bench/upstream.cjs` runs the
//! same ones on upstream rete. Each returns milliseconds, the median of several runs.

use std::rc::Rc;

use ruggine::classic::{ClassicNode, Connection, Input, Node, Output, Socket};
use ruggine::future::{LocalBoxFuture, join_all, now_or_never, ready};
use ruggine::{GetSchemes, NodeBase, NodeEditor, keymap};
use ruggine_engine::{DataflowEngine, DataflowNode, Error, Inputs, Outputs};

pub struct Calc {
    node: Node,
}

impl NodeBase for Calc {
    fn id(&self) -> &str {
        &self.node.id
    }
}

impl ClassicNode for Calc {
    type Socket = Socket;
    fn classic(&self) -> &Node {
        &self.node
    }
}

impl DataflowNode<f64> for Calc {
    fn data(&self, inputs: Inputs<f64>) -> LocalBoxFuture<'_, Result<Outputs<f64>, Error>> {
        let sum: f64 = inputs.get("in").map_or(0.0, |v| v.iter().sum());
        ready(Ok(keymap! { "out" => sum + 1.0 }))
    }
}

type Schemes = GetSchemes<Calc, Connection>;

fn calc(socket: &Rc<Socket>) -> Rc<Calc> {
    let node = Node::new("calc");
    node.add_input(
        "in",
        Input::new(socket.clone()).with_multiple_connections(true),
    )
    .unwrap();
    node.add_output("out", Output::new(socket.clone())).unwrap();
    Rc::new(Calc { node })
}

fn run<F: std::future::Future>(f: F) -> F::Output {
    now_or_never(f).expect("synchronous graph")
}

struct Setup {
    editor: NodeEditor<Schemes>,
    engine: DataflowEngine<Schemes, f64>,
    nodes: Vec<Rc<Calc>>,
}

fn setup(n: usize) -> Setup {
    let editor = NodeEditor::new();
    let engine = DataflowEngine::new();
    editor.use_scope(&engine).unwrap();
    let socket = Rc::new(Socket::new("number"));
    let nodes = (0..n).map(|_| calc(&socket)).collect();
    Setup {
        editor,
        engine,
        nodes,
    }
}

impl Setup {
    fn add_all(&self) {
        for node in &self.nodes {
            run(self.editor.add_node(node.clone())).unwrap();
        }
    }

    fn chain(&self) {
        for pair in self.nodes.windows(2) {
            let c = Connection::new(&*pair[0], "out", &*pair[1], "in").unwrap();
            run(self.editor.add_connection(Rc::new(c))).unwrap();
        }
    }

    fn fan_in(&self) {
        let sink = self.nodes.last().unwrap();
        for node in &self.nodes[..self.nodes.len() - 1] {
            let c = Connection::new(&**node, "out", &**sink, "in").unwrap();
            run(self.editor.add_connection(Rc::new(c))).unwrap();
        }
    }

    fn last(&self) -> &str {
        self.nodes.last().unwrap().id()
    }
}

/// The scenario names, in report order.
pub const SCENARIOS: &[&str] = &[
    "create_nodes",
    "add_nodes",
    "add_connections",
    "fetch_chain_cold",
    "fetch_chain_warm",
    "reset_and_refetch_chain",
    "fetch_fan_in_cold",
    "lookup_nodes",
    "remove_connections",
    "remove_nodes_concurrently",
    "clear",
];

/// Times one scenario on a graph of `n` nodes; `now` is a millisecond clock.
pub fn scenario(name: &str, n: usize, now: &dyn Fn() -> f64) -> f64 {
    let time = |f: &mut dyn FnMut()| {
        let start = now();
        f();
        now() - start
    };
    match name {
        "create_nodes" => {
            let socket = Rc::new(Socket::new("number"));
            time(&mut || {
                std::hint::black_box((0..n).map(|_| calc(&socket)).collect::<Vec<_>>());
            })
        }
        "add_nodes" => {
            let s = setup(n);
            time(&mut || s.add_all())
        }
        "add_connections" => {
            let s = setup(n);
            s.add_all();
            time(&mut || s.chain())
        }
        "fetch_chain_cold" => {
            let s = setup(n);
            s.add_all();
            s.chain();
            time(&mut || {
                run(s.engine.fetch(s.last())).unwrap();
            })
        }
        "fetch_chain_warm" => {
            let s = setup(n);
            s.add_all();
            s.chain();
            run(s.engine.fetch(s.last())).unwrap();
            time(&mut || {
                run(s.engine.fetch(s.last())).unwrap();
            })
        }
        "reset_and_refetch_chain" => {
            let s = setup(n);
            s.add_all();
            s.chain();
            run(s.engine.fetch(s.last())).unwrap();
            let first = s.nodes[0].id().to_owned();
            time(&mut || {
                s.engine.reset(Some(&first)).unwrap();
                run(s.engine.fetch(s.last())).unwrap();
            })
        }
        "fetch_fan_in_cold" => {
            let s = setup(n);
            s.add_all();
            s.fan_in();
            time(&mut || {
                run(s.engine.fetch(s.last())).unwrap();
            })
        }
        "lookup_nodes" => {
            let s = setup(n);
            s.add_all();
            let ids: Vec<String> = s.nodes.iter().map(|n| n.id().to_owned()).collect();
            time(&mut || {
                for id in &ids {
                    std::hint::black_box(s.editor.get_node(id));
                }
            })
        }
        "remove_connections" => {
            let s = setup(n);
            s.add_all();
            s.chain();
            let ids: Vec<String> = s
                .editor
                .get_connections()
                .iter()
                .map(|c| c.id.clone())
                .collect();
            time(&mut || {
                for id in &ids {
                    run(s.editor.remove_connection(id)).unwrap();
                }
            })
        }
        "remove_nodes_concurrently" => {
            let s = setup(n);
            s.add_all();
            let ids: Vec<String> = s.nodes.iter().map(|n| n.id().to_owned()).collect();
            time(&mut || {
                run(join_all(ids.iter().map(|id| s.editor.remove_node(id))));
            })
        }
        "clear" => {
            let s = setup(n);
            s.add_all();
            s.chain();
            time(&mut || {
                run(s.editor.clear()).unwrap();
            })
        }
        other => panic!("unknown scenario {other}"),
    }
}

/// Every scenario at size `n`, median of `runs`, as a JSON object.
pub fn report(n: usize, runs: usize, now: &dyn Fn() -> f64) -> String {
    let mut parts = Vec::new();
    for name in SCENARIOS {
        let mut times: Vec<f64> = (0..runs).map(|_| scenario(name, n, now)).collect();
        times.sort_by(f64::total_cmp);
        parts.push(format!("\"{name}\":{}", times[times.len() / 2]));
    }
    format!("{{{}}}", parts.join(","))
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::wasm_bindgen;

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = performance)]
        fn now() -> f64;
    }

    #[wasm_bindgen]
    pub fn report(n: usize, runs: usize) -> String {
        super::report(n, runs, &|| now())
    }
}
