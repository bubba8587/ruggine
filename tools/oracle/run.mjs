// [[B1]] upstreamIsSpec
// Records upstream's answers to scenarios.cjs in fixtures/oracle.json, which
// crates/ruggine-engine/tests/oracle.rs replays against the port.
// Usage: node tools/oracle/run.mjs   (needs tsc on PATH)
import { writeFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { join } from 'node:path'
import { buildUpstream, root } from './upstream.mjs'

const upstream = buildUpstream()
const require = createRequire(import.meta.url)
const scenarios = require('./scenarios.cjs')
const answers = {}
for (const [name, scenario] of Object.entries(scenarios)) {
  answers[name] = await scenario(upstream)
}
writeFileSync(join(root, 'fixtures/oracle.json'), JSON.stringify(answers, null, 2) + '\n')
console.log(`wrote ${Object.keys(answers).length} answers to fixtures/oracle.json`)
