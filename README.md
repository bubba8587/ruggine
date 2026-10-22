# Ruggine

A Rust port of [rete.js](https://retejs.org)'s headless core, built for
[Solenoid](https://github.com/bubba8587/solenoid). *Ruggine* is Italian for rust.

- `crates/ruggine`: the node editor (`NodeEditor`, scopes and signal pipes, the classic preset
  of nodes, sockets and connections), ported from `rete` 2.0.6.
- `crates/ruggine-engine`: the dataflow engine, ported from `rete-engine` 2.1.1.

Only the parts Solenoid uses are in scope for now; `docs/port-plan.md` has the map. Rendering
isn't part of rete's core and isn't part of Ruggine: Solenoid draws with React Flow.

## Building

```
cargo test --workspace
cargo build --workspace --target wasm32-unknown-unknown
```

## License

MIT. Ruggine is a port of rete and rete-engine, both MIT; see `THIRD-PARTY-NOTICES.md`.
