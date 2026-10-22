# Port plan

What gets ported, where it lands, and the questions the first port session has to answer.
Governed by [[A3]] portEverything (all of upstream, Solenoid's surface first) and [[B1]] upstreamIsSpec (tests
before code). Delete each row's "open" status as it lands; delete this file when the table is
all done and the questions are all leaves.

## What Solenoid calls

Counted from Solenoid's `src/` on the `tsrs` branch, 2026-10-22. Re-count before relying on
it (`grep -rhoE "editor\.(\w+)\(" src | sort | uniq -c` in Solenoid).

| Surface | Calls | Notes |
|---|---|---|
| `NodeEditor.getNode` / `getNodes` / `getConnections` / `getConnection` | 151 / 106 / 82 / 1 | read side, by far the hottest |
| `NodeEditor.addNode` / `removeNode` / `addConnection` / `removeConnection` / `clear` | 22 / 11 / 30 / 35 / 5 | all `async` upstream |
| `NodeEditor.addPipe` | 14 | pipes only watch `nodecreated`, `noderemoved`, `connectioncreated`, `connectionremoved`, `clear`; none were seen cancelling an event |
| `Scope.use` | 6 | always `editor.use(engine)` with a `DataflowEngine` |
| `ClassicPreset.Node` / `Input` / `Output` / `Socket` / `Connection` | heavy | `addInput` ×719, `addOutput` ×385, `removeInput` ×67, `removeOutput` ×7 |
| `DataflowEngine` | `fetch`, `reset` | `fetchInputs` unused; 7 node files have an `async data()` |
| `GetSchemes`, `Root` | types | the scheme generics |

Not used by Solenoid, so ported after the rows above: `ClassicPreset.Control` and
`InputControl` (and the `addControl` family on `Node` and `Input`), `ControlFlowEngine` and
`ControlFlow`, and the rest of `Scope.parentScope`. The type-level signal checks in
`utility-types.ts` / `NestedScope` / `useHelper` are compile-time TypeScript tricks; their Rust
counterpart is whatever trait bounds the scope design lands on, not a line-by-line port.

## Module map

| Upstream file | Lines | Rust home | Used | Status |
|---|---|---|---|---|
| `rete/src/types.ts` | 33 | `ruggine::types` | yes | open |
| `rete/src/utils.ts` (`getUID`) | 16 | `ruggine::uid` | yes | open |
| `rete/src/scope.ts` (`Signal`, `Scope`, pipes) | 129 | `ruggine::scope` | yes | open |
| `rete/src/editor.ts` (`NodeEditor`, `Root`) | 174 | `ruggine::editor` | yes | open |
| `rete/src/presets/classic.ts` (`Socket`, `Port`, `Input`, `Output`, `Node`, `Connection`, controls) | 291 | `ruggine::classic` | yes, controls no | open |
| `rete/src/utility-types.ts` | 58 | trait bounds in `ruggine::scope` | no | open |
| `rete-engine/src/dataflow.ts` | 115 | `ruggine_engine::dataflow` | yes | open |
| `rete-engine/src/dataflow-engine.ts` | 135 | `ruggine_engine::engine` | yes | open |
| `rete-engine/src/utils/cache.ts` | 34 | `ruggine_engine::cache` | yes | open |
| `rete-engine/src/utils/cancellable.ts` | 49 | depends on the async question below | yes | open |
| `rete-engine/src/control-flow.ts`, `control-flow-engine.ts` | 154 | `ruggine_engine::control_flow` | no | open |

Upstream tests to translate first, per module: `rete/test/{index,scope,utils}.test.ts`,
`rete/test/presets/classic.test.ts`, `rete-engine/test/{dataflow,dataflow-engine}.test.ts`,
`rete-engine/test/utils/{cache,cancellable}.test.ts`, then `rete-engine/test/{control-flow,control-flow-engine}.test.ts`.
`rete/test/index.perf.ts` becomes a bench, and `rete-engine/test/types.test-d.ts` (type-level
assertions) becomes compile checks where the Rust types can express them. They land in each crate's `tests/`.

## Questions for the first port session

Each one is a decision; answer it as a leaf before writing the code that depends on it.

1. **Async or sync.** Every editor mutation and `Signal.emit` is `async` upstream, and the
   dataflow engine awaits `data()`. Rust options: plain sync (simplest, and most of
   Solenoid's `data()` is sync), or `async fn` with no runtime dependency (keeps the 7 async
   nodes and the cancel-on-reset behavior, costs ergonomics). [[B2]] wasmClean rules out
   anything that needs threads or tokio.
2. **Schemes.** Upstream's `Scheme['Node']` / `Scheme['Connection']` generics become either a
   trait with associated types or concrete `classic` types only. Solenoid subclasses
   `ClassicPreset.Node` hundreds of ways; in Rust that's a trait object, an enum, or a
   generic payload on one node struct.
3. **Pipes and cancellation.** A pipe returns the context to continue or `undefined` to stop
   the chain (which is how `nodecreate` and friends get cancelled). Likely
   `Fn(Ctx) -> Option<Ctx>`; decide whether pipes own or borrow the context, and whether
   `use()` stays a pipe that forwards to the child scope or becomes a direct observer list.
4. **Ownership.** Upstream nodes are shared JS objects mutated in place (`node.addInput` after
   `editor.addNode`). Rust needs a home: the editor owns nodes in a map and hands out
   `&`/`&mut`, or nodes live behind `Rc<RefCell<_>>`. The engine needs to read the editor
   while running, which pushes toward the editor passing itself into `fetch`.
5. **Ids.** `getUID` makes a random hex id from `crypto` (with a `Date` fallback). Keep the
   string shape so Solenoid's saves don't change, and pick a randomness source that works on
   wasm (`getrandom` with its `js` feature, or a counter seeded by the caller).
6. **Values.** `data()` returns `Record<string, any>`. Solenoid's Rust core already has a
   value type; decide whether `ruggine-engine` is generic over the value (likely) or depends
   on Solenoid's.
7. **The wasm boundary.** Whether Ruggine gets its own `wasm-bindgen` surface, or only runs
   inside Solenoid's core and never crosses into JS by itself.
