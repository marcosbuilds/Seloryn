<div align="center">
  <img src="assets/seloryn-symbol.svg" alt="Seloryn symbol" width="88" height="88" />
  <h1>Seloryn</h1>
  <p><strong>Readable logic. Real computation.</strong></p>
  <p>A documented original programming language design for reliable AI systems.</p>
</div>

> **Current status: 0.00, experimental E3 checker, E4 pure interpreter and N1/N2 CPU tensor runtime.** Seloryn can validate and execute a bounded `.sely` subset through `check` and `run`. There is no module system, training runtime, HTTP/file standard library, installable release or GPU backend. Code samples below are design concepts unless explicitly marked as checked behavior. See the [current status](docs/project/status.md) and [tensor reference](docs/compute/tensors.md).

> **No performance claims, compatibility badges, release downloads or training benchmarks are published until independently measured.** This repository records only what is implemented and verified.

![Seloryn architecture](assets/architecture-flow.png)

## What exists today vs. what is designed

Seloryn is being designed as one expressive language for reliable AI development, integration and serving. The implemented result is a checker plus a pure interpreter for the validated scalar core and a bounded CPU tensor core; effects, training and accelerator layers remain roadmap work.

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
| Core IR | Not implemented; planned after interpreter evidence | The interpreter evaluates the checked AST directly |
| Modules, files, HTTP, packages and standard library | Not implemented; imports are diagnosed as unavailable | No module or systems runtime exists |
| Autodiff, training and model serving | Designed only; not checked or executable | See [autodiff](docs/compute/autograd.md) and [training](docs/compute/training.md) |
| GPU, distributed execution and installers | Not implemented or verified | No backend or release artifact exists |

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

The function above is intentionally illustrative beyond the current callable API. The verified runnable tensor examples are [`examples/tensor-matmul.sely`](examples/tensor-matmul.sely) and [`examples/tensor-numeric.sely`](examples/tensor-numeric.sely); they return real CPU results through `seloryn run`.

## Read next

[Getting started](docs/guide/start.md) · [Architecture](docs/project/architecture.md) · [Grammar](docs/reference/grammar.md) · [Types](docs/reference/types.md) · [Errors](docs/reference/errors.md) · [Tensors](docs/compute/tensors.md) · [Training](docs/compute/training.md) · [LLM path](docs/compute/llms.md) · [Servers](docs/systems/http.md) · [Platform support](docs/guide/platforms.md) · [Releases](docs/project/versions.md) · [License](docs/project/licensing.md)

## References and visuals

Original Seloryn symbol and architecture diagrams are packaged under `assets/`. Vendor and service brand SVGs may be retrieved via the [theSVG catalog](https://thesvg.org/) when needed; they must not be presented as Seloryn's own mark. No external brand assets are bundled in this repository.

**Documentation map:** [Browse by category](docs/index.md).
