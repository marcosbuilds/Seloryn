# CPU-first tabular niche benchmark

This is a reviewed local measurement, not a general performance claim and not
an assertion that the whole language is production-ready. The raw run records,
generated datasets and reference programs remain in the private operator
evidence directory so generated data does not become repository noise.

This is a historical snapshot, not a measurement of the current `origin/master`:
the recorded run used public commit `fff625e`, language `0.08` and runtime
`0.8.0`. It must not be used as current `0.12` performance, compatibility or
production evidence without rerunning the workloads.

## Environment and method

- source/runtime: public commit `fff625e`, language `0.08`, runtime `0.8.0`;
- host: Windows 10, 8 logical processors, Rust `1.99.0`, Python `3.10.0`,
  NumPy `2.2.6`;
- five fresh processes per positive implementation/workload;
- wall-clock includes process startup, input and output; RSS is sampled every
  2 ms and is an observed estimate, not a formal memory limit;
- p95 uses the inclusive 95th percentile of the five-run group.

## Workloads

1. **Validation and aggregation:** 8,192 JSON rows with numeric arrays for
   `revenue`, `units` and `target`; Seloryn converts bounded arrays to typed
   lists, validates rows and calculates business aggregates.
2. **Small tabular MLP:** 4,096 training rows, 1,024 holdout rows, 8 input
   features, 24 hidden units, 700 SGD updates, batch 64 and deterministic
   checkpointing. Seloryn uses the currently closed `training.train_mlp`
   profile; Python references reproduce the algorithm independently.

## Measurements

| Workflow | Implementation | Median | p95 | RSS median |
|---|---|---:|---:|---:|
| Validation/aggregation | Seloryn | 2.476 s | 2.534 s | 12.54 MiB |
| Validation/aggregation | Python | 0.137 s | 0.165 s | 12.05 MiB |
| Validation/aggregation | Python + NumPy | 0.312 s | 0.326 s | 23.17 MiB |
| MLP training | Seloryn/Rust | 0.204 s | 0.229 s | 10.02 MiB |
| MLP training | Python scalar F32 | 17.545 s | 18.241 s | 28.84 MiB |
| MLP training | Python + NumPy | 0.421 s | 0.455 s | 26.18 MiB |

The aggregate fields matched the Python reference. The MLP final and holdout
loss differences against the scalar reference stayed below `2.2e-8`; five
Seloryn checkpoints had the same SHA-256
`87ed360a00b69f60a9ec49c9cb2faa3df078237e893d57d476fcd531851adb85`.
The JSON2 receipt verifier accepted four events with exit code `0`.

Invalid inputs were also repeated: a non-numeric array produced `SEL-R127`,
and a row with the wrong feature dimension produced `SEL-R203`; both returned
exit code `65` consistently.

## Correct interpretation

The evidence supports a concrete niche: bounded local CPU workflows for table
validation, aggregation and small reproducible training with holdout metrics,
deterministic checkpoints and verifiable receipts. It does **not** support a
general speed or memory advantage. In particular, Seloryn's aggregation was
slower than both Python references, while the MLP advantage comes from a
closed native trainer rather than from a general optimized Seloryn loop.

The next production gates are still real implementation work: a supported
installer and ABI policy, `doctor/init` capability preparation, schema/zip
data APIs, batch inference from a checkpoint, interruption recovery, and
concurrency/compatibility testing. Serving, registry, distributed execution
and general GPU tensor/MLP training remain outside this evidence.
