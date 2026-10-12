<div align="center">
  <img src="assets/seloryn-symbol.svg" alt="Seloryn symbol" width="88" height="88" />
  <h1>Seloryn</h1>
  <p><strong>Readable logic. Real computation.</strong></p>
  <p>A documented original programming language design for reliable AI systems.</p>
</div>

> **Current status: 0.00, experimental syntax and semantic checker.** Seloryn can validate a bounded `.sely` syntax and semantic subset through `check`; it cannot execute programs. There is no interpreter, tensor engine, training runtime, HTTP/file standard library, installable release or GPU backend. Code samples below are design concepts unless explicitly marked as checked behavior. See the [current status](docs/project/status.md) and the [E3 checklist](docs/project/e3-checklist.md).

> **No performance claims, compatibility badges, release downloads or training benchmarks are published until independently measured.** The attached `seloryn_e3_design_review.md` is the design reference for the intended language; this repository records only what is implemented and verified.

![Seloryn architecture](assets/architecture-flow.png)

## What exists today vs. what is designed

Seloryn is being designed as one expressive language for reliable AI development, integration and serving. The implemented result is currently a checker for the front end; the runtime and numerical layers remain roadmap work.

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
| Program execution, interpreter and core IR | Not implemented; planned for E4 | No executable runtime exists |
| Modules, files, HTTP, packages and standard library | Not implemented; imports are diagnosed as unavailable | No module or systems runtime exists |
| Tensor buffers, autodiff, training and model serving | Designed only; not checked or executable | See [tensor design](docs/compute/tensors.md) and [E3 gaps](docs/project/e3-checklist.md) |
| GPU, distributed execution and installers | Not implemented or verified | No backend or release artifact exists |

| AI development | AI integration | AI serving | Evidence |
|:--|:--|:--|:--|
| Typed data, model and workflow boundaries | Local models and external providers through explicit libraries | Checked request/model/response contracts | Diagnostics and negative tests |
| Result-based failures | Safe transport, serialization and compatibility boundaries | Resource, cancellation and deployment policy | Reproducible comparative experiments |
| Future tensor and autodiff APIs | Future model interchange and adapters | Future training and inference runtimes | Review the architecture when evidence fails |

## A glimpse of the intended language

```seloryn
record Sample:
    input: Tensor[F32, (Batch, Features)]
    target: Tensor[I64, (Batch,)]

fn accuracy(predicted: Tensor[I64, (Batch,)], expected: Tensor[I64, (Batch,)]) -> Result[F32, ShapeError]:
    require predicted.shape == expected.shape else ShapeError("shape mismatch")
    return Ok((predicted == expected).cast(F32).mean().item())
```

The illustrative snippet above describes a **future API**, not a program accepted by the current checker. It may change before acceptance. `require` inside fallible functions has a limited checker rule in the [language reference](docs/reference/grammar.md); see [examples](examples/tensor-tour.md) for a design-only training flow.

## Read next

[Getting started](docs/guide/start.md) · [Architecture](docs/project/architecture.md) · [Grammar](docs/reference/grammar.md) · [Types](docs/reference/types.md) · [Errors](docs/reference/errors.md) · [Tensors](docs/compute/tensors.md) · [Training](docs/compute/training.md) · [LLM path](docs/compute/llms.md) · [Servers](docs/systems/http.md) · [Platform support](docs/guide/platforms.md) · [Releases](docs/project/versions.md) · [License](docs/project/licensing.md)

## References and visuals

Original Seloryn symbol and architecture diagrams are packaged under `assets/`. Vendor and service brand SVGs may be retrieved via the [theSVG catalog](https://thesvg.org/) when needed; they must not be presented as Seloryn's own mark. No external brand assets are bundled in this design-only archive.

**Documentation map:** [Browse by category](docs/index.md).
