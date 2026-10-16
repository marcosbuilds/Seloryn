# Current capability status

Seloryn `0.09` is a verified-preview language/runtime for a bounded `.sely`
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
| Human/JSON/JSON2 terminal streaming with flush, identity and stdout/stderr separation | **Verified preview for bounded CPU/GPU-linear training; JSON2 chain and replay independently verified** | [`src/main.rs`](../../src/main.rs), [`tests/streaming_v2_probe.py`](../../tests/streaming_v2_probe.py), [`verify-event-chain.py`](../../scripts/streaming/verify-event-chain.py), [`replay-receipt.py`](../../scripts/streaming/replay-receipt.py) |
| Seloryn-authored tensor objective, multi-input gradients, bounded `range` and SGD loop | **Verified preview** | [`examples/projects/self-hosted-linear`](../../examples/projects/self-hosted-linear), [`tests/self_hosted_probe.py`](../../tests/self_hosted_probe.py) |
| Physical hardware adapter inventory and bounded WGSL compute/readback probe | **Verified preview on the declared host; capability-aware elsewhere** | [`src/gpu.rs`](../../src/gpu.rs), [`tests/gpu_probe.py`](../../tests/gpu_probe.py) |
| Explicit `train_linear_gpu` scalar F32 profile with device-resident parameters and one final readback | **Verified preview on the declared host; capability-aware elsewhere** | [`src/gpu.rs`](../../src/gpu.rs), [`examples/projects/gpu-linear-training`](../../examples/projects/gpu-linear-training), [`tests/gpu_training_probe.py`](../../tests/gpu_training_probe.py) |
| JSON2 program/manifest/dataset/checkpoint/result identity, replay helper, structured path redaction and optional HMAC authentication | **Verified preview for bounded training receipts** | [`scripts/streaming/replay-receipt.py`](../../scripts/streaming/replay-receipt.py), [`tests/streaming_v2_probe.py`](../../tests/streaming_v2_probe.py), [`verify-event-chain.py`](../../scripts/streaming/verify-event-chain.py) |
| Bounded JSON number arrays converted to typed `List[I64]` / `List[F64]` | **Verified language gain** | [`examples/projects/json-array-pipeline`](../../examples/projects/json-array-pipeline), [`tests/json_array_probe.py`](../../tests/json_array_probe.py) |
| Hash-validated bounded CPU MLP checkpoint batch inference | **Verified language gain** | [`examples/projects/mlp-inference`](../../examples/projects/mlp-inference), [`tests/mlp_inference_probe.py`](../../tests/mlp_inference_probe.py); no general model loader or serving |
| Read-only `doctor` project diagnostics and transactional `init` project templates | **Verified runtime patch** | [`src/workspace.rs`](../../src/workspace.rs), [`tests/workspace_probe.py`](../../tests/workspace_probe.py); reports `production_ready=false` while ABI remains unstable |
| VS Code `.sely` highlighting, parser-backed LSP diagnostics/completion/definition, CLI fallback and canonical Seloryn file icon | **Verified editor preview on the `0.09` language line** | [`tooling/vscode`](../../tooling/vscode); headless transport/controller tests and GitHub release publication pending |
| `seloryn lsp --stdio` full-document diagnostics, completion, definition and document symbols | **Initial LSP v1 verified locally and wired to VS Code** | [`src/lsp.rs`](../../src/lsp.rs), [`tooling/vscode/lsp-client.js`](../../tooling/vscode/lsp-client.js), [`tests/lsp_probe.py`](../../tests/lsp_probe.py); indexed analysis and incremental parsing remain outside this gate |
| Local modules, bounded JSON capabilities and deterministic bundles | Experimental implemented and tested | [`src/project.rs`](../../src/project.rs), [`tests/release_probe.py`](../../tests/release_probe.py) |
| Strict `seloryn.toml` parsing and runtime-language compatibility rejection | **Verified preview** | [`src/project.rs`](../../src/project.rs), [`tests/project_manifest_probe.py`](../../tests/project_manifest_probe.py), CLI and VS Code integration fixtures |

## Explicit boundaries

| Capability | State | Consequence |
|---|---|---|
| Public `Module`/`Parameter`/`Dataset` APIs and optimizer families | Not implemented | Seloryn can now own a bounded optimization loop, but the reusable object API is still not exposed |
| GPU tensor/autodiff training, GPU MLP, distributed execution, mixed precision and memory sharding | Not implemented | `train_linear_gpu` is a closed scalar F32 profile; it does not make `Tensor` or MLP device-resident |
| Transformer/LLM training, tokenizers and serving | Not implemented | no model-scale or production-serving claim |
| Runtime installer, registry, broad package ecosystem and stable ABI | Not implemented | use a built binary or the source checkout; the preview is not a stable distribution |
| Indexed/incremental editor analysis and interactive VS Code training panel | Not implemented | the extension now uses the bounded full-document LSP by default, retains the saved-file CLI fallback, and training still runs through CLI/terminal |
| Public-key signatures, asynchronous backpressure protocol and general execution telemetry | Not implemented | JSON2 now supports deterministic identity/replay for bounded training and optional keyed HMAC authentication; it is not a public Ed25519 signature or async queue |

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
The public comparison remains honest: Seloryn's current differentiator is a
single typed, bounded and observable workflow with an independently verifiable
training receipt. Broad productivity, security-authenticity or performance
superiority over established ML runtimes has not been proved.
The `0.08` gain removes one concrete data-ingestion blocker by converting
bounded JSON numeric arrays into typed lists that ordinary Seloryn loops can
consume; it does not create a general dataframe, package registry or model
loading API.
The `0.09` gain adds a bounded CPU MLP batch-inference path that validates the
checkpoint's model weights/state hashes and input dimensions before producing
deterministic predictions. It does not create a general model format, serving
runtime, registry or GPU inference backend.
The runtime-only `0.9.1` patch adds read-only project diagnosis and
no-overwrite transactional initialization. These commands improve adoption and
failure visibility but deliberately do not convert the verified-preview channel
or unstable ABI into a production-ready claim.
