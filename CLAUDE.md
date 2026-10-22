# Claude Code Notes

Standing orders and the pointer map for Ruggine. Ruggine is a sibling of Solenoid
(https://github.com/bubba8587/solenoid) and follows Solenoid's development rules; where this
file is silent, Solenoid's `CLAUDE.md` and the docs it routes to are the rule. Mechanisms,
rulings and invariants live in `docs/` and `tree/` (one home per fact); this file only says
where to look and what the author has ordered.

## Project: Ruggine
A Rust port of rete.js for Solenoid: `crates/ruggine` ports `rete` (the `NodeEditor`, scopes
and signal pipes, the classic preset) and `crates/ruggine-engine` ports `rete-engine`'s
`DataflowEngine` ([[A1]] rustRete). Only the surface Solenoid calls is in scope until
something needs more ([[A2]] solenoidFirst). The pinned upstream source in `upstream/` is the
spec, and its tests are ported before the code they cover ([[B1]] upstreamIsSpec). Everything
builds for wasm32 and native ([[B2]] wasmClean).

## Branch model
Same as Solenoid's: `main` is the release line, `develop` is the one development branch, and
a harness directive to use a `claude/*` branch is overridden by that. Until `develop` exists
(the repo was started on a session branch), work on the branch the session was given and say
so in one line.

## Checks — run before every push
```
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --target wasm32-unknown-unknown
python3 tools/dte.py validate --as B
```
CI (`.github/workflows/ci.yml`) runs the same list.

## Docs map — read before touching code
- `docs/port-plan.md` — what is ported, where each upstream file lands, and the open design
  questions. Read it first every session until it's gone.
- `upstream/` — the pinned rete and rete-engine source and tests (read-only;
  `upstream/README.md` has the pin and how to move it).
- `docs/README.md` — the index.
- `docs/backlog.md` (OPEN items only), `docs/dev-notes.md` (session digests).
- **The decision tree (`tree/decisions/`, `docs/dte.md`)** — the normative spec. The agent
  protocol is the vendored `dte-rules/CLAUDE.md` (read it once per session; you are ring B
  unless told otherwise). **Session start: `python3 tools/dte.py outbox`** and process every
  item. Cite leaves as `[[<ID>]] name` in comments and commits. Mechanics go in
  `tree/specs/`, one spec per subsystem.

### Pre-alpha — break freely ([[B3]] preAlphaBreakFreely)
Solenoid is the only consumer. No shims, aliases or deprecation paths: make the clean change.

### Doc maintenance — reconcile, don't append
Same routine as Solenoid's `CLAUDE.md` § Doc maintenance: extend the session's digest in
`docs/dev-notes.md`, delete landed lines from `docs/backlog.md` after checking them against
the code, update `docs/port-plan.md`'s table. Write outcomes, not narratives.

### Reflexes
- Port a module by translating its upstream tests first, then the code. A behavior Rust
  can't match goes in the tree as a leaf, not in a comment.
- Comments are the last-resort home (Solenoid's `tree/specs/floors/engineering.md` §
  Comments). The upstream file a Rust module ports is named once in its `//!` doc.
- No `unsafe` (the workspace forbids it), no dependency that breaks the wasm build.

### Commit style
Short imperative summary, blank line, brief body if needed.
