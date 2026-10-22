# Backlog

Open items only. Delete a line once the code carries it.

- Solenoid's `tsrs` branch runs its dataflow engine and pass walks on Ruggine
  (`crates/solenoid-graph`), still hosting JS nodes under a mirrored rete editor. Next there: the
  editor itself, then node classes as Rust types behind one trait object
  (`tree/specs/port.md` § Solenoid's calls, `ruggine-engine/tests/solenoid_patterns.rs`).
- Decide whether Ruggine needs a `rules.test`-style guard for the tree like Solenoid's
  `tests/graph/rules.test.ts`.
