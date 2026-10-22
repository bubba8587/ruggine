# Ruggine

A headless node-graph engine for Rust: an editor that holds nodes and connections and tells you
about every change, plus engines that compute values through the graph (dataflow) or pass
control along it (control flow). It runs natively and in the browser via WebAssembly.

Ruggine is a faithful port of [rete.js](https://retejs.org)'s core (`rete` 2.0.6 and
`rete-engine` 2.1.1): the same model, the same events in the same order, the same answers. It is
checked against the real rete code, not just its documentation. It is also faster than rete at
everything measured, from 2× to 90× on whole-graph work and several hundred times on lookups,
and it keeps working on graphs where rete runs out of stack. *Ruggine* is Italian for
rust.

There's no rendering here, just as rete's core has none: draw the graph with whatever you like
(the app it was built for, [Solenoid](https://github.com/bubba8587/solenoid), uses React Flow).

## A taste

```rust
use std::rc::Rc;
use ruggine::classic::{ClassicNode, Connection, Input, Node, Output, Socket};
use ruggine::future::{now_or_never, ready, LocalBoxFuture};
use ruggine::{keymap, GetSchemes, NodeBase, NodeEditor};
use ruggine_engine::{DataflowEngine, DataflowNode, Error, Inputs, Outputs};

/// Adds 1 to whatever arrives on `in`.
struct Inc(Node);

impl NodeBase for Inc {
    fn id(&self) -> &str { &self.0.id }
}
impl ClassicNode for Inc {
    type Socket = Socket;
    fn classic(&self) -> &Node { &self.0 }
}
impl DataflowNode<f64> for Inc {
    fn data(&self, inputs: Inputs<f64>) -> LocalBoxFuture<'_, Result<Outputs<f64>, Error>> {
        let x = inputs.get("in").and_then(|v| v.first().copied()).unwrap_or(0.0);
        ready(Ok(keymap! { "out" => x + 1.0 }))
    }
}

let editor = NodeEditor::<GetSchemes<Inc, Connection>>::new();
let engine = DataflowEngine::new();
editor.use_scope(&engine)?;

// add a, b and a connection a.out -> b.in (see crates/ruggine-engine/src/lib.rs for the full
// example), then:
let out = engine.fetch(b.id()).await?;   // or now_or_never(...) for a synchronous graph
assert_eq!(out["out"], 2.0);
engine.reset(Some(a.id()))?;             // a changed: b recomputes on the next fetch
```

## What's in it

- **`ruggine`**, the editor:
  - `NodeEditor`: add, remove and look up nodes and connections. Every change goes through a
    chain of *pipes*, which can watch it, rewrite it or cancel it (`nodecreate`,
    `connectionremoved`, `clear` and the rest).
  - Scopes and plugins: attach a plugin (an engine, your own) under the editor and it sees
    every event.
  - The classic preset: nodes with named, typed inputs and outputs (sockets), controls, and
    connections between ports.
- **`ruggine-engine`**, the engines:
  - `DataflowEngine`: pull a node's outputs and it computes everything upstream first, caching
    each result until you `reset` that node (which also clears everything downstream). Node
    work can be async; a reset cancels whatever is in flight.
  - `ControlFlowEngine`: run a node, and it passes control along whichever outputs it
    forwards.

Node and connection types are yours: implement two small traits, or use a trait object to mix
many kinds of node in one graph. Values are one type of your choosing per engine.

## Faithful to rete

Coming from rete? [`docs/migrating-from-rete.md`](docs/migrating-from-rete.md) maps every rete
and rete-engine symbol to its Ruggine counterpart.

Ruggine aims to give the same answers as rete in every case, and to write down each case where
it doesn't.

- Every one of rete's own tests is translated and passes.
- **An oracle checks Ruggine against rete itself.** `tools/oracle/` compiles the pinned rete
  source and runs 27 scenarios on it: order of work, concurrent fetches, cancellation, resets
  from inside running nodes, errors, editor events, key order, control flow. The Rust tests
  replay each scenario and must give rete's exact answer. CI re-runs rete and fails if its
  answers drift from the recorded ones.
- Async works the way JavaScript does. Shared results run on a per-thread queue that stands in
  for JS's microtask queue, so concurrent fetches interleave, the first error wins and failed
  siblings still run, all as in rete.
- Every difference from rete is listed, with its reason, in
  [`tree/specs/port.md`](tree/specs/port.md). Most are places where rete itself breaks: it
  overflows the stack on long chains, can corrupt its node list under concurrent removals, and
  never finishes when a fetch runs into a cycle.

## Better than rete

- **Fast.** At 10,000 nodes, 9 to 90 times faster than rete at building, computing and tearing
  down a graph, and 650 times faster at lookups, natively and in wasm alike. Rete scans every connection for
  every node it computes; Ruggine indexes connections by node, keeps lookups by id constant
  time, and never shifts a list to remove one item. Numbers: [`docs/benchmarks.md`](docs/benchmarks.md).
- **No stack limit.** A chain of 50,000 nodes computes on a flat stack (20,000 in a debug wasm
  build, each node awaiting a real JS promise). Rete overflows resetting a 10,000-node chain.
- **Cycle-safe resets.** A reset walks each node once, so a cycle can't crash it.
- **A few additions:** `get_connections_to` and `get_connections_from` (a node's connections
  without a scan), `input_keys` and `output_keys` (cached), typed control lookup
  (`node.control::<InputControl<f64>>("x")`), and a swappable random source for ids.
- **No runtime required.** Ruggine spawns nothing and depends on no async runtime. Any executor
  drives it (Tokio, `wasm-bindgen-futures`, your own), and a fully synchronous graph finishes
  on the first poll.

## Rust only

Ruggine is for Rust programs, including Rust compiled to wasm for the browser. It doesn't ship
a JavaScript package with rete's API: an app moving off rete moves its graph code into Rust
along with it.

## Platforms

Ruggine builds for `wasm32-unknown-unknown` and native targets, and CI runs the tests in both,
the wasm ones in a real runtime (Node). The only call it makes out of wasm is for random bytes
for ids, through `crypto.getRandomValues`. No `unsafe`.

## Building and testing

```
cargo test --workspace
cargo build --workspace --target wasm32-unknown-unknown
node tools/oracle/run.mjs          # re-record rete's answers (needs tsc)
node tools/bench/compare.mjs       # benchmark against rete (needs tsc and wasm-bindgen-cli)
```

Contributing: `CLAUDE.md` has the working rules and the full check list, and `docs/` the
rest.

## Status

Pre-1.0 and moving fast: the API changes without deprecation paths. Its first user,
[Solenoid](https://github.com/bubba8587/solenoid), runs its dataflow engine on it, compiled to
wasm, with the app's own JavaScript nodes behind it.

## License

MIT. Ruggine is a port of rete and rete-engine, both MIT; see `THIRD-PARTY-NOTICES.md`.
