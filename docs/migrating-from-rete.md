# Migrating from rete

How rete and rete-engine code reads in Ruggine. Every public symbol of `rete` 2.0.6 and
`rete-engine` 2.1.1 has a home here; where behavior differs, `tree/specs/port.md` says how and
why. Names are rete's in snake_case.

## The shape of things

| In rete (TypeScript) | In Ruggine (Rust) |
|---|---|
| Objects shared by reference | `Rc`: the editor stores the `Rc` you hand it and gives back clones of it |
| Mutating a node after adding it (`node.addInput(...)`) | The same, through `&self`: `node.add_input(...)` on the `Rc` |
| `class MyNode extends ClassicPreset.Node` | A struct holding a `classic::Node`, implementing `NodeBase` and `ClassicNode` (return the inner node from `classic()`) |
| Many node classes in one editor | A trait over them, and that trait object as the scheme's node type: `GetSchemes<dyn MyNode, Connection>` |
| `async` methods returning promises | Methods returning futures. Any executor drives them; `future::now_or_never` finishes a fully synchronous graph |
| `throw` | `Result`, with rete's message as the error's `Display` |
| `any` values | One value type per engine, `DataflowEngine<S, V>`; use an enum for mixed values |
| Plain objects keyed by name (`node.inputs`, a node's data) | `KeyMap`, which keeps JavaScript's key order |

## rete

| rete | Ruggine |
|---|---|
| `new NodeEditor<Schemes>()` | `NodeEditor::<Schemes>::new()` (a cheap-clone handle) |
| `editor.getNode(id)`, `getNodes()` | `get_node(id)`, `get_nodes()` |
| `editor.getConnection(id)`, `getConnections()` | `get_connection(id)`, `get_connections()` |
| `getConnections().filter(c => c.target === id)` | `get_connections_to(id)` (also `get_connections_from`), without the scan |
| `await editor.addNode(node)` | `editor.add_node(rc).await?` |
| `await editor.addConnection(c)` | `editor.add_connection(rc).await?` |
| `await editor.removeNode(id)`, `removeConnection(id)` | `remove_node(id).await?`, `remove_connection(id).await?` |
| `await editor.clear()` | `editor.clear().await?` |
| `editor.addPipe(ctx => ...)` | `editor.add_pipe(\|ctx\| ...)`, returning a `PipeId`; `add_async_pipe` for an async pipe |
| a pipe returning `undefined` | returning `None` |
| (no way to remove a pipe) | `editor.remove_pipe(id)` |
| `ctx.type === 'nodecreated'`, `ctx.data` | `match ctx { Root::NodeCreated(node) => ..., _ => ... }`; `ctx.type_name()` gives the string |
| `editor.use(plugin)` | `editor.use_scope(&plugin)?` |
| `new Scope<Produces, Parents>('name')` | `Scope::<Signal>::new("name")`, where `Signal` is one type for `Produces \| Parents[number]` |
| a child scope's signals including its parent's | the child's signal type implements `Accepts<ParentSignal>` |
| `scope.parentScope(NodeEditor)` | `scope.parent_scope::<NodeEditor<S>>()?` |
| `scope.parentScope()`, `hasParent()`, `setParent(p)` | `parent_any()`, `has_parent()`, `set_parent(p.parent_ref())` |
| a plugin overriding `setParent` | implementing `Plugin<ParentSignal>` with its own `set_parent` |
| `new Signal<T>()`, `signal.addPipe`, `signal.emit` | `Signal::<T>::new()`, `add_pipe`, `emit` |
| `getUID()` | `get_uid()`; `uid::set_random_source` replaces the source (rete's tests mock `crypto`) |
| `NodeId`, `ConnectionId`, `NodeBase`, `ConnectionBase` | the same names (the bases are traits) |
| `GetSchemes<N, C>`, `BaseSchemes` | `GetSchemes<N, C>`, `BaseSchemes` (`GetSchemes<dyn NodeBase, dyn ConnectionBase>`) |
| `CanAssignSignal`, `NestedScope`, `ScopeAsParameter`, `useHelper().debug` | compile-time checks: the `Accepts` and `Plugin` bounds |

## ClassicPreset

| rete | Ruggine (`ruggine::classic`) |
|---|---|
| `new Socket('number')` | `Socket::new("number")` |
| `new Input(socket, 'Label', true)` | `Input::new(socket).with_label("Label").with_multiple_connections(true)` |
| `new Output(socket)` | `Output::new(socket)` (many connections unless told otherwise, as in rete) |
| `input.socket = other` | `input.set_socket(other)`; likewise `set_label`, `set_index`, `set_show_control` |
| `input.addControl(c)`, `input.control`, `removeControl()` | `add_control(c)?`, `control()`, `remove_control()` |
| `new Node('label')` | `Node::new("label")`; set `node.id` before sharing it if you choose your own ids |
| `node.addInput(key, input)`, `hasInput`, `removeInput` | `add_input(key, input)?`, `has_input`, `remove_input` |
| `node.inputs.a` | `node.input("a")`; `node.inputs()` for the whole map |
| `Object.keys(node.inputs)` | `node.input_keys()` (cached) |
| outputs, controls | the same, `output`/`add_output`/..., `control`/`add_control`/... |
| `node.controls.ctrl as InputControl<'number'>` | `node.control::<InputControl<f64>>("ctrl")` |
| `node.label`, `node.selected` | `label()` / `set_label`, `selected()` / `set_selected` |
| `new Control()` | `Control::new()`; a custom control embeds one and implements `AnyControl` |
| `new InputControl('number', { initial, readonly, change })` | `InputControl::number(InputControlOptions { initial, readonly, change })` |
| `control.value`, `setValue(v)` | `value()`, `set_value(Some(v))` |
| `new Connection(a, 'out', b, 'in')` | `Connection::new(&a, "out", &b, "in")?` |
| `ClassicScheme` (rete-engine) | `classic::ClassicScheme` |

## rete-engine

| rete-engine | Ruggine (`ruggine_engine`) |
|---|---|
| `new DataflowEngine<Schemes>()` | `DataflowEngine::<Schemes, V>::new()` |
| `new DataflowEngine(node => ({ inputs: () => [...], outputs: () => [...] }))` | `DataflowEngine::with_configure(\|node\| PortSelection { inputs: ..., outputs: ... })` |
| `class N { data(inputs) { return { out } } }` | `impl DataflowNode<V> for N { fn data(&self, inputs) -> ... }`, returning `future::ready(Ok(keymap! { "out" => v }))` or an async block |
| `await engine.fetch(id)` | `engine.fetch(id).await?`, giving `Rc<Outputs<V>>` |
| `await engine.fetchInputs(id)` | `engine.fetch_inputs(id).await?` |
| `engine.reset(id)`, `engine.reset()` | `engine.reset(Some(id))?`, `engine.reset(None)?` (cycle-safe) |
| `e instanceof Cancelled` | `e.is_cancelled()` |
| `engine.cache.delete(id)` | `engine.cache().delete(id)` |
| `engine.cache.add(id, Object.assign(Promise.resolve(o), { cancel() {} }))` | `engine.cache().add(id.into(), Cancellable::resolved(Ok(Rc::new(o))))?` |
| `engine.dataflow` | `engine.dataflow()` |
| `new Dataflow(editor)`, `dataflow.add(node, setup)` | `Dataflow::new(&editor)`, `dataflow.add(&node, DataflowNodeSetup::new(inputs, outputs, data))?` |
| `dataflow.setups.has(id)` | `dataflow.has(id)`; `dataflow.setup(id)` |
| `new ControlFlowEngine<Schemes>()`, `engine.execute(id)` | `ControlFlowEngine::<Schemes>::new()`, `engine.execute(id, None)?` |
| `class N { execute(input, forward) { forward('out') } }` | `impl ControlFlowNode for N { fn execute(&self, input, forward) -> ... { forward("out") } }` |
| `engine.controlflow`, `new ControlFlow(editor)` | `engine.control_flow()`, `ControlFlow::new(&editor)` |
| `createCancellblePromise(step1, step2)` | `Cancellable::new(\|commit\| async move { step1().await; commit.check()?; ... })` |

## What to watch for

- **Hold `Rc`s, not values.** `add_node` takes an `Rc`; keep a clone if you'll touch the node
  again.
- **Don't hold a borrowed view across a mutation.** `node.inputs()` returns a borrow; adding a
  port while holding it panics. Take what you need first.
- **Futures are single-threaded** (`!Send`), like JavaScript. Use a local executor
  (`wasm_bindgen_futures::spawn_local`, Tokio's `LocalSet`) for async nodes.
- **Background work:** in JS, work nobody awaits any more still finishes. Spawn
  `ruggine::future::background()` once on your executor for the same, or it waits for your next
  Ruggine call.
- **Timing under concurrency:** results, events and the work that runs match rete's. The exact
  interleaving of several fetches in flight at once can differ (`tree/specs/port.md` §
  Futures).
