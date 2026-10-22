# Backlog

Open items only. Delete a line once the code carries it.

- Answer the design questions in `port-plan.md` as leaves, then port module by module
  (tests first), starting with `ruggine::scope` and `ruggine::editor`.
- Create `main` and `develop` once the scaffold is merged, so the branch model in
  `CLAUDE.md` applies.
- Wire Ruggine into Solenoid's `tsrs` branch once the editor and engine pass upstream's tests.
- Decide whether Ruggine needs a `rules.test`-style guard for the tree like Solenoid's
  `tests/graph/rules.test.ts`.
