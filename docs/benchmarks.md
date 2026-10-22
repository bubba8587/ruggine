# Benchmarks

Ruggine against upstream rete on the same workloads, measured 2026-10-22 on a 4-core Intel Xeon
at 2.8 GHz (a shared cloud container, so expect noise of 10 to 30% between runs), Node 22 and
Rust 1.99. Rerun with `node tools/bench/compare.mjs [sizes] [runs]`.

- **rete (Node):** the pinned upstream source, compiled as in `tools/oracle/`, in Node.
- **Ruggine native:** `bench/` in release mode.
- **Ruggine wasm:** the same crate compiled to `wasm32-unknown-unknown` (release) and run in
  the same Node, so the wasm column is the closest stand-in for Ruggine in a browser.

Every workload is a graph of identical nodes, each with one many-connection input and one output
whose data adds 1 to the sum of its inputs. The workloads are defined twice, once per side,
in `bench/src/lib.rs` and `tools/bench/upstream.cjs`:

- `create_nodes`: build the nodes, each with an input and an output.
- `add_nodes`, `add_connections`: add them to an editor with a dataflow engine attached, then
  chain them.
- `fetch_chain_cold`, `fetch_chain_warm`: fetch the last node of the chain with nothing cached,
  then again with everything cached.
- `reset_and_refetch_chain`: reset the first node, which clears the whole chain, and fetch the
  last node again.
- `fetch_fan_in_cold`: one node fed by all the others, fetched with nothing cached.
- `lookup_nodes`: look up every node by id.
- `remove_connections`, `remove_nodes_concurrently`, `clear`: tear the graph down three ways.

### 1,000 nodes (median of 7 runs, ms)

| Workload | rete (Node) | Ruggine native | Ruggine wasm | native vs rete | wasm vs rete |
|---|---:|---:|---:|---:|---:|
| create_nodes | 18.1 | 0.540 | 1.0 | 33× | 18× |
| add_nodes | 7.2 | 0.630 | 1.0 | 11× | 6.8× |
| add_connections | 15.5 | 1.1 | 2.3 | 14× | 6.8× |
| fetch_chain_cold | 20.5 | 1.9 | 2.6 | 11× | 7.8× |
| fetch_chain_warm | 0.020 | 0.003 | 0.007 | 5.9× | 2.7× |
| reset_and_refetch_chain | 48.5 | 2.7 | 3.0 | 18× | 16× |
| fetch_fan_in_cold | 30.7 | 1.3 | 1.5 | 24× | 21× |
| lookup_nodes | 6.4 | 0.013 | 0.056 | 483× | 116× |
| remove_connections | 3.2 | 0.547 | 1.5 | 5.9× | 2.2× |
| remove_nodes_concurrently | 15.6 | 0.469 | 0.821 | 33× | 19× |
| clear | 7.0 | 1.1 | 3.1 | 6.3× | 2.2× |

### 10,000 nodes (median of 7 runs, ms)

| Workload | rete (Node) | Ruggine native | Ruggine wasm | native vs rete | wasm vs rete |
|---|---:|---:|---:|---:|---:|
| create_nodes | 197 | 14.4 | 12.3 | 14× | 16× |
| add_nodes | 666 | 8.6 | 7.3 | 78× | 91× |
| add_connections | 772 | 16.6 | 19.6 | 46× | 39× |
| fetch_chain_cold | 1767 | 30.1 | 48.7 | 59× | 36× |
| fetch_chain_warm | 0.028 | 0.006 | 0.012 | 4.5× | 2.4× |
| reset_and_refetch_chain | stack overflow | 88.1 | 73.0 | – | – |
| fetch_fan_in_cold | 1458 | 30.8 | 31.9 | 47× | 46× |
| lookup_nodes | 545 | 0.829 | 0.615 | 657× | 886× |
| remove_connections | 168 | 18.3 | 13.5 | 9.2× | 12× |
| remove_nodes_concurrently | 583 | 8.7 | 7.7 | 67× | 76× |
| clear | 226 | 25.2 | 22.0 | 9.0× | 10× |

## Reading it

- The gap grows with the graph because several of rete's operations are linear where Ruggine's
  are constant: `getNode` scans the node list, a fetch scans every connection once per node
  (so a whole-graph fetch is quadratic), and `addNode` checks for duplicates with a scan. Ruggine
  keeps an id index and indexes connections by node (`tree/specs/port.md` § Editor).
- rete's `reset` recurses once per node downstream; at 10,000 nodes in a chain it overflows
  Node's stack. Ruggine walks the graph with a work list.
- wasm runs close to native here, sometimes ahead, because the wasm build's allocator happens
  to suit this allocation pattern.

## Headroom

In a profile of a cold 3,000-node fetch, about half the instructions are memory allocation:
roughly 20 small allocations per node, mostly boxed futures and copies of id strings. That's
the cost of keeping rete's model of shared objects and promises. Interning ids and storing
futures in place of boxes would cut it, if a workload ever needs more speed.
