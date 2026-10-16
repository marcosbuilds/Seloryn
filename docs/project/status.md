# Current capability status

Seloryn `0.06` is a verified-preview language/runtime for a bounded `.sely`
subset. It includes an executable CPU MLP training profile, hardened resume
integrity and a real physical non-fallback GPU compute probe in addition to the earlier
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
| Human/JSON/JSON2 terminal streaming with flush and stdout/stderr separation | **Verified preview for CPU training; JSON2 chain independently verified** | [`src/main.rs`](../../src/main.rs), [`tests/streaming_v2_probe.py`](../../tests/streaming_v2_probe.py), [`verify-event-chain.py`](../../scripts/streaming/verify-event-chain.py) |
| Seloryn-authored tensor objective, multi-input gradients, bounded `range` and SGD loop | **Verified preview** | [`examples/projects/self-hosted-linear`](../../examples/projects/self-hosted-linear), [`tests/self_hosted_probe.py`](../../tests/self_hosted_probe.py) |
| Physical hardware adapter inventory and bounded WGSL compute/readback probe | **Verified preview on the declared host; capability-aware elsewhere** | [`src/gpu.rs`](../../src/gpu.rs), [`tests/gpu_probe.py`](../../tests/gpu_probe.py) |
| VS Code `.sely` highlighting, real snippets/completions, CLI diagnostics and Seloryn file icon | Verified editor preview | [`tooling/vscode`](../../tooling/vscode), [public 0.4.0 VSIX release](https://github.com/marcosbuilds/Seloryn/releases/tag/vscode-v0.4.0) |
| Local modules, bounded JSON capabilities and deterministic bundles | Experimental implemented and tested | [`src/project.rs`](../../src/project.rs), [`tests/release_probe.py`](../../tests/release_probe.py) |
| Strict `seloryn.toml` parsing and runtime-language compatibility rejection | **Verified preview** | [`src/project.rs`](../../src/project.rs), [`tests/project_manifest_probe.py`](../../tests/project_manifest_probe.py), CLI and VS Code integration fixtures |

## Explicit boundaries

| Capability | State | Consequence |
|---|---|---|
| Public `Module`/`Parameter`/`Dataset` APIs and optimizer families | Not implemented | Seloryn can now own a bounded optimization loop, but the reusable object API is still not exposed |
| GPU tensor/autodiff training, distributed execution, mixed precision and memory sharding | Not implemented | the GPU gate proves adapter selection and one compute/readback path, not model training |
| Transformer/LLM training, tokenizers and serving | Not implemented | no model-scale or production-serving claim |
| Runtime installer, registry, broad package ecosystem and stable ABI | Not implemented | use a built binary or the source checkout; the preview is not a stable distribution |
| Full LSP and interactive VS Code training panel | Not implemented | the extension uses the CLI as source of truth; training is run through CLI/terminal |
| Replay, signatures, redaction, backpressure protocol and general execution telemetry | Not implemented | JSON2 currently proves an integrity chain for the bounded CPU training profiles only |

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
training profiles. It does not claim replay, signatures, backpressure
independence or general execution telemetry.
The public comparison remains honest: Seloryn's current differentiator is a
single typed, bounded and observable workflow with an independently verifiable
training receipt. Broad productivity, security-authenticity or performance
superiority over established ML runtimes has not been proved.
