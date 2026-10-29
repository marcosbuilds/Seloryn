# Evidence record

The first black-box probe was run against the local release binary on Windows
with language `0.12` and runtime package `0.12.0`. The train/test JSON files
are repository-authored, domain-inspired synthetic fixtures; they are not
field measurements, a public dataset or production telemetry. Their file
hashes and evidence boundary are recorded in
[`docs/benchmarks/evidence-catalog.json`](../../../docs/benchmarks/evidence-catalog.json).

| Evidence | Observed result |
|---|---|
| Project check | exit `0` |
| Source-owned training | 320 `.sely` SGD steps; loss `0.12042054 → 0.04675742` |
| Holdout evaluation | loss `0.03375852`; accuracy `1.0` |
| Reload | reloaded loss equals holdout loss within `1e-6` |
| Checkpoint | 25 weights; SHA-256 `f164a947bf1686f3e0028e39aa722e16a7eea16034d73d4011f5b354bce776fc` |
| Training receipt | 25 events, 21 progress events plus source-owned contract/completion events; independent chain verification passed |
| Inference receipt | 3 events, one `audit.record`, independent chain verification passed |
| Source-owned replay | Isolated re-execution matched program/dataset/result/checkpoint identity and semantic events; tampered dataset was rejected |
| Shared optimizer contract | `app.parameters` and `app.optim` use the same `Parameter`, `replace`, `Sgd`, `sgd(Tensor[F32])`, `sgd_rate(F64)` and `step` contract as the language-model project |
| Negative cases | wrong order/count, dataset shape/target, model contract and checkpoint tampering all exited `65` |
| Independent reference | pure Python final/test losses agree within `1e-4`; accuracy exactly `1.0` |
| Performance artifacts | baseline `8dd3e37a…b37ce11`; current `8ad4ff7b…6fcc1834`; five-run median `0.182768 s → 0.157362 s` |

The authoritative reproducibility command is
`python tests/auditable_local_inference_probe.py`. The separate
`python tests/performance_probe.py --baseline <pre-fast-path-binary> --runs 5`
gate measured median/P95 `0.182768/0.188539 s` before and
`0.157362/0.165186 s` after the contiguous tensor-access fast path, a `13.9%`
median reduction on this Windows host. The benchmark is evidence for this
fixture and machine, not a universal performance claim.
