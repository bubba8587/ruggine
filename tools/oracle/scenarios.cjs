// [[B1]] upstreamIsSpec
// Each scenario runs against upstream rete and returns a JSON answer. oracle.rs runs the same
// scenario, by the same name, against Ruggine and must produce the same JSON.

function graph({ rete, engine }, opts = {}) {
  const { NodeEditor, ClassicPreset } = rete
  const sock = new ClassicPreset.Socket('s')
  const log = []
  const editor = new NodeEditor()
  const dataflow = new engine.DataflowEngine()
  const mk = (id, inputs, fail) => {
    const n = new ClassicPreset.Node(id)
    n.id = id
    for (const i of inputs) n.addInput(i, new ClassicPreset.Input(sock, undefined, true))
    n.addOutput('o', new ClassicPreset.Output(sock))
    n.data = () => {
      log.push(id)
      if (fail) throw new Error('fail ' + id)
      return { o: id }
    }
    return n
  }
  const link = (a, b, input) => editor.addConnection(new ClassicPreset.Connection(a, 'o', b, input))
  return { editor, dataflow, log, mk, link, attach: () => editor.use(dataflow), opts }
}

async function chain(up, fails = {}) {
  const g = graph(up)
  g.attach()
  const A = g.mk('A', [], fails.A), B = g.mk('B', ['i']), C = g.mk('C', [], fails.C), D = g.mk('D', ['b', 'c'])
  for (const n of [A, B, C, D]) await g.editor.addNode(n)
  await g.link(A, B, 'i')
  await g.link(B, D, 'b')
  await g.link(C, D, 'c')
  return g
}

const message = async promise => {
  try { await promise; return 'ok' } catch (e) { return e.message }
}

