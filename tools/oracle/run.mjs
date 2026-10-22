// [[B1]] upstreamIsSpec
// Compiles the pinned upstream rete in upstream/ and records its answers to scenarios.cjs in
// fixtures/oracle.json, which crates/ruggine-engine/tests/oracle.rs replays against the port.
// Usage: node tools/oracle/run.mjs   (needs tsc on PATH)
import { cpSync, existsSync, mkdirSync, rmSync, writeFileSync } from 'node:fs'
import { execFileSync } from 'node:child_process'
import { createRequire } from 'node:module'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..')
const build = join(root, 'target/oracle')
rmSync(build, { recursive: true, force: true })
mkdirSync(build, { recursive: true })
cpSync(join(root, 'upstream/rete/src'), join(build, 'rete'), { recursive: true })
cpSync(join(root, 'upstream/rete-engine/src'), join(build, 'engine'), { recursive: true })
writeFileSync(join(build, 'tsconfig.json'), JSON.stringify({
  compilerOptions: {
    target: 'ES2020', module: 'commonjs', outDir: 'out', strict: false, skipLibCheck: true,
    paths: { rete: ['./rete/index.ts'] }, types: []
  },
  include: ['rete/**/*.ts', 'engine/**/*.ts']
}))
// Upstream type-checks against @types/node, which isn't installed here; the emitted JS is all
// that's needed, so type errors are ignored.
try {
  execFileSync('tsc', ['-p', build], { stdio: 'ignore' })
} catch {}
if (!existsSync(join(build, 'out/rete/index.js'))) throw new Error('tsc emitted nothing')
cpSync(join(build, 'out/rete'), join(build, 'out/node_modules/rete'), { recursive: true })

const require = createRequire(join(build, 'out/index.js'))
const upstream = {
  rete: require('rete'),
  engine: require(join(build, 'out/engine/index.js'))
}
const scenarios = require(join(root, 'tools/oracle/scenarios.cjs'))
const answers = {}
for (const [name, scenario] of Object.entries(scenarios)) {
  answers[name] = await scenario(upstream)
}
writeFileSync(join(root, 'fixtures/oracle.json'), JSON.stringify(answers, null, 2) + '\n')
console.log(`wrote ${Object.keys(answers).length} answers to fixtures/oracle.json`)
