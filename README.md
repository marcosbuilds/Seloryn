<div align="center">
  <img src="assets/brand/seloryn-logo.svg" alt="Seloryn" width="320" />
  <p><strong>Readable logic. Real computation.</strong></p>
  <p>A documented original programming language design for reliable AI systems.</p>
</div>

> **Current verified-preview line: language 0.12 / runtime package 0.12.0, with verified Windows x64 runtime, installer, VSIX and experimental binary-font releases.** The language evidence includes CPU training, a complete source-owned `.sely` contextual next-token workflow and an auditable local-inference workflow with explicit data/model/run contracts, hash-bound checkpoints and inference audit events. Transformer training, GPU tensor/MLP training, distributed, registry, stable ABI and stable runtime claims remain outside the evidence. See the [current status](docs/project/status.md), [distribution catalog](packages/catalog.toml), [auditable proof](docs/benchmarks/auditable-local-inference.md), [language-model proof](docs/compute/language-model.md) and [training reference](docs/compute/training.md).

> **No performance claims, compatibility badges or training benchmarks are published without independent measurement.** Release assets carry exact platform, version and SHA-256 boundaries; the executable [public conformance and evidence matrix](docs/project/conformance.md) maps language claims to source paths, positive/negative probes, platform requirements and limits.

The first reviewed comparative result for the intended CPU-first tabular niche
is documented in [the production-niche benchmark](docs/benchmarks/production-niche-v2.md).
The first source-owned contract-and-audit proof is documented in [the auditable
local-inference benchmark](docs/benchmarks/auditable-local-inference.md).

## Public preview artifacts

