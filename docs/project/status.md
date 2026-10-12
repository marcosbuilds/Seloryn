# Current capability status

**Staged implementation baseline (`0.00`); no public release.**

| Area | State | Evidence |
|---|---|---|
| Grammar and language contracts | Proposed documentation | Reference pages only |
| Lexer and syntax parser | Experimental implemented | [`check`](../guide/cli.md), [bootstrap tests](../../tests/bootstrap_cli.rs) |
| Type checker and semantic analysis | Not implemented | No artifacts |
| Interpreter and runtime | Not implemented | No binary |
| CLI bootstrap | Experimental implemented | [`--help`/`--version`](../guide/cli.md), [`check`](../guide/cli.md) |
| Formatter, language server | Not implemented | No executable |
| HTTP, files, automation | Not implemented | No standard library |
| Tensor CPU engine | Not implemented | No kernels |
| Autograd or model training | Not implemented | No experiment |
| GPU/distributed/LLM training | Not implemented | No backend |
| Windows/Linux/macOS support | Not verified | No packaged target |

This page must be updated on verified public product changes. Documentation about a feature **must not be treated as implementation**. The `README` claims exactly this current status.