module.exports = {
  async fetch_order(up) {
    const g = await chain(up)
    const out = await g.dataflow.fetch('D')
    return { log: g.log, out }
  },
  async fetch_inputs(up) {
    const g = await chain(up)
    const inputs = await g.dataflow.fetchInputs('D')
    return { log: g.log, inputs }
  },
  async cached(up) {
    const g = await chain(up)
    await g.dataflow.fetch('D')
    await g.dataflow.fetch('D')
    return { log: g.log }
  },
  async reset_before_await(up) {
    const g = await chain(up)
    const p = g.dataflow.fetch('D')
    g.dataflow.reset('D')
    return { error: await message(p) }
  },
  async two_failures(up) {
    const g = await chain(up, { A: true, C: true })
    return { error: await message(g.dataflow.fetch('D')), log: g.log }
  },
  async reset_empty(up) {
    const g = await chain(up)
    await g.dataflow.fetch('D')
    g.dataflow.reset('')
    g.log.length = 0
    await g.dataflow.fetch('D')
    return { log: g.log }
  },
  async reset_middle(up) {
    const g = await chain(up)
    await g.dataflow.fetch('D')
    g.dataflow.reset('B')
    g.log.length = 0
    await g.dataflow.fetch('D')
    return { log: g.log }
  },
  async diamond(up) {
    const g = graph(up)
    g.attach()
    const A = g.mk('A', []), B = g.mk('B', ['i']), C = g.mk('C', ['i']), D = g.mk('D', ['b', 'c'])
    for (const n of [A, B, C, D]) await g.editor.addNode(n)
    await g.link(A, B, 'i')
    await g.link(A, C, 'i')
    await g.link(B, D, 'b')
    await g.link(C, D, 'c')
    await g.dataflow.fetch('D')
    return { log: g.log }
  },
  async fan_in(up) {
    const g = graph(up)
    g.attach()
    const A = g.mk('A', []), B = g.mk('B', []), C = g.mk('C', []), D = g.mk('D', ['i'])
    for (const n of [A, B, C, D]) await g.editor.addNode(n)
    for (const n of [B, A, C]) await g.link(n, D, 'i')
    const inputs = await g.dataflow.fetchInputs('D')
    return { log: g.log, inputs }
  },
  async missing_outputs(up) {
    const g = graph(up)
    g.attach()
    const X = g.mk('X', [])
    X.data = () => ({ other: 1 })
    await g.editor.addNode(X)
    return { error: await message(g.dataflow.fetch('X')) }
  },
  async reset_stops_at_missing_setup(up) {
    const g = graph(up)
    const A = g.mk('A', []), B = g.mk('B', ['i']), C = g.mk('C', ['i'])
    await g.editor.addNode(B)
    g.attach()
    await g.editor.addNode(A)
    await g.editor.addNode(C)
    await g.link(A, B, 'i')
    await g.link(A, C, 'i')
    await g.dataflow.fetch('C')
    const error = await message((async () => g.dataflow.reset('A'))())
    g.log.length = 0
    await g.dataflow.fetch('C')
    await g.dataflow.fetch('A')
    return { error, log: g.log }
  },
  async editor_events({ rete }) {
    const { NodeEditor } = rete
    const editor = new NodeEditor()
    const log = []
    editor.addPipe(ctx => { log.push(ctx.type); return ctx })
    await editor.addNode({ id: '1' })
    await editor.addNode({ id: '2' })
    await editor.addConnection({ id: 'c', source: '1', target: '2' })
    await editor.removeConnection('c')
    await editor.addConnection({ id: 'd', source: '2', target: '1' })
    await editor.clear()
    return { log }
  },
  async cancelled_clear({ rete }) {
    const { NodeEditor } = rete
    const editor = new NodeEditor()
    const log = []
    editor.addPipe(ctx => { log.push(ctx.type); return ctx.type === 'clear' ? undefined : ctx })
    await editor.addNode({ id: '1' })
    const result = await editor.clear()
    return { result, log, nodes: editor.getNodes().map(n => n.id) }
  },
  async concurrent_removes({ rete }) {
    const { NodeEditor } = rete
    const editor = new NodeEditor()
    const log = []
    editor.addPipe(async ctx => {
      if (ctx.type === 'noderemove' || ctx.type === 'noderemoved') {
        await Promise.resolve()
        log.push(ctx.type + ':' + ctx.data.id)
      }
      return ctx
    })
    for (let i = 1; i <= 6; i++) await editor.addNode({ id: String(i) })
    await Promise.all(['1', '2', '3'].map(id => editor.removeNode(id)))
    return { log, nodes: editor.getNodes().map(n => n.id) }
  },
  async key_order({ rete }) {
    const { ClassicPreset } = rete
    const sock = new ClassicPreset.Socket('s')
    const n = new ClassicPreset.Node('n')
    for (const k of ['b', '2', 'a', '10', '1', '01']) n.addInput(k, new ClassicPreset.Input(sock))
    const first = Object.keys(n.inputs)
    n.removeInput('b')
    n.addInput('b', new ClassicPreset.Input(sock))
    n.removeInput('2')
    n.addInput('2', new ClassicPreset.Input(sock))
    return { first, after: Object.keys(n.inputs) }
  },
  async control_order({ rete, engine }) {
    const { NodeEditor, ClassicPreset } = rete
    const sock = new ClassicPreset.Socket('s')
    const editor = new NodeEditor()
    const flow = new engine.ControlFlow(editor)
    const log = []
    const mk = id => {
      const n = new ClassicPreset.Node(id)
      n.id = id
      n.addInput('in', new ClassicPreset.Input(sock))
      n.addOutput('out', new ClassicPreset.Output(sock))
      return n
    }
    const [a, b, c, d] = ['a', 'b', 'c', 'd'].map(mk)
    for (const [s, t] of [[a, b], [a, c], [b, d]]) {
      await editor.addConnection(new ClassicPreset.Connection(s, 'out', t, 'in'))
    }
    for (const n of [a, b, c, d]) {
      flow.add(n, {
        inputs: () => ['in'],
        outputs: () => ['out'],
        execute: (input, forward) => {
          log.push(n.id + ':' + (input ?? null))
          forward('out')
          log.push(n.id + ':end')
        }
      })
    }
    flow.execute('a')
    return { log }
  }
}
