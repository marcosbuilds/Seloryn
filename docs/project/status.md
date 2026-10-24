# Current capability status

Seloryn `0.12` is a verified-preview language/runtime for a bounded `.sely`
subset. It includes an executable CPU MLP training profile, hardened resume
integrity, a real physical non-fallback GPU compute probe and a closed GPU
linear-training profile in addition to the earlier
scalar, project, tensor and autodiff gates. The status applies to the observed
contracts below; it does not turn the whole runtime into a stable general-
purpose ML platform.

## What works today

| Area | State | Evidence |
|---|---|---|
| UTF-8 lexer, layout, parser, names and partial type checker | Experimental implemented and tested | [`src/parser.rs`](../../src/parser.rs), [`src/semantic.rs`](../../src/semantic.rs) |
| Bounded scalar/list/project interpreter with structured diagnostics | Experimental implemented and tested | [`src/runtime.rs`](../../src/runtime.rs), CLI tests |
| CPU `Tensor[F32]`/`Tensor[I64]`, broadcasting, reductions, matmul and stable numeric ops | Experimental implemented and tested | [`src/tensor`](../../src/tensor), [`tests/tensor_cpu.rs`](../../tests/tensor_cpu.rs) |
| Bounded Tensor[F32] reverse-mode autodiff | Experimental implemented and tested | [`src/tensor_autograd.rs`](../../src/tensor_autograd.rs), [`tests/n3_tensor_probe.py`](../../tests/n3_tensor_probe.py) |
| Two-layer CPU MLP training with mini-batches, ReLU backpropagation, SGD, evaluation and deterministic seeds | **Verified preview** | [`training.md`](../compute/training.md), [`examples/projects/mlp-training`](../../examples/projects/mlp-training), [`tests/mlp_training_probe.py`](../../tests/mlp_training_probe.py) |
| Versioned checkpoint save/resume with dataset, weight, sampler-state and complete-state hashes | **Verified preview** | [`src/runtime.rs`](../../src/runtime.rs), MLP black-box probe |
| Human/JSON/JSON2 terminal streaming with flush, identity and stdout/stderr separation | **Verified preview for bounded CPU/GPU-linear and source-owned CPU training; JSON2 chain and replay independently verified** | [`src/main.rs`](../../src/main.rs), [`tests/streaming_v2_probe.py`](../../tests/streaming_v2_probe.py), [`tests/auditable_local_inference_probe.py`](../../tests/auditable_local_inference_probe.py), [`replay-receipt.py`](../../scripts/streaming/replay-receipt.py) |
| Seloryn-authored tensor objective, multi-input gradients, bounded `range` and SGD loop | **Verified preview** | [`examples/projects/self-hosted-linear`](../../examples/projects/self-hosted-linear), [`tests/self_hosted_probe.py`](../../tests/self_hosted_probe.py) |
| Custom `.sely` ReLU MLP with own dataset, training, holdout evaluation, atomic checkpoint, reload and inference | **Verified preview** | [`examples/projects/self-hosted-mlp`](../../examples/projects/self-hosted-mlp), [`tests/self_hosted_mlp_probe.py`](../../tests/self_hosted_mlp_probe.py); `train_mlp` is not used |
| Complete source-owned `.sely` bigram language model with runtime-loaded vocabulary/datasets, reusable parameters/SGD, training, held-out evaluation, hash-validated checkpoint reload and separate inference | **Verified preview** | [`examples/projects/self-hosted-language-model`](../../examples/projects/self-hosted-language-model), [`tests/self_hosted_language_model_probe.py`](../../tests/self_hosted_language_model_probe.py), [`docs/compute/language-model.md`](../compute/language-model.md); CPU-only and one-token context |
| Source-owned auditable local-inference workflow with explicit dataset/model/run contracts, hash-bound checkpoint persistence, separate inference and structured audit event | **Verified preview** | [`examples/projects/auditable-local-inference`](../../examples/projects/auditable-local-inference), [`tests/auditable_local_inference_probe.py`](../../tests/auditable_local_inference_probe.py), [`docs/benchmarks/auditable-local-inference.md`](../benchmarks/auditable-local-inference.md); bounded CPU fixture |
| Physical hardware adapter inventory and bounded WGSL compute/readback probe | **Verified preview on the declared host; capability-aware elsewhere** | [`src/gpu.rs`](../../src/gpu.rs), [`tests/gpu_probe.py`](../../tests/gpu_probe.py) |
| Explicit `train_linear_gpu` scalar F32 profile with device-resident parameters and one final readback | **Verified preview on the declared host; capability-aware elsewhere** | [`src/gpu.rs`](../../src/gpu.rs), [`examples/projects/gpu-linear-training`](../../examples/projects/gpu-linear-training), [`tests/gpu_training_probe.py`](../../tests/gpu_training_probe.py) |
| JSON2 program/manifest/dataset/checkpoint/result identity, source-owned replay, structured path redaction and optional HMAC authentication | **Verified preview for bounded training receipts** | [`scripts/streaming/replay-receipt.py`](../../scripts/streaming/replay-receipt.py), [`tests/streaming_v2_probe.py`](../../tests/streaming_v2_probe.py), [`tests/auditable_local_inference_probe.py`](../../tests/auditable_local_inference_probe.py), [`verify-event-chain.py`](../../scripts/streaming/verify-event-chain.py) |
| Shared source-owned `Parameter`/`Sgd` contract across the MLP and language-model projects | **Verified project-level API slice** | [`examples/projects/auditable-local-inference/src/app/parameters.sely`](../../examples/projects/auditable-local-inference/src/app/parameters.sely), [`examples/projects/auditable-local-inference/src/app/optim.sely`](../../examples/projects/auditable-local-inference/src/app/optim.sely), [`examples/projects/self-hosted-language-model/src/app/optim.sely`](../../examples/projects/self-hosted-language-model/src/app/optim.sely) |
| Same-project source-level contract reuse example for two consumers | **Verified experimental slice** | [`examples/projects/source-contract-reuse`](../../examples/projects/source-contract-reuse), [`tests/source_contract_reuse_probe.py`](../../tests/source_contract_reuse_probe.py); not a package, resolver, registry or ABI |
| Cooperative cancellation for receipt-enabled bounded runs | **Verified bounded preview slice** | [`tests/cancel_probe.py`](../../tests/cancel_probe.py); `SEL-CANCELLED` ends with a valid JSON2 `run.end` and exit `130`; loops without event points, forced termination, cleanup and resume remain outside the proof |
| Local job lifecycle, read-only preflight and reservation lease contract with canonical specs, atomic state snapshots and event-authoritative repair | **Experimental implemented; contract-only** | [`src/job.rs`](../../src/job.rs), [`tests/job_preflight_probe.py`](../../tests/job_preflight_probe.py), lease/lifecycle/recovery/concurrency probes; local preflight/claim/heartbeat/release/recover only, no worker, process fencing, retry or execution |
| Contiguous tensor access benchmark and repeated source-owned training measurement | **Verified local performance reduction** | [`src/tensor/ops.rs`](../../src/tensor/ops.rs), [`tests/performance_probe.py`](../../tests/performance_probe.py); median `0.182768 s → 0.157362 s` on the recorded Windows run |
| Validation of every symbol in a local multi-symbol import | **Verified parser/loader integrity patch** | [`src/project.rs`](../../src/project.rs), [`tests/import_contract_probe.py`](../../tests/import_contract_probe.py); missing export fails with `SEL-M004` |
| Bounded JSON number arrays converted to typed `List[I64]` / `List[F64]` | **Verified language gain** | [`examples/projects/json-array-pipeline`](../../examples/projects/json-array-pipeline), [`tests/json_array_probe.py`](../../tests/json_array_probe.py) |
| Hash-validated bounded CPU MLP checkpoint batch inference | **Verified language gain** | [`examples/projects/mlp-inference`](../../examples/projects/mlp-inference), [`tests/mlp_inference_probe.py`](../../tests/mlp_inference_probe.py); no general model loader or serving |
| Read-only `doctor` project diagnostics and transactional `init` project templates | **Verified runtime patch** | [`src/workspace.rs`](../../src/workspace.rs), [`tests/workspace_probe.py`](../../tests/workspace_probe.py); reports `production_ready=false` while ABI remains unstable |
| Runtime release provenance: source tree, toolchain, lockfile, binary and staged artifact hashes | **Verified packaging gate** | [`scripts/release/package-runtime.py`](../../scripts/release/package-runtime.py), [`scripts/release/prepare-release.py`](../../scripts/release/prepare-release.py), [`tests/release_staging_probe.py`](../../tests/release_staging_probe.py) |
| Versioned `seloryn.bundle.v1` artifacts with build metadata hash verification | **Verified runtime patch** | [`src/project.rs`](../../src/project.rs), [`src/main.rs`](../../src/main.rs), [`tests/release_probe.py`](../../tests/release_probe.py) |
| Four-process operational reliability, deterministic semantic receipts/checkpoints and workspace confinement | **Verified bounded CPU evidence** | [`tests/operational_reliability_probe.py`](../../tests/operational_reliability_probe.py), [`operational-reliability.md`](../benchmarks/operational-reliability.md); forced Windows termination is classified separately from the cooperative-cancellation slice |
| VS Code `.sely` highlighting, parser-backed LSP diagnostics/completion/definition, CLI fallback and canonical Seloryn file icon | **Verified editor preview on the `0.12` language line** | [`tooling/vscode`](../../tooling/vscode); headless transport/controller tests and Marketplace publication pending |
| `seloryn lsp --stdio` full-document diagnostics, completion, definition and document symbols | **Verified bounded LSP v1 lifecycle slice** | [`src/lsp.rs`](../../src/lsp.rs), [`tooling/vscode/lsp-client.js`](../../tooling/vscode/lsp-client.js), [`tests/lsp_probe.py`](../../tests/lsp_probe.py); version ordering, stale-change rejection, `didClose` cleanup and diagnostic clearing are covered; indexed analysis and incremental range parsing remain outside this gate |
| LSP document lifecycle, monotonic versions, stale-change rejection and framed-header validation | **Verified runtime patch** | [`src/lsp.rs`](../../src/lsp.rs), [`tests/lsp_probe.py`](../../tests/lsp_probe.py) |
| Local modules, bounded JSON capabilities and deterministic bundles | Experimental implemented and tested | [`src/project.rs`](../../src/project.rs), [`tests/release_probe.py`](../../tests/release_probe.py) |
| Strict `seloryn.toml` parsing and runtime-language compatibility rejection | **Verified preview** | [`src/project.rs`](../../src/project.rs), [`tests/project_manifest_probe.py`](../../tests/project_manifest_probe.py), CLI and VS Code integration fixtures |

