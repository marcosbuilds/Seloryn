# Source map

This is the canonical map of the public Rust implementation. The tree is
organized by responsibility, not by historical file size. The binary entry
point is intentionally thin; the language, interpreter, compute, jobs and LSP
remain open-source implementation modules in this crate.

| Path | Responsibility | Main navigation |
|---|---|---|
| `bin/seloryn.rs` | Process entry point and exit-code handoff | `cli::run` |
| `cli/` | Command routing, workspace commands and terminal/event output | `check`, `run`, `inspect`, `job`, `lsp` |
| `language/lexer.rs` | UTF-8 tokenization and layout | `language::parser` |
| `language/parser.rs` | Source parser and syntax tree | `language::semantic`, `language::ir` |
| `language/semantic/` | Type/symbol model and semantic checking | `language::semantic::check` |
| `language/ir.rs` | Deterministic lowering boundary | `runtime::execute_ir_with_context` |
| `language/analysis.rs` | Bounded symbols/imports/diagnostic inspection | `cli inspect`, LSP |
| `runtime/` | Bounded interpreter, effects, training and cancellation | `runtime::execute_with_output` |
| `compute/` | GPU adapter boundary, scalar autodiff and tensor compute | `compute::tensor` |
| `compute/tensor/` | Tensor storage, shapes, operations and errors | `Tensor`, `TensorDtype` |
| `jobs/` | Job contracts, state/event persistence and worker lifecycle | `job` CLI |
| `project/` | Manifest, local module and bundle loading | `workspace`, `build`, `exec` |
| `protocols/lsp.rs` | Core parser-backed Language Server Protocol | `lsp --stdio` |
| `data/` | Shared bounded data integrity helpers | SHA-256 contracts |
| `diagnostic.rs`, `json.rs`, `builtins.rs` | Cross-domain contracts with narrow ownership | Imported by their consumers |

## Critical paths

```text
source → language/lexer → language/parser → language/semantic → language/ir
                                                        ↓
                                             runtime interpreter

CLI → project/workspace, jobs, runtime, protocols/lsp, compute
runtime training → compute/tensor + data integrity → CLI event receipts
```

The interpreter is not a wrapper: `runtime/mod.rs` contains execution and
effect boundaries, while `runtime/training.rs` owns the training-specific
serialization and numerical orchestration. Tensor storage is separate from
tensor operations so allocation/indexing ownership can be studied without
burying kernels in a CLI module. Visibility is crate-local or module-local
unless a public Rust API is required by the implementation.

The public source is a bootstrap binary crate with an internal library root;
this does not promise a stable external Rust API. Run the full public checks
from the repository root with `scripts/ci/verify-public.ps1` or
`scripts/ci/verify-public.sh`.
