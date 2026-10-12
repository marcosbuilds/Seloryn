# E4 interpreter checklist

This page records the observed E4 execution boundary for Seloryn 0.00. It is
evidence for the current development tree, not a release or a language-version
credit.

## Status

**Experimental implemented.** `seloryn run <file.sely>` performs UTF-8 reading,
lexing, parsing and semantic checking before calling the pure interpreter. Any
lexical, parse or semantic diagnostic stops the pipeline before evaluation.

## Executed subset

The interpreter evaluates:

- `Bool`, `I64`, `F64`, `Text`, `Unit` and constructed `List` values;
- nominal record construction and field access;
- closed enum constructors and exhaustive `match` cases, including binders;
- `Some`/`None` and `Ok`/`Err` values;
- `if`/`elif`/`else`, `match`, `require`, calls, recursion and `return`;
- `??`, postfix `?`, short-circuit `and`/`or` and named-function `|>`.

Results are rendered deterministically to stdout. Runtime failures use stable
`SEL-R***` codes, source spans and a function stack. Integer overflow,
division by zero, invalid numeric values, invalid calls and impossible runtime
matches are reported instead of panicking.

## Evidence map

| Requirement | Evidence |
|---|---|
| Positive scalar, conditional and recursive execution | `src/runtime.rs` unit tests |
| Lists, records, enums and pipelines | `src/runtime.rs` unit tests |
| `Option`, `Result`, `require`, `?` and `match` | `src/runtime.rs` unit tests |
| Static failures stop before execution | `tests/bootstrap_cli.rs` |
| Deterministic runtime diagnostic and stack | `src/runtime.rs`, `tests/bootstrap_cli.rs` |
| CRLF and Unicode execution | `tests/bootstrap_cli.rs` |
| CMD, PowerShell 5.1 and PowerShell 7 smoke | `tests/e4-cli-smoke.sely`, release binary invocation |

## Explicit boundary

The interpreter is a pure tree-walk evaluator over the checked AST. It does not
open files, use the network, spawn processes, load modules, allocate tensors,
use GPUs or provide a `print` effect. Lists have construction and rendering but
not indexing or iteration because those constructs are outside the validated
grammar. The core IR is not implemented. These boundaries keep E4 compatible
with the AI-first mission without adding AI keywords, training syntax, visual
features or tensor surface area.
