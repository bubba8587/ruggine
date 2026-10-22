// [[B1]] upstreamIsSpec
// Random operation sequences, run on upstream and recorded with their traces; oracle_fuzz.rs
// replays the same operations on Ruggine and must produce the same traces.

function rng(seed) {
  let a = seed >>> 0
  return () => {
    a = (a + 0x6d2b79f5) >>> 0
    let t = a
    t = Math.imul(t ^ (t >>> 15), t | 1)
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61)
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

function generate(seed, length, concurrent) {
  const r = rng(seed)
  const pick = list => list[Math.floor(r() * list.length)]
  const nodes = []
  const connections = []
  let next = 0
  const ops = []
  while (ops.length < length) {
    const roll = r()
    if (roll < 0.25 || nodes.length < 2) {
      const id = 'n' + next++
      nodes.push(id)
      ops.push({ op: 'addNode', id, yields: r() < 0.3 ? 1 + Math.floor(r() * 3) : 0, fail: r() < 0.08 })
    } else if (roll < 0.5) {
      const a = Math.floor(r() * nodes.length)
      const b = Math.floor(r() * nodes.length)
      if (a === b) continue
      const [s, t] = a < b ? [nodes[a], nodes[b]] : [nodes[b], nodes[a]]
      if (Number(s.slice(1)) > Number(t.slice(1))) continue
      const id = 'c' + ops.length
      connections.push(id)
      ops.push({ op: 'addConnection', id, source: s, target: t, input: r() < 0.5 ? 'a' : 'b' })
    } else if (roll < 0.58 && connections.length) {
      const id = pick(connections)
      connections.splice(connections.indexOf(id), 1)
      ops.push({ op: 'removeConnection', id })
    } else if (roll < 0.62 && nodes.length > 2) {
      const id = pick(nodes)
      nodes.splice(nodes.indexOf(id), 1)
      ops.push({ op: 'removeNode', id })
    } else if (roll < 0.77) {
      ops.push({ op: 'fetch', id: pick(nodes) })
    } else if (roll < 0.84) {
      if (!concurrent) continue
      ops.push({ op: 'fetchAll', ids: [pick(nodes), pick(nodes), pick(nodes)] })
    } else if (roll < 0.89) {
      ops.push({ op: 'fetchInputs', id: pick(nodes) })
    } else {
      ops.push({ op: 'reset', id: r() < 0.2 ? null : pick(nodes) })
    }
  }
  return ops
}

async function run({ rete, engine }, ops) {
  const { NodeEditor, ClassicPreset } = rete
  const sock = new ClassicPreset.Socket('s')
  const trace = []
  const editor = new NodeEditor()
  const dataflow = new engine.DataflowEngine()
  editor.use(dataflow)
  editor.addPipe(ctx => {
    if (ctx.data) trace.push(`evt ${ctx.type} ${ctx.data.id}`)
    else trace.push(`evt ${ctx.type}`)
    return ctx
  })
  const byId = {}
  const outcome = async (label, promise, show) => {
    try {
      trace.push(`${label} -> ${show(await promise)}`)
    } catch (e) {
      trace.push(`${label} !! ${e.message}`)
    }
  }
  for (const op of ops) {
    switch (op.op) {
      case 'addNode': {
        const n = new ClassicPreset.Node(op.id)
        n.id = op.id
        n.addInput('a', new ClassicPreset.Input(sock, undefined, true))
        n.addInput('b', new ClassicPreset.Input(sock, undefined, true))
        n.addOutput('o', new ClassicPreset.Output(sock))
        n.data = async inputs => {
          trace.push(`data ${op.id}`)
          for (let i = 0; i < op.yields; i++) await Promise.resolve()
          if (op.fail) throw new Error('fail ' + op.id)
          return { o: `${op.id}(${(inputs.a || []).join(',')}|${(inputs.b || []).join(',')})` }
        }
        byId[op.id] = n
        await outcome(`addNode ${op.id}`, editor.addNode(n), String)
        break
      }
      case 'addConnection': {
        const c = new ClassicPreset.Connection(byId[op.source], 'o', byId[op.target], op.input)
        c.id = op.id
        await outcome(`addConnection ${op.id}`, editor.addConnection(c), String)
        break
      }
      case 'removeConnection':
        await outcome(`removeConnection ${op.id}`, editor.removeConnection(op.id), String)
        break
      case 'removeNode':
        await outcome(`removeNode ${op.id}`, editor.removeNode(op.id), String)
        break
      case 'fetch':
        await outcome(`fetch ${op.id}`, dataflow.fetch(op.id), out => out.o)
        break
      case 'fetchAll':
        await outcome(`fetchAll ${op.ids.join(',')}`, Promise.all(op.ids.map(id => dataflow.fetch(id))), outs => outs.map(o => o.o).join(' '))
        break
      case 'fetchInputs':
        await outcome(`fetchInputs ${op.id}`, dataflow.fetchInputs(op.id), JSON.stringify)
        break
      case 'reset':
        await outcome(`reset ${op.id}`, (async () => dataflow.reset(op.id ?? undefined))(), () => 'ok')
        break
    }
  }
  await new Promise(r => setTimeout(r, 0))
  trace.push('end')
  return trace
}

module.exports = { generate, run }
