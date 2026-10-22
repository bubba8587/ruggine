// [[A1]] rustRete
// The workloads of bench/src/lib.rs, on upstream rete. Each returns milliseconds.
const { performance } = require('node:perf_hooks')

function workloads({ rete, engine }) {
  const { NodeEditor, ClassicPreset } = rete
  const calc = socket => {
    const n = new ClassicPreset.Node('calc')
    n.addInput('in', new ClassicPreset.Input(socket, undefined, true))
    n.addOutput('out', new ClassicPreset.Output(socket))
    n.data = inputs => ({ out: (inputs.in || []).reduce((a, b) => a + b, 0) + 1 })
    return n
  }
  const setup = n => {
    const editor = new NodeEditor()
    const dataflow = new engine.DataflowEngine()
    editor.use(dataflow)
    const socket = new ClassicPreset.Socket('number')
    const nodes = Array.from({ length: n }, () => calc(socket))
    return {
      editor, dataflow, nodes,
      async addAll() { for (const node of nodes) await editor.addNode(node) },
      async chain() {
        for (let i = 1; i < n; i++) {
          await editor.addConnection(new ClassicPreset.Connection(nodes[i - 1], 'out', nodes[i], 'in'))
        }
      },
      async fanIn() {
        const sink = nodes[n - 1]
        for (let i = 0; i < n - 1; i++) {
          await editor.addConnection(new ClassicPreset.Connection(nodes[i], 'out', sink, 'in'))
        }
      },
      last: () => nodes[n - 1].id
    }
  }
  const time = async f => {
    const start = performance.now()
    await f()
    return performance.now() - start
  }
  return {
    async create_nodes(n) {
      const socket = new ClassicPreset.Socket('number')
      return time(() => { Array.from({ length: n }, () => calc(socket)) })
    },
    async add_nodes(n) { const s = setup(n); return time(() => s.addAll()) },
    async add_connections(n) { const s = setup(n); await s.addAll(); return time(() => s.chain()) },
    async fetch_chain_cold(n) {
      const s = setup(n); await s.addAll(); await s.chain()
      return time(() => s.dataflow.fetch(s.last()))
    },
    async fetch_chain_warm(n) {
      const s = setup(n); await s.addAll(); await s.chain(); await s.dataflow.fetch(s.last())
      return time(() => s.dataflow.fetch(s.last()))
    },
    async reset_and_refetch_chain(n) {
      const s = setup(n); await s.addAll(); await s.chain(); await s.dataflow.fetch(s.last())
      return time(() => { s.dataflow.reset(s.nodes[0].id); return s.dataflow.fetch(s.last()) })
    },
    async fetch_fan_in_cold(n) {
      const s = setup(n); await s.addAll(); await s.fanIn()
      return time(() => s.dataflow.fetch(s.last()))
    },
    async lookup_nodes(n) {
      const s = setup(n); await s.addAll()
      const ids = s.nodes.map(x => x.id)
      return time(() => { for (const id of ids) s.editor.getNode(id) })
    },
    async remove_connections(n) {
      const s = setup(n); await s.addAll(); await s.chain()
      const ids = s.editor.getConnections().map(c => c.id)
      return time(async () => { for (const id of ids) await s.editor.removeConnection(id) })
    },
    async remove_nodes_concurrently(n) {
      const s = setup(n); await s.addAll()
      const ids = s.nodes.map(x => x.id)
      return time(() => Promise.all(ids.map(id => s.editor.removeNode(id))))
    },
    async clear(n) { const s = setup(n); await s.addAll(); await s.chain(); return time(() => s.editor.clear()) }
  }
}

async function report(upstream, n, runs, names) {
  const w = workloads(upstream)
  const out = {}
  for (const name of names) {
    try {
      await w[name](Math.min(n, 100))
      const times = []
      for (let i = 0; i < runs; i++) times.push(await w[name](n))
      times.sort((a, b) => a - b)
      out[name] = times[Math.floor(times.length / 2)]
    } catch (e) {
      out[name] = e instanceof RangeError ? 'stack overflow' : String(e.message)
    }
  }
  return out
}

module.exports = { report }
