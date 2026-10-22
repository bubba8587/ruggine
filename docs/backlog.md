# Backlog

Open items only. Delete a line once the code carries it.

- Author's call: how JS code reaches Ruggine. Either Solenoid's graph layer moves into its Rust
  core and uses Ruggine there (node classes become types behind one trait object,
  `tree/specs/port.md` § Solenoid's calls), or Ruggine ships a JS package with rete's API.
  Until then no JS code can use Ruggine.
- Decide whether Ruggine needs a `rules.test`-style guard for the tree like Solenoid's
  `tests/graph/rules.test.ts`.
