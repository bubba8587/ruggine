# Dev notes

Open problems and the latest session digests. Older digests move to
`docs/archive/dev-notes-history.md` (created when the first one moves).

## 2026-10-22 — scaffold and full port

- Scaffold: Cargo workspace, CI (fmt, clippy, tests, wasm build, `dte validate`), upstream rete
  2.0.6 / rete-engine 2.1.1 pinned in `upstream/`, the DTE tree copied from Solenoid.
- Ported all of both packages: editor, scopes and pipes, classic preset with controls, dataflow
  and control-flow engines, cache, cancellable. Every upstream test has a Rust counterpart, plus
  extra tests for behavior upstream's source states but its suite doesn't check.
- Design leaves: [[B4]] asyncNoRuntime, [[B5]] sharedHandles, [[B6]] jsKeyOrder, [[B7]]
  idsByLookup, [[B8]] resetVisitsOnce, [[B9]] randomIds; mechanics and every divergence in
  `tree/specs/port.md`. All unratified.
- Open: stack depth on very long chains (`tree/specs/port.md` § Futures); the wasm uid check
  (`docs/backlog.md`).
