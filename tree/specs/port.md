---
aliases: ["Port mapping"]
tags: [spec, port]
---
<!-- [[B1]] upstreamIsSpec, [[B4]] asyncNoRuntime, [[B5]] sharedHandles, [[B6]] jsKeyOrder, [[B7]] idsByLookup, [[B8]] resetVisitsOnce, [[B9]] randomIds -->

# Spec: how upstream maps onto Ruggine

Serves [[B1]] upstreamIsSpec. Upstream is `rete` 2.0.6 and `rete-engine` 2.1.1 in `upstream/`.
Every upstream module and test has a Rust home; this spec says where, and lists every place the
Rust behaves differently, each with its reason. A difference not listed here is a bug.

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
| (promises, `Promise.all`) | `ruggine::future` | `ruggine/tests/future.rs` |

Names are upstream's in snake_case. A JS method that throws returns `Result`, with upstream's
message as the error's `Display`. `editor.use(x)` is `use_scope` (`use` is a Rust keyword).
`rete-engine/test/types.test-d.ts` checks TypeScript inference only and has no Rust counterpart:
values are one type `V` per engine (see Dataflow).

## Futures ([[B4]] asyncNoRuntime)

- Every upstream `async` method returns a future; nothing in Ruggine spawns or blocks.
  `future::now_or_never` polls once, and a graph with only synchronous pipes and nodes is always
  done after it.
- JavaScript promises start work at once; Rust futures start when first polled. Upstream code
  always awaits right away, so the order of work is the same.
- `Promise.all` is `join_all` / `try_join_all`, which poll their futures in input order. On an
  error `try_join_all` drops the rest, where JS lets them run on. Work cached in the engine is
  not lost: the cache still holds the shared future, and the next fetch resumes it.
- A promise held in several places is `future::Shared`. It runs once and hands each awaiter a
  clone of the output, so cached outputs are `Rc<Outputs<V>>` and errors are `Clone`.
- Recursion depth: a fetch nests one poll frame per node in the longest chain it walks, and
  control flow nests one call per forward. Very deep graphs can exhaust the stack where JS would
  not (fetch) or would throw `RangeError` (control flow).

## Scopes ([[B5]] sharedHandles)

- `Scope<T>` is generic over its whole signal type `T`, which is upstream's
  `Produces | Parents[number]` written as one Rust type. A child's signal type implements
  `Accepts<ParentSignal>`; `T: Accepts<T>` is built in, so a plugin that only listens uses the
  parent's type itself (the engines use `Root<S>`).
- A pipe returns `Some(context)` to continue or `None` to stop, upstream's `undefined`. Sync and
  async pipes are added with `add_pipe` / `add_async_pipe` and run in one chain, in order. A pipe
  added during an emit runs in that emit.
- `use_scope(child)` sets the child's parent, then adds a pipe that holds a clone of the child
  and runs the child's chain at that point, so the parent keeps the child alive as in JS. If a child pipe turns the parent's signal into one the parent's type can't hold
  (`into_parent` fails), the parent's chain stops as if a pipe returned `None`; in JS the foreign
  signal would continue down the parent's chain.
- Upstream throws "cannot use non-Scope instance"; here `use_scope` only accepts a `Plugin`, so
  that mistake doesn't compile.
- A child holds its parent weakly (`ParentRef`). `has_parent` is false once the parent is gone.
  `parent_scope::<P>()` returns the parent as handle type `P`, or upstream's two errors.

## Editor ([[B5]] sharedHandles, [[B7]] idsByLookup)

- `NodeEditor<S>` is a cheap-clone handle. Nodes and connections go in as `Rc` and come out as
  the same `Rc`; `get_nodes` / `get_connections` return a copy of the list, as upstream's
  `slice()`.
- `Schemes` has a `Node` and a `Connection` type, either of which may be a trait object;
  `BaseSchemes` is `GetSchemes<dyn NodeBase, dyn ConnectionBase>`, upstream's untyped default.
- Lookup by id is a hash lookup; list order is insertion order, and removal keeps the rest in
  order. A node's id must not change while it is in the editor.
- Root signals are an enum, `Root::NodeCreate(node)` for upstream's
  `{ type: 'nodecreate', data: node }`; `type_name()` gives the string.
- Two concurrency fixes over upstream, both where upstream corrupts its list: if the same id is
  added twice while a `nodecreate` (or `connectioncreate`) pipe is pending, the second add fails
  with "has already been added" instead of storing a duplicate; and if an item is already gone
  when its remove finishes waiting on `noderemove`, nothing is removed, where upstream's
  `splice(-1, 1)` would drop the last item.

## Classic preset ([[B5]] sharedHandles, [[B6]] jsKeyOrder)