## Explicit boundaries

| Capability | State | Consequence |
|---|---|---|
| Public package-wide `Module`/`Dataset` APIs and optimizer families | Not implemented | Two projects now share a verified source-level `Parameter`/`Sgd` contract; a registry-wide API and optimizer state families are still not exposed |
| GPU tensor/autodiff training, GPU MLP, distributed execution, mixed precision and memory sharding | Not implemented | `train_linear_gpu` is a closed scalar F32 profile; it does not make `Tensor` or MLP device-resident |
| Transformer/LLM-scale training, Unicode/subword tokenizers and HTTP serving | Not implemented | the current language-model proof is a CPU bigram with one-token context and JSON CLI integration |
| Runtime installer, registry, broad package ecosystem and stable ABI | Experimental implemented | the Windows installer can verify and install a local binary with optional user PATH integration; the preview is not a stable ABI |
| Indexed/incremental editor analysis and interactive VS Code training panel | Not implemented | the extension now uses the bounded full-document LSP by default, retains the saved-file CLI fallback, and training still runs through CLI/terminal |
| Public-key signatures, asynchronous backpressure protocol and general execution telemetry | Not implemented | JSON2 now supports deterministic identity/replay for bounded training and optional keyed HMAC authentication; it is not a public Ed25519 signature or async queue |
| Job worker/supervisor, process fencing, crash recovery, retries and remote execution | Not implemented | The local store has an expiring reservation lease, but it does not spawn a process, prove worker liveness, fence stale owners, retry lost jobs or recover a live process |

