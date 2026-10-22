# Upstream reference

The pinned rete source Ruggine ports from ([[B1]] upstreamIsSpec). Read-only: never edit
these files, re-copy them to move the pin. Only `src/`, `test/`, `LICENSE`, `README.md` and
`package.json` are kept from each repo.

| Package | Version | Repo | Commit |
|---|---|---|---|
| `rete` | 2.0.6 | https://github.com/retejs/rete | `2aae19950180dc12725306f06c0440f64473bd21` |
| `rete-engine` | 2.1.1 | https://github.com/retejs/engine | `b43af76473c60c37b7a6f29b173aca6adaacd8bf` |

These are the versions Solenoid's `package.json` pins. Both are MIT licensed
(`THIRD-PARTY-NOTICES.md`).

To move the pin: clone the repo at the new tag, replace the folder's five entries, update
this table, and diff the old and new `src/` and `test/` against the Rust port.
