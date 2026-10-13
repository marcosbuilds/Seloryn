# Source layout

The source tree is intentionally split by runtime responsibility while the
bootstrap crate still uses Rust modules at the root:

| Path | Responsibility |
|---|---|
| `main.rs` | CLI commands, output channels and exit codes |
| `lexer.rs`, `parser.rs`, `semantic.rs` | UTF-8 frontend and partial checker |
| `diagnostic.rs` | structured source and runtime diagnostics |
| `ir.rs`, `runtime.rs` | deterministic lowering and bounded interpreter |
| `project.rs` | local project/module loading and `.selyapp` bundles |
| `json.rs` | bounded native JSON value/parser/encoder |
| `builtins.rs` | tensor builtin catalog |
| `tensor/` | bounded CPU tensor shapes, operations and errors |

The root modules are not interchangeable libraries: JSON, project effects,
models and training remain narrow runtime contracts with explicit tests.
