# Ruggine

A Rust port of [rete.js](https://retejs.org)'s headless core, built for
[Solenoid](https://github.com/bubba8587/solenoid). *Ruggine* is Italian for rust.

- `crates/ruggine`: the node editor (`NodeEditor`, scopes and signal pipes, the classic preset
  of nodes, sockets and connections), ported from `rete` 2.0.6.
- `crates/ruggine-engine`: the dataflow and control-flow engines, ported from `rete-engine`
  2.1.1.

Both packages are ported in full, upstream's test suites included, and checked against the
upstream code itself: `tools/oracle/` runs scenarios on the pinned upstream and the Rust tests
must give the same answers. `tree/specs/port.md` maps
every upstream module onto the Rust and lists where the Rust differs and why. Rendering
isn't part of rete's core and isn't part of Ruggine: Solenoid draws with React Flow.

## Building

```
cargo test --workspace
cargo build --workspace --target wasm32-unknown-unknown
```

## License

MIT. Ruggine is a port of rete and rete-engine, both MIT; see `THIRD-PARTY-NOTICES.md`.
