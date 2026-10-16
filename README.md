<div align="center">
  <img src="assets/seloryn-symbol.svg" alt="Seloryn symbol" width="88" height="88" />
  <h1>Seloryn</h1>
  <p><strong>Readable logic. Real computation.</strong></p>
  <p>A documented original programming language design for reliable AI systems.</p>
</div>

> **Current status: language 0.07 / runtime package 0.7.0, verified-preview CPU training plus identity-bearing replay receipts and an explicit device-resident GPU linear-training profile.** The release also retains hardened checkpoints, strict project manifests, Seloryn-authored tensor optimization, bounded tensors, autodiff, projects and deterministic bundles. GPU tensor/MLP training, distributed, registry, installer and stable runtime claims remain outside the evidence. See the [current status](docs/project/status.md) and [training reference](docs/compute/training.md).

> **No performance claims, compatibility badges or training benchmarks are published without independent measurement.** Public preview artifacts are now available, but they carry the exact platform, version and boundary claims documented below.

## Public preview artifacts

- [Seloryn runtime 0.7.0 / language 0.07](https://github.com/marcosbuilds/Seloryn/releases/tag/seloryn-v0.07): Windows x64, Linux x64 and macOS arm64 archives with SHA-256 checksums.
- [Seloryn VS Code extension 0.4.0](https://github.com/marcosbuilds/Seloryn/releases/tag/vscode-v0.4.0): editor preview with syntax highlighting, completions, CLI diagnostics and `.sely` file icons.

These are verified previews, not a stable ABI or a claim of GPU tensor/MLP
training, distributed execution, package registry, HTTP runtime or a general
model framework.

![Seloryn architecture](assets/architecture-flow.png)

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

| Capability | Availability in 0.07 | Evidence or boundary |
|:--|:--|:--|
| UTF-8 lexer, layout and parser subset | Implemented and tested | [`src/parser.rs`](src/parser.rs), parser tests |
| Names, scalar types, records, enums, `Result`, `Option` and `match` checks | Experimental partial checker | [`src/semantic.rs`](src/semantic.rs), semantic tests |
| `seloryn check <file.sely or project>` diagnostics | Implemented in the development CLI | [`src/main.rs`](src/main.rs), [`tests/bootstrap_cli.rs`](tests/bootstrap_cli.rs), [`tests/project_manifest_probe.py`](tests/project_manifest_probe.py) |
| Pure scalar/list interpreter and `seloryn run` | Experimental E4 implementation | [`src/runtime.rs`](src/runtime.rs), runtime and CLI tests; no effects |
| CPU `Tensor[F32]` / `Tensor[I64]` buffers and N1/N2 operations | Experimental N1/N2 implementation | [`src/tensor`](src/tensor), tensor unit and CLI tests; CPU only |
| Typed tensor reads and lossless dtype conversion | Experimental N1 implementation | `tensor_get_f32` / `tensor_get_i64`, `tensor_to_f32` / `tensor_to_i64`, bounds and dtype regressions in [`tests/tensor_cpu.rs`](tests/tensor_cpu.rs) |
| Deterministic core IR and execution boundary | Experimental implemented | `src/ir.rs`, runtime parity tests; bounded subset only |
| Local modules and strict project manifests | Experimental implemented | Public project fixtures, real TOML parsing and future-language rejection; no registry or external package resolver |
| VS Code syntax highlighting, native completion, saved-file CLI diagnostics and optional `.sely` file icons | Verified editor preview | [`tooling/vscode`](tooling/vscode); headless tests, real CLI bridge fixture, reproducible local VSIX and CLI installation inspection; Explorer rendering remains environment-dependent |
| JSON read/parse/encode/get and bounded file capability | Experimental implemented | JSON project fixtures; paths remain root-confined |
| Scalar and Tensor[F32] reverse-mode tapes and finite-difference gradient check | Experimental implemented, narrow | [`examples/autograd`](examples/autograd), [`tests/n3_tensor_probe.py`](tests/n3_tensor_probe.py) |
| Bounded CPU MLP training, mini-batches, evaluation, deterministic seed and checkpoint/resume | Verified preview | [`examples/projects/mlp-training`](examples/projects/mlp-training), [`tests/mlp_training_probe.py`](tests/mlp_training_probe.py); CPU/single-process only |
| Opt-in human/JSON/JSON2 training event stream | Verified preview for CPU and bounded GPU-linear training | `seloryn run ... --log=human|json|json2`; JSON2 carries program/manifest/dataset/checkpoint/result identity, structured path redaction, replay verification and flushed stderr; [`verify-event-chain.py`](scripts/streaming/verify-event-chain.py), [`replay-receipt.py`](scripts/streaming/replay-receipt.py) and [`streaming_v2_probe.py`](tests/streaming_v2_probe.py) |
| Seloryn-authored multi-input gradients, bounded `range` and SGD loop | Verified preview | [`examples/projects/self-hosted-linear`](examples/projects/self-hosted-linear), [`tests/self_hosted_probe.py`](tests/self_hosted_probe.py); model/objective/update orchestration is `.sely` source |
| Physical hardware inventory, non-fallback GPU compute/readback probe and explicit GPU linear training | Verified preview on the declared host; capability-aware elsewhere | [`src/gpu.rs`](src/gpu.rs), [`tests/gpu_probe.py`](tests/gpu_probe.py), [`tests/gpu_training_probe.py`](tests/gpu_training_probe.py); the GPU trainer is scalar F32 linear only, device-resident parameters, one final readback, no silent CPU fallback |
| `build`/`exec` `.selyapp` bundle path | Experimental | Deterministic bundle and clean terminal fixtures; not an installer |
| HTTP, general GPU tensor/MLP training, packages and distributed execution | Not implemented or verified | `train_linear_gpu` is a closed proof profile, not a general GPU tensor backend |
| Local installer scripts | Development-only; Windows x64 lifecycle verified here | No public download, signing or cross-OS binary support |

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

[Getting started](docs/guide/start.md) · [Editor support](docs/guide/editor.md) · [Next executable gates](docs/project/next-gates.md) · [Architecture](docs/project/architecture.md) · [Grammar](docs/reference/grammar.md) · [Types](docs/reference/types.md) · [Errors](docs/reference/errors.md) · [Tensors](docs/compute/tensors.md) · [Training](docs/compute/training.md) · [LLM path](docs/compute/llms.md) · [Servers](docs/systems/http.md) · [Platform support](docs/guide/platforms.md) · [Releases](docs/project/versions.md) · [License](docs/project/licensing.md)

## References and visuals

Original Seloryn symbol and architecture diagrams are packaged under `assets/`. Vendor and service brand SVGs may be retrieved via the [theSVG catalog](https://thesvg.org/) when needed; they must not be presented as Seloryn's own mark. No external brand assets are bundled in this repository.

**Documentation map:** [Browse by category](docs/index.md).