## Evidence rule

The language `0.02` baseline earned its evidence from the MLP profile's measured
holdout loss, resumable checkpoint and live event stream. The self-hosted
optimization profile earns `0.03` because an external user can run a separate
`.sely` project and observe a nontrivial learned model from its own objective,
multi-input gradients, bounded loop and parameter updates. The gain does not
imply that GPU, distributed or general training promises are fulfilled; it
demonstrates that model/loss/update orchestration can now be written in Seloryn
rather than only selected through a native trainer.
The `0.04` gain adds a non-degenerate seed path, a total sample-update budget,
tamper-detecting sampler state for resumable MLP training, and a real physical
non-fallback adapter inventory plus an 8-element WGSL compute/readback proof on
the declared host. It does not claim GPU tensor training or universal backend
availability.
The `0.05` gain makes project manifests use a real TOML parser, rejects
malformed manifests and rejects language declarations newer than the runtime;
the public CLI and editor integration fixtures exercise both failures before
source execution.
The `0.06` gain adds hash-chained JSON2 training receipts, independent
tamper/duplicate-key verification, early event-sink cancellation, checkpoint
temporary-file cleanup and enforcement of manifest JSON limits in the CPU
training profiles.
The `0.07` gain adds program/manifest/dataset/checkpoint/result identity,
successful-run replay in an isolated copy when the recorded initial checkpoint
is available, structured path redaction in JSON2, optional HMAC-SHA256 receipt
authentication and a closed GPU linear trainer whose weights remain in a WGSL
storage buffer until one final readback. It does not claim public-key signing,
asynchronous backpressure, GPU tensor/autodiff/MLP training or distributed
execution.
The public comparison remains honest: Seloryn now has a typed, bounded and
observable workflow whose model and optimization policy can be authored in
`.sely`, with an independently verifiable training receipt. Broad productivity,
security-authenticity or performance superiority over established ML runtimes
has not been proved.
The `0.08` gain removes one concrete data-ingestion blocker by converting
bounded JSON numeric arrays into typed lists that ordinary Seloryn loops can
consume; it does not create a general dataframe, package registry or model
loading API.
The `0.09` gain adds a bounded CPU MLP batch-inference path that validates the
checkpoint's model weights/state hashes and input dimensions before producing
deterministic predictions. It does not create a general model format, serving
runtime, registry or GPU inference backend.
The `0.10` custom `.sely` MLP gain adds program-owned JSON construction/persistence,
tensor value export, flushed source-owned progress events and a black-box
workflow that defines the network, objective, gradient updates, holdout
evaluation, checkpoint reload and inference without `train_mlp`. It does not
create a public `Module`/`Parameter` framework, GPU tensor training or a general
checkpoint registry.
The `0.11` language-model gain added the first complete source-owned next-token baseline:
modular imports, runtime-sized textual dataset ingestion, tokenizer, fused
softmax cross-entropy, reverse-mode gradient updates, named parameters,
reusable `.sely` SGD, held-out evaluation, checkpoint hash validation, reload
and a second-process inference request. Its independent comparison matches a
stdlib reference within `1e-5` for loss and exactly for accuracy. It does not
claim a transformer, subword tokenizer, GPU training, HTTP serving or stable ABI.
The `0.12` gain generalizes that workflow to runtime-sized vocabularies and
datasets, reusable source-level parameters and SGD, fused cross-entropy with
finite-difference coverage, and a repeated benchmark plus installed-terminal
proof. It still does not claim a transformer, context larger than one token,
GPU tensor training, HTTP serving or stable ABI.
The first auditable local-inference proof adds a separate source-owned ReLU MLP
workflow with explicit dataset/model/run contracts, hash-bound checkpoint
metadata, a second-process inference request, structured audit events inside
the JSON2 receipt and six negative boundary cases. It proves the contract and
receipt shape for this fixture; it does not claim general model interchange,
serving, performance superiority or production-ready runtime status.
The next continuation adds a contiguous tensor-access fast path with a
five-run before/after benchmark, validates every symbol in local multi-symbol
imports, and gives both source-owned training projects the same `Parameter`/
`Sgd` contract. Their training receipts now carry source-owned dataset/request
contracts and checkpoint completion evidence; the replay helper re-executes
those workflows in an isolated copy and rejects a changed dataset. The
measured reduction is specific to the recorded Windows fixture and does not
claim general performance superiority. Public package-wide namespaces,
optimizer families and filesystem-wide output snapshots remain future work.
The runtime-only `0.9.1` patch adds read-only project diagnosis and
no-overwrite transactional initialization. These commands improve adoption and
failure visibility but deliberately do not convert the verified-preview channel
or unstable ABI into a production-ready claim.
The runtime-only `0.9.2` patch adds a checked provenance envelope for staged
artifacts. It improves traceability and tamper detection but is not a signature,
stable ABI or public release.
The runtime-only `0.9.3` patch makes the Windows installer verify that supplied
provenance describes the exact binary and compatibility line before installation.
It closes an installation-integrity gap but is not a signature, stable ABI or
public release.
The runtime-only `0.9.4` patch versions the `.selyapp` wire format, verifies
build-directory metadata before execution and hardens the LSP document
lifecycle. It improves compatibility and maintenance safety but is not a stable
runtime ABI or public release.
