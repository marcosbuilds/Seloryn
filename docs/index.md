# Documentation

![Seloryn language flow](../assets/architecture-flow.png)

The public documentation is deliberately short-named and categorized around
Seloryn's AI-first mission: development, integration and serving with explicit
typed boundaries. Every page distinguishes a proposed contract from an
implemented, verified feature. **The E4 interpreter and bounded N1/N2 CPU
tensor runtimes are bounded CPU implementations; small MLP training is a
verified preview while broader training remains outside the evidence.**

| Area | Start with | Continue |
|---|---|---|
| **Guide** | [Getting started](guide/start.md) | [Install](guide/install.md), [CLI](guide/cli.md), [Editor support](guide/editor.md), [Platforms](guide/platforms.md), [Visuals](guide/visuals.md) |
| **Language** | [Philosophy](reference/language.md) | [Grammar](reference/grammar.md), [Types](reference/types.md), [Patterns](reference/patterns.md), [Errors](reference/errors.md), [Modules](reference/modules.md) |
| **Compute** | [Tensors](compute/tensors.md) | [Autograd](compute/autograd.md), [Training](compute/training.md), [Executable language model](compute/language-model.md), [LLMs](compute/llms.md), [Inference](compute/inference.md) |
| **Systems** | [HTTP services](systems/http.md) | [Automation](systems/automation.md), [Security](systems/security.md) |
| **Project** | [Architecture](project/architecture.md) | [Evidence Lab](project/evidence-lab.md), [Conformance and evidence](project/conformance.md), [Benchmark Suite](../benchmarks/README.md), [AI development evaluation](benchmarks/agent-token-economy.md), [Next executable gates](project/next-gates.md), [Versioning](project/versions.md), [Status](project/status.md), [Licensing](project/licensing.md) |

The checked runnable examples are grouped under [`examples/`](../examples/README.md), including standalone CPU tensors and project examples for local modules, JSON, linear training, the verified-preview MLP workflow and the executable language-model proof. Other forward-looking system and model examples remain design documentation. The public benchmark harness and AI-development evaluation contracts live under [`benchmarks/`](../benchmarks/README.md) and [`research/agent-token-economy/`](../research/agent-token-economy/README.md); machine-specific reports remain generated artifacts. Release history is summarized in [`CHANGELOG.md`](../CHANGELOG.md).
