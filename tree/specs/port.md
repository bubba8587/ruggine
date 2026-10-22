---
aliases: ["Port mapping"]
tags: [spec, port]
---
<!-- [[B1]] upstreamIsSpec, [[B2]] browserAndDesktop -->

# Spec: how upstream maps onto Ruggine

Serves [[B1]] upstreamIsSpec and [[B2]] browserAndDesktop. Upstream is `rete` 2.0.6 and
`rete-engine` 2.1.1 in `upstream/`. Every upstream module and test has a Rust home; this spec
says where, and lists every place the Rust behaves differently, each with its reason. A
difference not listed here is a bug.

## Method

- An upstream module is ported by translating its tests first, then writing the code until they
  pass. Each Rust test file names the upstream test file it translates in its first line.
- Tests for behavior upstream's source states but its suite doesn't check follow, under a
  "Beyond upstream's suite" line; every divergence below has one.
- The oracle: `tools/oracle/scenarios.cjs` runs scenarios against the pinned upstream itself
  (`node tools/oracle/run.mjs` compiles `upstream/` and records the answers in
  `fixtures/oracle.json`), and `ruggine-engine/tests/oracle.rs` replays each scenario, by name,
  against Ruggine. Any behavior where order or concurrency matters gets a scenario there rather
  than an expectation written by hand.
- The fuzzer: `tools/oracle/fuzz.cjs` generates 300 random operation sequences with one
  operation at a time and 300 with several fetches in flight, runs them on upstream and records
  each trace in `fixtures/oracle-fuzz.json` (by `run.mjs`); `oracle_fuzz.rs` replays them,
  driving the queue as a JS host would, and holds Ruggine to the guarantees in § Futures.
- Moving the pin: `upstream/README.md`; then rerun the oracle and diff old and new `src/` and
  `test/` against this spec.

## Where everything lives

