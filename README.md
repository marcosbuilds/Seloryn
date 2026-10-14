<div align="center">
  <img src="assets/seloryn-symbol.svg" alt="Seloryn symbol" width="88" height="88" />
  <h1>Seloryn</h1>
  <p><strong>Readable logic. Real computation.</strong></p>
  <p>A documented original programming language design for reliable AI systems.</p>
</div>

> **Current status: 0.00, experimental checker/runtime, bounded N1/N2 CPU tensors, narrow scalar-autograd, linear-training and checkpoint-schema fixtures, and deterministic bundles.** Seloryn can validate and execute a bounded `.sely` subset through `check`, `run` and local project commands. There is no package registry, general effects runtime, supported release or public installer; local development scripts only install an existing binary. Code samples below are design concepts unless explicitly marked as checked behavior. See the [current status](docs/project/status.md) and [tensor reference](docs/compute/tensors.md).

> **No performance claims, compatibility badges, release downloads or training benchmarks are published until independently measured.** This repository records only what is implemented and verified.

![Seloryn architecture](assets/architecture-flow.png)

## What exists today vs. what is designed

Seloryn is being designed as one expressive language for reliable AI development, integration and serving. The implemented result is a checker, deterministic core IR, pure interpreter, local modules, bounded JSON/file capabilities, narrow model/training fixtures and a reproducible experimental bundle path. General effects, broad model APIs and accelerator layers remain roadmap work.

## Mission: AI development and integration

Seloryn's primary purpose is to help a developer build and integrate complete
AI systems with fewer invalid states, fewer adaptations between development and
production, and better diagnostics at model, data and service boundaries. This
is a hypothesis to prove with real comparative experiments, not a performance
claim. If equivalent workloads do not show the proposed reduction in failures,
integration drift or diagnostic cost, the architecture must be reconsidered
instead of gaining features by default.

| Capability | Availability in 0.00 | Evidence or boundary |
|:--|:--|:--|
| UTF-8 lexer, layout and parser subset | Implemented and tested | `src/parser.rs:224-1180`, parser tests |
| Names, scalar types, records, enums, `Result`, `Option` and `match` checks | Experimental partial checker | `src/semantic.rs:101-1115`, semantic tests |
| `seloryn check <file.sely>` diagnostics | Implemented in the development CLI | `src/main.rs:82-119`, `tests/bootstrap_cli.rs:61-178` |
| Pure scalar/list interpreter and `seloryn run` | Experimental E4 implementation | [`src/runtime.rs`](src/runtime.rs), runtime and CLI tests; no effects |
| CPU `Tensor[F32]` / `Tensor[I64]` buffers and N1/N2 operations | Experimental N1/N2 implementation | [`src/tensor`](src/tensor), tensor unit and CLI tests; CPU only |
| Typed tensor reads and lossless dtype conversion | Experimental N1 implementation | `tensor_get_f32` / `tensor_get_i64`, `tensor_to_f32` / `tensor_to_i64`, bounds and dtype regressions in [`tests/tensor_cpu.rs`](tests/tensor_cpu.rs) |
| Deterministic core IR and execution boundary | Experimental implemented | `src/ir.rs`, runtime parity tests; bounded subset only |
| Local modules and project manifests | Experimental implemented | Public project fixtures; no registry or external package resolver |
| JSON read/parse/encode/get and bounded file capability | Experimental implemented | JSON project fixtures; paths remain root-confined |
| Scalar reverse-mode tape, finite-difference gradient check, linear-training and checkpoint-validation fixtures | Experimental implemented, narrow | [`examples/autograd`](examples/autograd), runtime tests and public linear example; no tensor autograd, general provider or MLP claim |
| Opt-in sampled training events | Experimental implemented, narrow | `seloryn run ... --log=human|json`; stderr-only lifecycle for `training.train_linear`, not general telemetry |
| `build`/`exec` `.selyapp` bundle path | Experimental | Deterministic bundle and clean terminal fixtures; not an installer |
| HTTP, packages, GPU and distributed execution | Not implemented or verified | No supported release artifact exists |
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

[Getting started](docs/guide/start.md) · [Architecture](docs/project/architecture.md) · [Grammar](docs/reference/grammar.md) · [Types](docs/reference/types.md) · [Errors](docs/reference/errors.md) · [Tensors](docs/compute/tensors.md) · [Training](docs/compute/training.md) · [LLM path](docs/compute/llms.md) · [Servers](docs/systems/http.md) · [Platform support](docs/guide/platforms.md) · [Releases](docs/project/versions.md) · [License](docs/project/licensing.md)

## References and visuals

Original Seloryn symbol and architecture diagrams are packaged under `assets/`. Vendor and service brand SVGs may be retrieved via the [theSVG catalog](https://thesvg.org/) when needed; they must not be presented as Seloryn's own mark. No external brand assets are bundled in this repository.

**Documentation map:** [Browse by category](docs/index.md).
