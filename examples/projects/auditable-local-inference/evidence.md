# Evidence record

The first black-box probe was run against the local release binary on Windows
with language `0.12` and runtime package `0.12.0`.

| Evidence | Observed result |
|---|---|
| Project check | exit `0` |
| Source-owned training | 320 `.sely` SGD steps; loss `0.12042054 → 0.04675742` |
| Holdout evaluation | loss `0.03375852`; accuracy `1.0` |
| Reload | reloaded loss equals holdout loss within `1e-6` |
| Checkpoint | 25 weights; SHA-256 `f164a947bf1686f3e0028e39aa722e16a7eea16034d73d4011f5b354bce776fc` |
| Training receipt | 23 events, 21 progress events, independent chain verification passed |
| Inference receipt | 3 events, one `audit.record`, independent chain verification passed |
| Negative cases | wrong order/count, dataset shape/target, model contract and checkpoint tampering all exited `65` |
| Independent reference | pure Python final/test losses agree within `1e-4`; accuracy exactly `1.0` |

The authoritative reproducibility command is
`python tests/auditable_local_inference_probe.py`. Timings are intentionally
not stored as a performance claim; the first run measured `0.2596 s` for
Seloryn and `0.0225 s` for the pure-Python reference on this small fixture.
