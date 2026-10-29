# Documentation

![Seloryn language flow](../assets/diagrams/architecture-flow.png)

Seloryn is a typed language and bounded Rust interpreter for studying reliable
data, model and execution contracts. The documentation distinguishes
implemented behavior, verified preview evidence and future design. The public
Rust source contains the lexer, parser, semantic checker, IR, interpreter,
compute, jobs, CLI and core LSP; auxiliary product source is distributed
separately from this repository.

## Choose a journey

| Goal | Start here | Then inspect |
|---|---|---|
| **Start the language** | [Getting started](guide/start.md) | [CLI](guide/cli.md), [grammar](reference/grammar.md), [examples](../examples/README.md) |
| **Learn the language** | [Language overview](reference/language.md) | [Types](reference/types.md), [patterns](reference/patterns.md), [errors](reference/errors.md), [modules](reference/modules.md) |
| **Study AI computation** | [Tensors](compute/tensors.md) | [Autograd](compute/autograd.md), [training](compute/training.md), [inference](compute/inference.md), [language-model proof](compute/language-model.md) |
| **Reproduce real proofs** | [Conformance](project/conformance.md) | [Evidence Lab](project/evidence-lab.md), [auditable inference](benchmarks/auditable-local-inference.md), [benchmark contracts](../benchmarks/README.md) |
| **Integrate systems** | [Python bridge](systems/python-bridge.md) | [HTTP boundary](systems/http.md), [automation](systems/automation.md), [security](systems/security.md) |
| **Understand internals** | [Architecture](project/architecture.md) | [Rust source map](../src/README.md), [status](project/status.md), [version ledger](project/versions.md) |
| **Download products** | [Downloads](downloads.md) | [Install guide](guide/install.md), [editor preview](guide/editor.md), [platform limits](guide/platforms.md) |
| **Contribute** | [Licensing](project/licensing.md) | [source map](../src/README.md), [tests](../tests/README.md), [scripts](../scripts/README.md) |

| Area | Start with | Continue |
|---|---|---|
| **Guide** | [Getting started](guide/start.md) | [Install](guide/install.md), [CLI](guide/cli.md), [Editor support](guide/editor.md), [Platforms](guide/platforms.md), [Visuals](guide/visuals.md) |
| **Language** | [Philosophy](reference/language.md) | [Grammar](reference/grammar.md), [Types](reference/types.md), [Patterns](reference/patterns.md), [Errors](reference/errors.md), [Modules](reference/modules.md) |
| **Compute** | [Tensors](compute/tensors.md) | [Autograd](compute/autograd.md), [Training](compute/training.md), [Executable language model](compute/language-model.md), [LLMs](compute/llms.md), [Inference](compute/inference.md) |
| **Systems** | [HTTP services](systems/http.md) | [Python bridge](systems/python-bridge.md), [Automation](systems/automation.md), [Security](systems/security.md) |
| **Project** | [Architecture](project/architecture.md) | [Evidence Lab](project/evidence-lab.md), [Conformance and evidence](project/conformance.md), [Benchmark Suite](../benchmarks/README.md), [Versioning](project/versions.md), [Status](project/status.md), [Licensing](project/licensing.md) |

The checked runnable examples are grouped under [`examples/`](../examples/README.md), including standalone CPU tensors and project examples for local modules, JSON, linear training, the verified-preview MLP workflow and the executable language-model proof. Other forward-looking system and model examples remain design documentation. The public benchmark harness lives under [`benchmarks/`](../benchmarks/README.md); machine-specific reports remain generated artifacts. Release history is summarized in [`CHANGELOG.md`](../CHANGELOG.md). Product downloads are indexed from the verified GitHub Releases API in [Downloads](downloads.md); the index never invents a stable release.
