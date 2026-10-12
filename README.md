<div align="center">
  <img src="assets/seloryn-symbol.svg" alt="Seloryn symbol" width="88" height="88" />
  <h1>Seloryn</h1>
  <p><strong>Readable logic. Real computation.</strong></p>
  <p>A planned original programming language for reliable applications, tensors, model training and inference.</p>
</div>

> **Project status: staged implementation — 0.00.** The native CLI exposes `--help`, `--version` and experimental `check`, which validates the E3 syntax and semantic core. The interpreter, compiler, language libraries, GPU support and installers do not exist yet. Code samples throughout this repository show the **proposed language contract**, not running programs. [Current status](docs/project/status.md).

![Seloryn architecture](assets/architecture-flow.png)

## What Seloryn is designed to do

Seloryn is being designed as one expressive language for services and numerical computing. Its contract calls for strong diagnostics, explicit errors, immutable-by-default bindings, exhaustive pattern matching, composable functions, typed tensor shapes, automatic differentiation and cross-platform tooling. Reliability matters as much as surface syntax.

| Language | Systems | Machine learning | Delivery |
|:--|:--|:--|:--|
| Strictly defined grammar | Local services, network requests, async work | Tensors, gradient graphs, optimizers | Reproducible builds, CLI, diagnostics |
| Result-based errors | Files, processes, streams | Accelerator backends, model checkpoints | Package imports, platform adapters |
| Typed interfaces | Secure localhost defaults | Training, inference, future distributed execution | Explicit capability and support status |

## A glimpse of the intended language

```seloryn
record Sample:
    input: Tensor[F32, (Batch, Features)]
    target: Tensor[I64, (Batch,)]

fn accuracy(predicted: Tensor[I64, (Batch,)], expected: Tensor[I64, (Batch,)]) -> Result[F32, ShapeError]:
    require predicted.shape == expected.shape else ShapeError("shape mismatch")
    return Ok((predicted == expected).cast(F32).mean().item())
```

The illustrative snippet above describes a future API and may change before acceptance. `require` inside fallible functions has strict rules in the [language reference](docs/reference/grammar.md); see [examples](examples/tensor-tour.md) for a fuller training flow.

## Read next

[Getting started](docs/guide/start.md) · [Architecture](docs/project/architecture.md) · [Grammar](docs/reference/grammar.md) · [Types](docs/reference/types.md) · [Errors](docs/reference/errors.md) · [Tensors](docs/compute/tensors.md) · [Training](docs/compute/training.md) · [LLM path](docs/compute/llms.md) · [Servers](docs/systems/http.md) · [Platform support](docs/guide/platforms.md) · [Releases](docs/project/versions.md) · [License](docs/project/licensing.md)

## References and visuals

Original Seloryn symbol and architecture diagrams are packaged under `assets/`. Vendor and service brand SVGs may be retrieved via the [theSVG catalog](https://thesvg.org/) when needed; they must not be presented as Seloryn's own mark. No external brand assets are bundled in this design-only archive.

**No performance claims, compatibility badges, release downloads or training benchmarks are published until independently measured.**

**Documentation map:** [Browse by category](docs/index.md).
