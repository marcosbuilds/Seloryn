# P0 CPU `.sely` workloads

These workloads are the pre-registered performance campaign for the
post-organization runtime mission. They execute the public Seloryn binary and
are compared as baseline A versus candidate B with identical sources, seeds,
toolchain, release profile and host.

| Workload | Path | Dominant path | Scale |
|---|---|---|---|
| Array aggregation | `array-aggregation.sely` | `range` allocation, list iteration, branch, integer arithmetic | 750,000 loop elements, below the 1,000,000 loop cap |
| Tensor loop | `tensor-loop.sely` | repeated 32x32 CPU `matmul` and reduction | 250 matmuls, deterministic seed |
| Numeric JSON pipeline | `numeric-json-pipeline/` | JSON ingestion, typed list indexing, nested numeric loops | 16 input values x 50,000 passes |

The three cases cover a near-limit interpreted array loop, a medium tensor
kernel loop and a real file-ingestion pipeline. They return deterministic
scalar checksums. The independent oracle is
`benchmarks/p0/reference.py`; it uses only Python standard-library arithmetic
and the checked-in JSON input.

Acceptance metrics are pre-registered before optimization: at least 30 valid
paired measurements per case, at least 5 warmups per binary, balanced A/B
ordering, correctness before timing, median, p95, raw samples, output hashes,
binary hashes, startup/parse timing separated from steady execution where the
instrument permits, and memory/allocation evidence where available. The
ambitious gate is hotspot median >=2.0x and end-to-end primary workload >=1.5x;
otherwise the result is not a performance PASS.

No workload may be shortened, special-cased by filename, cached by output, or
replaced by host-language code. Any optimized result must match the oracle and
the baseline output checksum within the declared numeric tolerance.
