# Dev notes

Open problems and the latest session digests. Older digests move to
`docs/archive/dev-notes-history.md` (created when the first one moves).

## 2026-10-22: scaffold and full port

- Ported all of rete 2.0.6 and rete-engine 2.1.1, with every upstream test translated. The tree
  holds only product calls ([[A1]] rustRete, [[A3]] portEverything, [[B1]] upstreamIsSpec,
  [[B2]] browserAndDesktop, [[B3]] preAlphaBreakFreely, all unratified); the mechanics and every
  divergence are in `tree/specs/port.md`.
- Upstream is checked directly: `tools/oracle/` runs scenarios on the pinned upstream and
  `ruggine-engine/tests/oracle.rs` must give the same answers (27 scenarios: work order,
  concurrency, cancellation, reset, errors, editor events, key order, control flow).
- Shared work runs on a per-thread queue standing in for JS's microtask queue: flat stack (50,000
  nodes native, 20,000 in debug wasm with real JS promises) and upstream's order of work.
- Wasm is tested in a real runtime (wasm-bindgen-test under Node), including getrandom's
  browser backend.
- Open: control flow stays recursive by contract (`port.md` § Futures); a fetched cycle never
  finishes and isn't freed, as upstream's never finishes.
