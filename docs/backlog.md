# Backlog

Open items only. Delete a line once the code carries it.

- Port Solenoid's graph layer onto Ruggine on the `tsrs` branch: its node classes become types
  behind one trait object (`tree/specs/port.md` § Solenoid's calls). Open there: whether Ruggine
  ever gets its own `wasm-bindgen` surface, or only runs inside Solenoid's core.
- Run the uid test on wasm (wasm-bindgen-test under Node) to prove getrandom's browser backend.
- Decide whether Ruggine needs a `rules.test`-style guard for the tree like Solenoid's
  `tests/graph/rules.test.ts`.