| Upstream | Rust | Tests |
|---|---|---|
| `rete/src/types.ts` | `ruggine::types` | (types only) |
| `rete/src/utils.ts` `getUID` | `ruggine::uid` | `ruggine/tests/uid.rs` |
| `rete/src/scope.ts` | `ruggine::scope` | `ruggine/tests/scope.rs` |
| `rete/src/editor.ts` | `ruggine::editor` | `ruggine/tests/editor.rs`, `perf.rs` |
| `rete/src/presets/classic.ts` | `ruggine::classic` | `ruggine/tests/classic.rs` |
| `rete/src/utility-types.ts` | the `Accepts` and `Plugin` bounds in `ruggine::scope` | compile-time |
| `rete-engine/src/dataflow.ts` | `ruggine_engine::dataflow` | `ruggine-engine/tests/dataflow.rs` |
| `rete-engine/src/dataflow-engine.ts` | `ruggine_engine::dataflow_engine` | `dataflow_engine.rs`, `dyn_nodes.rs` |
| `rete-engine/src/control-flow.ts` | `ruggine_engine::control_flow` | `control_flow.rs` |
| `rete-engine/src/control-flow-engine.ts` | `ruggine_engine::control_flow_engine` | `control_flow_engine.rs` |
| `rete-engine/src/utils/cache.ts` | `ruggine_engine::cache` | `cache.rs` |
| `rete-engine/src/utils/cancellable.ts` | `ruggine_engine::cancellable` | `cancellable.rs` |
| (JS objects keyed by name) | `ruggine::keymap::KeyMap` | `ruggine/tests/keymap.rs` |
| (promises, `Promise.all`, the microtask queue) | `ruggine::future` | `ruggine/tests/future.rs`, `ruggine-engine/tests/real_wakeups.rs` |
| (upstream's own answers) | `fixtures/oracle.json` | `ruggine-engine/tests/oracle.rs` |

Names are upstream's in snake_case. A JS method that throws returns `Result`, with upstream's
message as the error's `Display`. `editor.use(x)` is `use_scope` (`use` is a Rust keyword).
`rete-engine/test/types.test-d.ts` checks TypeScript inference only and has no Rust counterpart:
values are one type `V` per engine (see Dataflow).

## Futures

- Async because upstream is: Solenoid has nodes whose data really waits (frames, scripts, live
  connections, composites) and relies on `reset` cancelling a fetch in flight. Futures are
  `!Send`, single-threaded like JavaScript, and nothing in Ruggine spawns threads or blocks: any
  host's executor drives them, and `future::now_or_never` finishes any graph whose pipes and
  nodes are synchronous, including from inside a node's `data`.
- Eager where upstream is eager. A JS async function runs synchronously up to its first `await`,
  so upstream's `fetch` calls the node's setup (creating and caching its run) and `fetchInputs`
  starts every predecessor's fetch at call time. `Dataflow::fetch` and `Dataflow::fetch_inputs`
  do that work when called, and only the rest waits for a poll; a `reset` between `fetch` and
  the first poll cancels it, as upstream.
- JavaScript's microtask queue is `future`'s per-thread queue. A promise held in several places
  is a `Shared`, a task on that queue; the engine's cached runs are `Shared::spawn`, queued the
  moment they are created, as a promise starts at once. The queue runs tasks first in, first
  out, one at a time, so a chain of nodes never nests on the stack (a 50,000-node chain is
  tested natively, 20,000 in debug wasm; nesting overflowed a debug build before 1,000).
- Who runs the queue: awaiting a `Shared` from inside queued work only queues it. Awaiting it
  from anywhere else (an executor, `now_or_never`, a blocking call inside a node) runs queued
  work until the `Shared` is done, then the work already queued at that moment, and returns:
  where JS would resume the caller, behind the microtasks already waiting. If the work still
  waits on something outside, the poller is parked and woken as soon as anything on the queue
  wakes, from any thread. The engines' `fetch` and `fetch_inputs` are wrapped in
  `future::settle`, which does the same when the result is ready without awaiting a `Shared`
  (an input with no setup fails at once while its siblings' work is queued).
- Work nobody awaits any more (the siblings of a fetch that failed) runs whenever the queue next
  runs. `future::background()` is the host's event loop for it: spawned once, it runs the queue
  dry every time work arrives, so that work finishes as it would in JS even if Ruggine is never
  called again.
- What matches upstream, checked by `ruggine-engine/tests/oracle_fuzz.rs` on random operation
  sequences recorded from upstream (`tools/oracle/fuzz.cjs`): every operation's result and every
  editor event, in order, and the set of node work that runs. What can differ, because it
  follows from how many microtask turns each step of upstream's promise chain takes (a node
  whose inputs are cached still waits about three turns more than one with no inputs):
  - the order in which sibling inputs' work runs within one fetch;
  - when several fetches are in flight and more than one fails, which failure is reported;
  - whether a reset issued while abandoned work is still running lands before or after a given
    node's work.
  Matching those would mean replaying V8's promise job scheduling turn for turn; nothing
  Solenoid does depends on them, since it fetches one node at a time.
- A `Shared` whose work panics poisons: every later await panics too, and everyone waiting is
  woken to see it. Work left queued by a panic runs the next time the queue does.
- Like JS microtasks, a task that wakes itself on every poll starves the rest: the queue never
  runs dry, and the poll that is running it never returns. Yield to the host's executor with a
  future that waits on the host (a timer, a JS promise), not a self-wake.
- `Promise.all` is `join_all` / `try_join_all`, polling their futures in input order.
- Control flow is synchronous upstream and stays recursive here, one call per forward, because
  the order (forward runs the successor before the forwarding node continues) is its contract.
  A very long control chain can exhaust the stack where JS would throw `RangeError`.

## Scopes

- Scopes, editors, dataflows and engines are cheap-clone handles over shared state, so a pipe
  or an engine can hold one, as upstream's plugins hold the editor.
- `Scope<T>` is generic over its whole signal type `T`, which is upstream's
  `Produces | Parents[number]` written as one Rust type. A child's signal type implements
  `Accepts<ParentSignal>`; `T: Accepts<T>` is built in, so a plugin that only listens uses the
  parent's type itself (the engines use `Root<S>`).
- A pipe returns `Some(context)` to continue or `None` to stop, upstream's `undefined`. Sync and
  async pipes are added with `add_pipe` / `add_async_pipe` and run in one chain, in order. A pipe
  added during an emit runs in that emit.
- Beyond upstream, which can't take a pipe back: `add_pipe` returns a `PipeId` and
  `remove_pipe(id)` takes the pipe out. An emit in progress that hasn't reached it skips it; the
  others keep their places. Solenoid works around the missing removal with a "live" flag per
  pipe.
- `use_scope(child)` sets the child's parent, then adds a pipe that holds a clone of the child
  and runs the child's chain at that point, so the parent keeps the child alive as in JS. If a
  child pipe turns the parent's signal into one the parent's type can't hold
  (`into_parent` fails), the parent's chain stops as if a pipe returned `None`; in JS the foreign
  signal would continue down the parent's chain.
- Upstream throws "cannot use non-Scope instance"; here `use_scope` only accepts a `Plugin`, so
  that mistake doesn't compile.
- A child holds its parent weakly (`ParentRef`). `has_parent` is false once the parent is gone.
  `parent_scope::<P>()` returns the parent as handle type `P`, or upstream's two errors.

## Editor

- `NodeEditor<S>` is a cheap-clone handle. Nodes and connections go in as `Rc` and come out as
  the same `Rc`; `get_nodes` / `get_connections` return a copy of the list, as upstream's
  `slice()`.
- `Schemes` has a `Node` and a `Connection` type, either of which may be a trait object;
  `BaseSchemes` is `GetSchemes<dyn NodeBase, dyn ConnectionBase>`, upstream's untyped default.
- Lookup by id is a hash lookup (upstream scans the list): Solenoid looks nodes up by id inside
  loops over nodes, which a scan makes quadratic. List order is insertion order, and removal keeps
  the rest in order, so nothing observable changes. A removal leaves a gap that a later
  compaction closes, so removing is constant time on average where shifting the list (upstream's
  `splice`) is linear. Internal maps hash with Fx (`ruggine::fxhash`), not SipHash: ids are local
  data, not hostile input. A node's id must not change while it is in the editor.
- `get_connections_to(node)` / `get_connections_from(node)` answer upstream's
  `getConnections().filter(c => c.target === node)` (or `source`) from an index, in the same
  order. The engines use them; upstream's scan per node makes a fetch quadratic in graph size.
- Root signals are an enum, `Root::NodeCreate(node)` for upstream's
  `{ type: 'nodecreate', data: node }`; `type_name()` gives the string.
- Two concurrency fixes over upstream, both where upstream corrupts its list: if the same id is
  added twice while a `nodecreate` (or `connectioncreate`) pipe is pending, the second add fails
  with "has already been added" instead of storing a duplicate; and if an item is already gone
  when its remove finishes waiting on `noderemove`, nothing is removed, where upstream's
  `splice(-1, 1)` would drop the last item.

## Classic preset

- `Node`, `Input`, `Output` keep upstream's mutable fields behind interior mutability, so they
  change through `&self` after the node is in an editor: label, selection, ports, controls,
  socket, port label, `multiple_connections`, `index`, `show_control`; Solenoid adds, removes and
  retypes ports on live nodes. Ids are plain fields, set before the node is shared.
- Holding the `Ref` from `inputs()` / `outputs()` / `controls()` while adding or removing a port
  on the same node panics; take what you need and drop it first.
- Ports are `Rc<Input<S>>` / `Rc<Output<S>>` in a `KeyMap`, in JavaScript key order (canonical
  array-index keys ascending, then insertion order; a removed and re-added key goes last). The
  engines' default inputs and outputs come from that order, so it is part of the answer.
  `input_keys()` / `output_keys()` give upstream's `Object.keys(node.inputs)` as a shared
  `Rc<[String]>`, cached until a port is added or removed, and a setup's `inputs` / `outputs`
  return the same type, so a fetch doesn't copy key lists.
  `Input` and `Output` deref to `Port`.
- `Input` takes one connection unless set otherwise, `Output` many (upstream: `undefined` versus
  `!== false`). Upstream keeps an unset input flag `undefined`; here it is `false`, which reads
  the same anywhere it is tested for truth.
- Controls are `Rc<dyn AnyControl>`; `Control` is the base class and a subclass embeds one and
  implements `AnyControl`. `node.control::<C>(key)` returns the control if it is a `C`.
  `add_control` takes the control by value and returns the `Rc` it now holds.
- `InputControl<N>`: `text` holds a `String`, `number` an `f64`. `value()` returns a copy.
  `set_value` stores the value, then calls `change` with `Option<&N>` (upstream calls it with
  `undefined` when the value is cleared), holding nothing, so the callback may set the value
  again.
- A node type built on the classic node implements `ClassicNode` (return the inner `Node` from
  `classic()`); a connection type implements `ClassicConnection`. The engines and
  `Connection::new` only read through those traits.

## Dataflow

- One value type `V: Clone` per engine stands in for JS `any`; a graph with mixed values uses an
  enum. Outputs are `KeyMap<V>`, inputs `KeyMap<Vec<V>>`.
- Inputs: one value per connection into the key, in connection order. A connection whose source
  data lacks its output adds nothing to the list (upstream pushes `undefined`); the key is still
  present.
- `DataflowNode::data` returns the engine's `Error`, so a node can pass an inner engine's
  `Error::Cancelled` up unchanged (Solenoid's composites do), or wrap its own failure with
  `Error::node`.
- The engine caches each node's run as a `Cancellable`; `reset(Some(id))` deletes the cache of the
  node and of every node reached through outputs its setup lists, depth first in connection
  order as upstream recurses, but with a work list that visits each node once. Without cycles it
  deletes exactly what upstream deletes, in the same order, and fails at the same node when one
  has no setup ("setup"). With a cycle it stops where upstream recurses until the stack
  overflows, which in wasm aborts the whole module instead of throwing. `reset(None)` and
  `reset(Some(""))` clear everything (upstream's `if (nodeId)`).
- A cached run that failed stays failed until reset, as upstream's rejected promise does.
- `engine.cache()` is upstream's `engine.cache`: `delete` an entry to make that node run again
  (cancelling a run in flight), or `add` / `patch` a `Cancellable::resolved(result)` to give a
  node a result without running it, upstream's
  `Object.assign(Promise.resolve(outputs), { cancel() {} })`. Solenoid seeds loop errors this
  way. `engine.dataflow()` is upstream's `engine.dataflow`.
- Both engines: upstream throws out of the editor's `nodecreated` pipe when the engine isn't
  attached or the node is already set up; a pipe can't fail here, so the engine skips the node,
  and a later fetch or execute fails with "node is not initialized". An engine that isn't
  attached fails with "DataflowEngine isn't attached to NodeEditor" (upstream's message) or
  "ControlFlowEngine isn't attached to NodeEditor" (upstream fails with a `TypeError` there).
- A fetch that awaits itself through a cycle never finishes, as in JS, and the futures in the
  cycle hold each other, so they are never freed (JS would collect them). Solenoid marks cycles
  as errors before fetching.

## Control flow

- `execute(node_id, input)` takes `Option<&str>`; an empty string counts as none, as upstream's
  truthiness check has it. The engine hands nodes `Option<&str>` where upstream hands them
  `String(input)`, the literal `"undefined"` for a run that starts at the node.
- `forward` returns `Result`; upstream's thrown "outputs don't have a key" reaches the caller when
  the node passes it on with `?`.

## Ids and platforms

- `get_uid` takes eight secure random bytes and prints them as sixteen lowercase hex
  characters, upstream's shape. The bytes come from getrandom 512 at a time, one call to the
  platform per 64 ids. Ids land in Solenoid's saves and get pasted between documents,
  so they need a secure source, not a counter. `uid::set_random_source` swaps the source for the
  current thread, the seam upstream's tests get by mocking `crypto`.
- Both crates build for `wasm32-unknown-unknown` and native, and CI builds both. No dependency
  may need threads or an OS. On wasm the one call out to the host is getrandom's `wasm_js`
  backend (`crypto.getRandomValues`), which every browser and Node has.

## Solenoid's calls

How Solenoid's rete calls read in Ruggine, for the port of Solenoid's graph layer
(`ruggine-engine/tests/solenoid_patterns.rs` runs these patterns: cycle seeding, invalidation, a
pass that survives a reset, composites):

| Solenoid (TS) | Ruggine |
|---|---|
| `new NodeEditor<Schemes>()` | `NodeEditor::<Schemes>::new()` |
| `editor.getNode(id)` / `getNodes()` | `get_node(id)` / `get_nodes()` |
| `await editor.addNode(node)` | `editor.add_node(rc).await?` |
| `editor.addPipe(ctx => …)` | `add_pipe(\|ctx\| …)`, matching on `Root::NodeCreated(n)` etc. |
| `new DataflowEngine<Schemes>()` + `editor.use(engine)` | `DataflowEngine::<Schemes, V>::new()` + `editor.use_scope(&engine)?` |
| `engine.fetch(id)` / `engine.reset(id)` | `engine.fetch(id).await` / `engine.reset(Some(id))` |
| `e instanceof Cancelled` | `e.is_cancelled()` |
| `engine.cache.delete(id)`, `engine.cache.add(id, seeded)` (graphCompute.ts) | `engine.cache().delete(id)`, `engine.cache().add(id, Cancellable::resolved(..))` |
| a "live" flag to neutralize a pipe on cleanup | `editor.remove_pipe(id)` |
| its own `invalidate`, because `engine.reset` overflows on a cycle | `engine.reset(Some(id))` is cycle-safe (it clears only through outputs the setups list, as upstream does) |
| `class X extends ClassicPreset.Node` | a struct holding a `classic::Node`, implementing `ClassicNode` and `DataflowNode<V>`; the scheme's node type is a trait object over them (`ruggine-engine/tests/dyn_nodes.rs`) |
