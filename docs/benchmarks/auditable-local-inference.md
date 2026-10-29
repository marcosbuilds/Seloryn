# Auditable local inference proof

This is the first product-shaped proof of Seloryn's proposed niche: a local AI
workflow whose data, model, checkpoint and runtime contracts are explicit and
machine-checked before a result is accepted. Its four-feature
equipment-health data is a small repository-authored fixture inspired by that
domain, not field data, public-dataset evidence or production telemetry.

The fixture is deliberately small so its behavior is inspectable. It contains
a synthetic domain-inspired binary equipment-health dataset, a
four-input/four-hidden-unit ReLU
MLP, mean-squared error, source-owned reverse-mode updates, holdout evaluation,
JSON checkpoint persistence, reload and a separate inference request. The
network and training loop are written in `.sely`; the runtime supplies tensor
and autodiff primitives, not a predefined trainer.

The file-level provenance, hashes, split, shape and unsupported claims are
listed in the [public evidence catalog](evidence-catalog.json). Reproducing
the probe verifies this authored fixture and its independent numerical
reference; it does not turn the fixture into production evidence.

## Reproduction gate

```powershell
cargo build --release --locked
python tests/auditable_local_inference_probe.py
```

The probe uses a temporary copy of the project and checks all of the following:

| Gate | Required fact | Result from the first proof run |
|---|---|---|
| Static validation | The project passes `seloryn check` | exit `0` |
| Source ownership | No `train_mlp`, `predict_mlp`, `train_linear_gpu` or `load_checkpoint` appears in `.sely` source | pass |
| Training | Loss decreases for 320 source-owned SGD steps | `0.12042054 → 0.04675742` |
| Evaluation | Reloaded checkpoint reaches the holdout target | loss `0.03375852`, accuracy `1.0` |
| Persistence | Checkpoint contains 25 weights and a SHA-256 payload hash | pass; `f164a947…776fc` |
| Inference | A second request loads the checkpoint and returns a decision | `review`, score `1.15945053` |
| Streaming | Training and inference receipts verify independently | `25` and `3` events; both valid |
| Source-owned replay | A second isolated run receives the recorded train request and matches identity, result, checkpoint and semantic events | pass; dataset tamper rejected |
| Reusable source API | The MLP and language-model projects share `app.parameters` plus `Sgd`, `sgd(Tensor[F32])`, `sgd_rate(F64)` and `step` contracts | pass |
| Performance gate | Five cold process runs against the same fixture and independent Python oracle | `0.182768 s → 0.157362 s` median after contiguous-access fast path; `13.9%` lower |
| Performance artifacts | Baseline/current release binaries are identified by SHA-256 | `8dd3e37a…b37ce11` → `8ad4ff7b…6fcc1834` |
| Audit event | Inference emits structured model/decision/checkpoint evidence | one `audit.record` event |
| Negative boundaries | Wrong feature order/count, dataset shape/target, model contract and checkpoint tampering fail | all six exit `65` |

The exact wall time is hardware- and load-dependent, so it is not a product
claim. The reproducible benchmark is `python tests/performance_probe.py`; the
reported five-run comparison used the pre-fast-path release binary
`8dd3e37af79d6f3fb47e594ea69f381c4c2c13df2558c8c2e93e01373b37ce11` as the
baseline, the current release binary
`8ad4ff7b315d585887b83bb6254196930d94e8a036db04bf7f96d2606fcc1834`, and the
same temporary fixture for every run. Median/P95 were `0.182768/0.188539 s`
for the baseline and `0.157362/0.165186 s` for the current binary. The
pure-Python oracle measured
`0.022318 s` median and is a numerical reference, not a speed claim. Both
implementations still produce the same loss and accuracy.

## Independent numerical comparison

The reference in
[`examples/projects/auditable-local-inference/baseline/reference.py`](../../examples/projects/auditable-local-inference/baseline/reference.py)
uses only Python's standard library. It reimplements the same matrix
operations, ReLU, MSE derivatives and SGD equations without importing Seloryn
or calling a training helper.

| Backend | Initial loss | Final loss | Test loss | Accuracy | Interpretation |
|---|---:|---:|---:|---:|---|
| Seloryn `.sely` | 0.12042054 | 0.04675742 | 0.03375852 | 1.000000 | executable source-owned workflow |
| Python pure reference | 0.12042056 | 0.04675742 | 0.03375852 | 1.000000 | independent correctness oracle |

The probe accepts a loss difference below `1e-4` and exact accuracy equality.
The small initial-loss difference is expected from Seloryn's `F32` tensors;
the final and holdout values agree within the gate.

## Workflow boundary

```mermaid
flowchart LR
    D[Owned JSON data] --> DC[Dataset contract]
    M[Model contract] --> C[Seloryn contract loader]
    R[Run contract] --> C
    D --> C
    C --> T[.sely training loop]
    T --> CK[Hash-bound JSON checkpoint]
    CK --> I[.sely inference]
    I --> A[Structured audit event]
    T --> Q[Hash-chained JSON2 receipt]
    I --> Q
```

This is a verified preview, not a stable ABI or a general model ecosystem. It
does not claim transformer training, GPU tensor training, distributed runs,
HTTP serving, package publication or production-ready runtime status.
