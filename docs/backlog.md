# Backlog

Open items only. Delete a line once the code carries it.

- Solenoid's `rust-port` branch holds its graph model, dataflow engine and pass on Ruggine
  (`crates/solenoid-graph`), hosting JS nodes. Next there: node classes as Rust types behind one
  trait object (`tree/specs/port.md` § Solenoid's calls, `ruggine-engine/tests/solenoid_patterns.rs`).
- Small quadratic spots left (whole-crate review 2026-10-24): `Dataflow`'s `KeyMap` lookups scan
  keys per connection (a node with 500 outputs does ~250k comparisons per fetch), and
  `Wakers::register` scans every waiter, so m concurrent awaiters on one run cost m².
- Oracle gaps: no scenario removes a node and re-adds the same id while its cache holds a run.
- Decide whether Ruggine needs a `rules.test`-style guard for the tree like Solenoid's
  `tests/graph/rules.test.ts`.
