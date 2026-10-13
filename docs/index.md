# Documentation

![Seloryn language flow](../assets/architecture-flow.png)

The public documentation is deliberately short-named and categorized around
Seloryn's AI-first mission: development, integration and serving with explicit
typed boundaries. Every page distinguishes a proposed contract from an
implemented, verified feature. **The E4 interpreter and bounded N1/N2 CPU
tensor runtimes are experimental and pure; training is limited to a narrow
fixture contract and accelerator engines do not exist yet.**

| Area | Start with | Continue |
|---|---|---|
| **Guide** | [Getting started](guide/start.md) | [Install](guide/install.md), [CLI](guide/cli.md), [Platforms](guide/platforms.md), [Visuals](guide/visuals.md) |
| **Language** | [Philosophy](reference/language.md) | [Grammar](reference/grammar.md), [Types](reference/types.md), [Patterns](reference/patterns.md), [Errors](reference/errors.md), [Modules](reference/modules.md) |
| **Compute** | [Tensors](compute/tensors.md) | [Autograd](compute/autograd.md), [Training](compute/training.md), [LLMs](compute/llms.md), [Inference](compute/inference.md) |
| **Systems** | [HTTP services](systems/http.md) | [Automation](systems/automation.md), [Security](systems/security.md) |
| **Project** | [Architecture](project/architecture.md) | [Versioning](project/versions.md), [Status](project/status.md), [Licensing](project/licensing.md) |

The checked runnable examples are grouped under [`examples/`](../examples/README.md), including standalone CPU tensors and project examples for local modules, JSON and the narrow linear-training fixture. Other forward-looking system and model examples remain design documentation.
