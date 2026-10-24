# Seloryn Benchmark Suite

This is a reproducible, CPU-first benchmark harness for six bounded contracts:

| ID | Workload | Seloryn path | Independent reference |
|---|---|---|---|
| BENCH-01 | JSON Validation | `examples/projects/json-config` | `references/bench_01_json_validation.py` |
| BENCH-02 | Data Transformation | `examples/projects/json-array-pipeline` | `references/bench_02_data_transformation.py` |
| BENCH-03 | Numerical Computation | `fixtures/bench-03-numerical-computation.sely` | `references/bench_03_numerical_computation.py` |
| BENCH-04 | Small Neural Network | `examples/projects/self-hosted-mlp` | `references/bench_04_small_neural_network.py` |
| BENCH-05 | Language Model | `examples/projects/self-hosted-language-model` | `references/bench_05_language_model.py` |
| BENCH-06 | Error Handling | `fixtures/bench-06-error-handling.sely` | `references/bench_06_error_handling.py` |

The references use Python's standard library only. The runner measures
correctness separately from wall-clock time, records every sample and failure,
reports the median and p95, and writes both JSON and Markdown reports. It does
not compute an aggregate score or claim that Seloryn is generally faster or
better.

## Run

Build the local binary first, then run from the repository root:

```powershell
cargo build --release --locked
python benchmarks/runner.py --samples 3 --warmup 1
python benchmarks/validate_json.py benchmarks/manifest.json --schema benchmarks/schema/manifest.schema.json
python benchmarks/validate_json.py benchmarks/reports/latest.json --schema benchmarks/schema/report.schema.json
```

Use `--samples N` and `--warmup N` to control measurement. `--binary PATH`
selects another executable, and repeated `--case BENCH-0N` runs a subset while
marking the remaining cases `not-run`. A missing executable is reported as
`blocked`, not as a false pass.

## Correctness contracts

- BENCH-01 requires the exact nested JSON result and exit code 0.
- BENCH-02 requires the typed numeric transformation to agree within `1e-6`.
- BENCH-03 requires exact shape and F32 values within `2e-4`.
- BENCH-04 requires loss reduction, checkpoint-reload agreement and model
  metrics within `2e-3`.
- BENCH-05 requires loss reduction, held-out metrics and the declared greedy
  generation to agree with the stdlib oracle.
- BENCH-06 requires a nonzero exit, empty stdout and stable `SEL-R210` domain
  diagnostics, so an error cannot masquerade as a successful result.

The MLP and language-model cases are real source-owned Seloryn workloads. Each
sample runs from a fresh temporary project copy because those workloads write a
checkpoint. The remaining cases are read-only or standalone fixtures.

## Report interpretation

The `comparison.direction` field records a case-local timing difference as a
`gain`, `loss` or `tie` relative to the independent reference. The reference
is an oracle rather than a production competitor, and process startup,
serialization and runtime boundaries are included in the measured wall time.
Use the report to investigate individual workloads; do not infer a general
performance ranking from it.
