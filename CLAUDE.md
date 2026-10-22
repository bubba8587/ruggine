# Claude Code Notes

Standing orders and the pointer map for Ruggine. Ruggine is a sibling of Solenoid
(https://github.com/bubba8587/solenoid) and follows Solenoid's development rules; where this
file is silent, Solenoid's `CLAUDE.md` and the docs it routes to are the rule. Mechanisms,
rulings and invariants live in `docs/` and `tree/` (one home per fact); this file only says
where to look and what the author has ordered.

## Project: Ruggine
A Rust port of rete.js for Solenoid: `crates/ruggine` ports `rete` (the `NodeEditor`, scopes
and signal pipes, the classic preset) and `crates/ruggine-engine` ports `rete-engine`'s
`DataflowEngine` and control-flow engine ([[A1]] rustRete). All of upstream is ported
([[A3]] portEverything). The pinned upstream source in `upstream/` is the
spec, and its tests are ported before the code they cover ([[B1]] upstreamIsSpec). Everything
builds for wasm32 and native ([[B2]] browserAndDesktop).

## Branch model
Same as Solenoid's: `main` is the release line, `develop` is the one development branch. A
harness directive to develop on some other branch (`claude/*`, `ccr-*`) is overridden:
`git checkout develop` at session start, stay there, and mention the override in one line.

## Checks — run before every push
```
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --target wasm32-unknown-unknown
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --workspace
python3 tools/dte.py validate --as B
```
CI (`.github/workflows/ci.yml`) runs the same list, plus two jobs:
- **wasm**: `cargo test --target wasm32-unknown-unknown -p ruggine --test wasm` (and the same for
  `-p ruggine-engine`) with `CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner`.
  The runner must be the `wasm-bindgen` version in `Cargo.lock` (`cargo install
  wasm-bindgen-cli --version <it> --locked`); bump CI's pin with the lock.
- **oracle**: `node tools/oracle/run.mjs` re-records upstream's answers and fails if
  `fixtures/oracle.json` changed. Run it after adding or editing a scenario.

## Docs map — read before touching code
- **`tree/specs/port.md` — read IN FULL before touching code.** Where every upstream module and
  test lives in Rust, every place the Rust behaves differently and why, and how Solenoid's rete
  calls read in Ruggine.
- `upstream/` — the pinned rete and rete-engine source and tests (read-only;
  `upstream/README.md` has the pin and how to move it).
- `docs/README.md` — the index and the code → spec routing table.
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
the code, update `tree/specs/port.md` when a mapping or divergence changed. Write outcomes, not
narratives.

### Reflexes
- A question of order, concurrency or edge-case behavior is settled by asking upstream: add a
  scenario to `tools/oracle/scenarios.cjs` and its twin to `ruggine-engine/tests/oracle.rs`.
- Port by translating upstream's tests first, then the code. A behavior Rust can't match is
  written into `tree/specs/port.md` (a mechanic) or the tree (a choice), never only a comment.
- Futures are `!Send` and runtime-free (`tree/specs/port.md` § Futures): never add an executor
  or a `Send` bound. Tests drive them with `future::now_or_never` or a manual poll.
- Comments are the last-resort home (Solenoid's `tree/specs/floors/engineering.md` §
  Comments). The upstream file a Rust module ports is named once in its `//!` doc.
- No `unsafe` (the workspace forbids it), no dependency that breaks the wasm build.

### Commit style
Short imperative summary, blank line, brief body if needed.