- [Seloryn runtime 0.12.0 / language 0.12](https://github.com/marcosbuilds/Seloryn/releases/tag/seloryn-v0.12): Windows x64 verified-preview asset; other platforms are not claimed until their assets are published.
- [Seloryn VS Code extension package 0.12.0](https://github.com/marcosbuilds/Seloryn/releases/tag/vscode-v0.12.0): verified-preview VSIX distributed separately from this public language source tree.
- [Seloryn Windows installer 0.12.0](https://github.com/marcosbuilds/Seloryn/releases/tag/installer-v0.12.0) and [Seloryn Font 2.0.0](https://github.com/marcosbuilds/Seloryn/releases/tag/fonts-v2.0.0): unsigned/experimental binary distributions with published checksums.

These are verified previews, not a stable ABI or a claim of GPU tensor/MLP
training, distributed execution, package registry, HTTP runtime or a general
model framework.

## First run from a clone

The shortest verified CLI path is provided by the idempotent quickstart scripts.
They build the release binary, run read-only `doctor` on the tracked JSON
project, check and run the tracked tensor example, then execute the portable
core conformance suite. The scripts use only repository examples and do not
create or overwrite user project files.

On POSIX shells:

```sh
bash scripts/quickstart/first-run-posix.sh
```

On PowerShell 5.1 or 7+:

```powershell
pwsh -NoProfile -File .\scripts\quickstart\first-run-powershell.ps1
```

The observable tensor result is:

```text
Tensor[F32, shape=[2, 2], values=[19.0, 22.0, 43.0, 50.0]]
```

The prerequisite doctor requires the Rust toolchain pinned by
`rust-toolchain.toml` and Python 3.10+ for the conformance tooling. Node.js/npm
are not required for this CLI path. Missing required tools produce an
installation-oriented error before build or source execution. The runtime
remains a verified preview with an unstable ABI; this path does not promote a
stable release.

![Seloryn architecture](assets/diagrams/architecture-flow.png)

## What exists today vs. what is designed

Seloryn is being built as one typed language for reliable AI development, integration and serving. The implemented result is a checker, deterministic core IR, pure interpreter, local modules, bounded JSON/file capabilities, a real small CPU MLP training profile and a reproducible bundle path. General effects, broad model APIs and accelerator layers remain outside the verified preview.

## Mission: AI development and integration

Seloryn's primary purpose is to help a developer build and integrate complete
AI systems with fewer invalid states, fewer adaptations between development and
production, and better diagnostics at model, data and service boundaries. This
is a hypothesis to prove with real comparative experiments, not a performance
claim. If equivalent workloads do not show the proposed reduction in failures,
integration drift or diagnostic cost, the architecture must be reconsidered
instead of gaining features by default.

| Capability | Availability in 0.12 | Evidence or boundary |
|:--|:--|:--|
| UTF-8 lexer, layout and parser subset | Implemented and tested | [`src/parser.rs`](src/parser.rs), parser tests |
| Names, scalar types, records, enums, `Result`, `Option` and `match` checks | Experimental partial checker | [`src/semantic.rs`](src/semantic.rs), semantic tests |
| `seloryn check <file.sely or project>` diagnostics | Implemented in the development CLI | [`src/main.rs`](src/main.rs), [`tests/bootstrap_cli.rs`](tests/bootstrap_cli.rs), [`tests/project_manifest_probe.py`](tests/project_manifest_probe.py) |
| `seloryn inspect <file.sely or project>` compact agent context | Verified bounded inspection slice | [`src/main.rs`](src/main.rs), [`src/analysis.rs`](src/analysis.rs), [`tests/structured_inspection_probe.py`](tests/structured_inspection_probe.py); `seloryn.analysis.v1` preserves symbols/imports/diagnostics and adds bounded redacted `seloryn.context.v1` agent metadata |
| Pure scalar/list interpreter and `seloryn run` | Experimental E4 implementation | [`src/runtime.rs`](src/runtime.rs), runtime and CLI tests; no effects |
| CPU `Tensor[F32]` / `Tensor[I64]` buffers and N1/N2 operations | Experimental N1/N2 implementation | [`src/tensor`](src/tensor), tensor unit and CLI tests; CPU only |
| Typed tensor reads and lossless dtype conversion | Experimental N1 implementation | `tensor_get_f32` / `tensor_get_i64`, `tensor_to_f32` / `tensor_to_i64`, bounds and dtype regressions in [`tests/tensor_cpu.rs`](tests/tensor_cpu.rs) |
| Deterministic core IR and execution boundary | Experimental implemented | `src/ir.rs`, runtime parity tests; bounded subset only |
| Local modules and strict project manifests | Experimental implemented | Public project fixtures, real TOML parsing and future-language rejection; no registry or external package resolver |
| VS Code syntax highlighting, parser-backed LSP diagnostics/completion/definition, CLI fallback and optional `.sely` file icons | Verified editor preview release asset | [`docs/guide/editor.md`](docs/guide/editor.md), [`packages/catalog.toml`](packages/catalog.toml); the VSIX builder is outside this public source tree |
| `seloryn lsp --stdio` diagnostics, completion, definition and document symbols | Initial LSP v1 verified locally | [`src/lsp.rs`](src/lsp.rs), [`tests/lsp_probe.py`](tests/lsp_probe.py); indexed analysis, incremental parsing and rename remain future |
| JSON read/parse/encode/get and bounded file capability | Experimental implemented | JSON project fixtures; paths remain root-confined |
| Typed JSON number-array ingestion into `List[I64]` / `List[F64]` | **Verified language gain** | [`examples/projects/json-array-pipeline`](examples/projects/json-array-pipeline), [`tests/json_array_probe.py`](tests/json_array_probe.py); bounded by runtime list limits |
| Hash-validated bounded CPU MLP checkpoint batch inference | **Verified language gain** | [`examples/projects/mlp-inference`](examples/projects/mlp-inference), [`tests/mlp_inference_probe.py`](tests/mlp_inference_probe.py); CPU-only, bounded inputs, no serving ABI |
| Scalar and Tensor[F32] reverse-mode tapes and finite-difference gradient check | Experimental implemented, narrow | [`examples/autograd`](examples/autograd), [`tests/n3_tensor_probe.py`](tests/n3_tensor_probe.py) |
| Bounded CPU MLP training, mini-batches, evaluation, deterministic seed and checkpoint/resume | Verified preview | [`examples/projects/mlp-training`](examples/projects/mlp-training), [`tests/mlp_training_probe.py`](tests/mlp_training_probe.py); CPU/single-process only |
| Opt-in human/JSON/JSON2 training event stream | Verified preview for CPU and bounded GPU-linear training | `seloryn run ... --log=human|json|json2`; JSON2 carries program/manifest/dataset/checkpoint/result identity, bounded source/manifest/static preflight error receipts, structured path redaction, replay verification and flushed stderr; [`verify-event-chain.py`](scripts/streaming/verify-event-chain.py), [`replay-receipt.py`](scripts/streaming/replay-receipt.py) and [`streaming_v2_probe.py`](tests/streaming_v2_probe.py) |
| Seloryn-authored multi-input gradients, bounded `range` and SGD loop | Verified preview | [`examples/projects/self-hosted-linear`](examples/projects/self-hosted-linear), [`tests/self_hosted_probe.py`](tests/self_hosted_probe.py); model/objective/update orchestration is `.sely` source |
| Custom `.sely` ReLU MLP with own dataset, training, holdout evaluation, atomic JSON checkpoint, reload and inference | **Verified preview** | [`examples/projects/self-hosted-mlp`](examples/projects/self-hosted-mlp), [`tests/self_hosted_mlp_probe.py`](tests/self_hosted_mlp_probe.py); no `train_mlp` call and real JSON2 progress stream |
| Complete source-owned `.sely` contextual next-token model with whitespace tokenizer, runtime-sized vocabulary/dataset, configurable context windows, reusable parameters/SGD, perplexity, held-out evaluation, hash-bound checkpoint reload and text inference | **Verified preview** | [`examples/projects/self-hosted-language-model`](examples/projects/self-hosted-language-model), [`tests/self_hosted_language_model_probe.py`](tests/self_hosted_language_model_probe.py), [`docs/compute/language-model.md`](docs/compute/language-model.md); CPU-only, linear one-hot model |
| Source-owned auditable local-inference workflow with explicit data/model/run contracts, hash-bound checkpoint, separate inference and structured audit event | **Verified preview** | [`examples/projects/auditable-local-inference`](examples/projects/auditable-local-inference), [`tests/auditable_local_inference_probe.py`](tests/auditable_local_inference_probe.py), [`docs/benchmarks/auditable-local-inference.md`](docs/benchmarks/auditable-local-inference.md); bounded CPU fixture, no stable ABI |
| Local `job` spec/state/event contract with read-only preflight, leases and bounded worker | Experimental implemented; direct-process execution slice | [`src/job.rs`](src/job.rs), [`src/job_worker.rs`](src/job_worker.rs), [`tests/job_worker_probe.py`](tests/job_worker_probe.py); `job run` snapshots only the validated Seloryn source, uses lease fencing, timeout, cancellation, bounded output hashes and a committed atomic result receipt; no arbitrary commands, descendants or remote workers |
| Physical hardware inventory, non-fallback GPU compute/readback probe and explicit GPU linear training | Verified preview on the declared host; capability-aware elsewhere | [`src/gpu.rs`](src/gpu.rs), [`tests/gpu_probe.py`](tests/gpu_probe.py), [`tests/gpu_training_probe.py`](tests/gpu_training_probe.py); the GPU trainer is scalar F32 linear only, device-resident parameters, one final readback, no silent CPU fallback |
| `build`/`exec` `.selyapp` bundle path | Experimental | Deterministic bundle and clean terminal fixtures; not an installer |
| Transformer/LLM-scale training, embeddings, subword tokenizers, HTTP serving, general GPU tensor/MLP training, packages and distributed execution | Not implemented or verified | The language-model project is a bounded CPU contextual linear proof, and `train_linear_gpu` is a closed scalar profile, not a general backend |
| Native Windows setup and installer | Verified-preview release asset; unsigned | [`packages/catalog.toml`](packages/catalog.toml), [`docs/guide/install.md`](docs/guide/install.md); clean-machine support and signing are not claimed |

| AI development | AI integration | AI serving | Evidence |
|:--|:--|:--|:--|
| Typed data, model and workflow boundaries | Local models and external providers through explicit libraries | Checked request/model/response contracts | Diagnostics and negative tests |
| Result-based failures | Safe transport, serialization and compatibility boundaries | Resource, cancellation and deployment policy | Reproducible comparative experiments |
| Future tensor and autodiff APIs | Future model interchange and adapters | Future training and inference runtimes | Review the architecture when evidence fails |

## A glimpse of the intended language

```seloryn
record Sample:
    input: Tensor[F32]
    target: Tensor[I64]

fn accuracy(predicted: Tensor[I64], expected: Tensor[I64]) -> Result[I64, Text]:
    return Ok(tensor_elements(predicted))
```

The function above is intentionally illustrative beyond the current callable API. The verified runnable tensor examples are [`examples/tensors/tensor-matmul.sely`](examples/tensors/tensor-matmul.sely), [`examples/tensors/tensor-index.sely`](examples/tensors/tensor-index.sely) and [`examples/tensors/tensor-numeric.sely`](examples/tensors/tensor-numeric.sely); they return real CPU results through `seloryn run`. Project examples for local modules, JSON and the narrow linear-training fixture live under [`examples/projects`](examples/projects/README.md).

## Read next

[Getting started](docs/guide/start.md) · [Editor support](docs/guide/editor.md) · [Conformance matrix](docs/project/conformance.md) · [Evidence Lab](docs/project/evidence-lab.md) · [Architecture](docs/project/architecture.md) · [Grammar](docs/reference/grammar.md) · [Types](docs/reference/types.md) · [Errors](docs/reference/errors.md) · [Tensors](docs/compute/tensors.md) · [Training](docs/compute/training.md) · [Executable language model](docs/compute/language-model.md) · [LLM boundary](docs/compute/llms.md) · [Servers](docs/systems/http.md) · [Platform support](docs/guide/platforms.md) · [Releases](docs/project/versions.md) · [License](docs/project/licensing.md)

## References and visuals

Original Seloryn symbol and architecture diagrams are packaged under `assets/`. Vendor and service brand SVGs may be retrieved via the [theSVG catalog](https://thesvg.org/) when needed; they must not be presented as Seloryn's own mark. No external brand assets are bundled in this repository.

**Documentation map:** [Browse by category](docs/index.md).