- `Node`, `Input`, `Output` keep upstream's mutable fields behind interior mutability, so they
  change through `&self` after the node is in an editor: label, selection, ports, controls,
  socket, port label, `multiple_connections`, `index`, `show_control`. Ids are plain fields, set
  before the node is shared.
- Holding the `Ref` from `inputs()` / `outputs()` / `controls()` while adding or removing a port
  on the same node panics; take what you need and drop it first.
- Ports are `Rc<Input<S>>` / `Rc<Output<S>>` in a `KeyMap`, in JavaScript key order (canonical
  array-index keys ascending, then insertion order). `Input` and `Output` deref to `Port`.
- `Input` takes one connection unless set otherwise, `Output` many (upstream: `undefined` versus
  `!== false`). Upstream keeps an unset input flag `undefined`; here it is `false`, which reads
  the same anywhere it is tested for truth.
- Controls are `Rc<dyn AnyControl>`; `Control` is the base class and a subclass embeds one and
  implements `AnyControl`. `node.control::<C>(key)` returns the control if it is a `C`.
  `add_control` takes the control by value and returns the `Rc` it now holds.
- `InputControl<N>`: `text` holds a `String`, `number` an `f64`. `change` receives
  `Option<&N>`, because upstream calls it with `undefined` when the value is cleared.
- A node type built on the classic node implements `ClassicNode` (return the inner `Node` from
  `classic()`); a connection type implements `ClassicConnection`. The engines and
  `Connection::new` only read through those traits.

## Dataflow ([[B6]] jsKeyOrder, [[B8]] resetVisitsOnce)

- One value type `V: Clone` per engine stands in for JS `any`; a graph with mixed values uses an
  enum. Outputs are `KeyMap<V>`, inputs `KeyMap<Vec<V>>`.
- Inputs: one value per connection into the key, in connection order. A connection whose source
  data lacks its output adds nothing to the list (upstream pushes `undefined`); the key is still
  present.
- `DataflowNode::data` returns the engine's `Error`, so a node can pass an inner engine's
  `Error::Cancelled` up unchanged (Solenoid's composites do), or wrap its own failure with
  `Error::node`.
- The engine caches each node's run as a `Cancellable`; `reset(Some(id))` deletes the cache of the
  node and of every node reached through outputs its setup lists, visiting each node once with a
  work list. On a graph without cycles this deletes exactly what upstream deletes; with a cycle
  it stops where upstream overflows the stack. `reset(None)` clears everything.
- A cached run that failed stays failed until reset, as upstream's rejected promise does.
- Upstream's engine throws out of the editor's `nodecreated` pipe when it isn't attached or the
  node is already set up; a pipe can't fail here, so the engine skips the node, and a later
  fetch fails with "node is not initialized".
- A fetch that awaits itself through a cycle never finishes, as in JS.

## Control flow

- `execute(node_id, input)` takes `Option<&str>`; an empty string counts as none, as upstream's
  truthiness check has it. The engine hands nodes `Option<&str>` where upstream hands them
  `String(input)`, the literal `"undefined"` for a run that starts at the node.
- `forward` returns `Result`; upstream's thrown "outputs don't have a key" reaches the caller when
  the node passes it on with `?`.

## Ids ([[B9]] randomIds)

`get_uid` reads eight bytes from getrandom (`crypto.getRandomValues` on wasm) and prints them as
lowercase hex. `uid::set_random_source` swaps the source for the current thread, the seam
upstream's tests get by mocking `crypto`.

## Solenoid's calls

How Solenoid's rete calls read in Ruggine, for the port of Solenoid's graph layer:

| Solenoid (TS) | Ruggine |
|---|---|
| `new NodeEditor<Schemes>()` | `NodeEditor::<Schemes>::new()` |
| `editor.getNode(id)` / `getNodes()` | `get_node(id)` / `get_nodes()` |
| `await editor.addNode(node)` | `editor.add_node(rc).await?` |
| `editor.addPipe(ctx => …)` | `add_pipe(\|ctx\| …)`, matching on `Root::NodeCreated(n)` etc. |
| `new DataflowEngine<Schemes>()` + `editor.use(engine)` | `DataflowEngine::<Schemes, V>::new()` + `editor.use_scope(&engine)?` |
| `engine.fetch(id)` / `engine.reset(id)` | `engine.fetch(id).await` / `engine.reset(Some(id))` |
| `e instanceof Cancelled` | `e.is_cancelled()` |
| `class X extends ClassicPreset.Node` | a struct holding a `classic::Node`, implementing `ClassicNode` and `DataflowNode<V>`; the scheme's node type is a trait object over them (`ruggine-engine/tests/dyn_nodes.rs`) |
