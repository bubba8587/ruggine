// [[A1]] rustRete
// Times the same workloads on upstream rete (Node), Ruggine native and Ruggine wasm (Node), and
// prints a markdown table. Usage: node tools/bench/compare.mjs [sizes, default 1000,10000] [runs]
// Needs tsc, cargo and wasm-bindgen (the CLI version in Cargo.lock) on PATH.
import { execFileSync } from 'node:child_process'
import { existsSync } from 'node:fs'
import { createRequire } from 'node:module'
import { join } from 'node:path'
import { buildUpstream, root } from '../oracle/upstream.mjs'

const sizes = (process.argv[2] ?? '1000,10000').split(',').map(Number)
const runs = Number(process.argv[3] ?? 5)
const require = createRequire(import.meta.url)
const sh = (cmd, args) => execFileSync(cmd, args, { cwd: root, encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit'] })

const upstream = buildUpstream()
const { report } = require('./upstream.cjs')

sh('cargo', ['build', '--release', '-q', '-p', 'ruggine-bench'])
sh('cargo', ['build', '--release', '-q', '-p', 'ruggine-bench', '--lib', '--target', 'wasm32-unknown-unknown'])
const wasmOut = join(root, 'target/bench-wasm')
sh('wasm-bindgen', ['--target', 'nodejs', '--out-dir', wasmOut,
  join(root, 'target/wasm32-unknown-unknown/release/ruggine_bench.wasm')])
const wasm = require(join(wasmOut, 'ruggine_bench.js'))

const names = Object.keys(JSON.parse(sh(join(root, 'target/release/ruggine-bench'), ['10', '1'])))
const fmt = ms => (typeof ms !== 'number' ? ms : ms < 1 ? ms.toFixed(3) : ms < 100 ? ms.toFixed(1) : ms.toFixed(0))
const ratio = (a, b) => (typeof a !== 'number' ? '–' : b > 0 ? (a / b).toFixed(a / b < 10 ? 1 : 0) + '×' : '–')

for (const n of sizes) {
  const js = await report(upstream, n, runs, names)
  const native = JSON.parse(sh(join(root, 'target/release/ruggine-bench'), [String(n), String(runs)]))
  wasm.report(Math.min(n, 100), 1)
  const web = JSON.parse(wasm.report(n, runs))
  console.log(`\n### ${n} nodes (median of ${runs}, ms)\n`)
  console.log('| Workload | rete (Node) | Ruggine native | Ruggine wasm | native vs rete | wasm vs rete |')
  console.log('|---|---:|---:|---:|---:|---:|')
  for (const name of names) {
    console.log(`| ${name} | ${fmt(js[name])} | ${fmt(native[name])} | ${fmt(web[name])} | ${ratio(js[name], native[name])} | ${ratio(js[name], web[name])} |`)
  }
}
