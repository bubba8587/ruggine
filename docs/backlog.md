# Backlog

Open items only. Delete a line once the code carries it.

- Solenoid's `tsrs` branch moves its graph layer into its Rust core on top of Ruggine
  ([[B10]] rustOnly): node classes become types behind one trait object, and graphCompute.ts's
  passes run on `DataflowEngine` (`tree/specs/port.md` § Solenoid's calls,
  `ruggine-engine/tests/solenoid_patterns.rs`).
- Decide whether Ruggine needs a `rules.test`-style guard for the tree like Solenoid's
  `tests/graph/rules.test.ts`.
