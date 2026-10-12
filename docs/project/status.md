# Current capability status

**Primary truth source for Seloryn 0.00.** The repository contains an experimental syntax and semantic checker plus a bounded pure E4 interpreter. It does not contain a core IR, effects runtime, tensor runtime or public release. The attached `seloryn_e3_design_review.md` defines the intended E3 gate; this page reports the narrower observed implementation.

## What works today

| Area | State | Evidence |
|---|---|---|
| UTF-8 lexer, layout and parser subset | Experimental implemented and tested | [`parse`](../../src/parser.rs#L224), parser tests |
| Name resolution and local scopes | Partial E3 checker | [`collect_declarations`](../../src/semantic.rs#L101), [`lookup`](../../src/semantic.rs#L1039) |
| Scalar types and local inference | Partial E3 checker | [`type_of_expr`](../../src/semantic.rs#L652), semantic tests |
| Records, enums and field/constructor checks | Partial E3 checker | [`type_of_call`](../../src/semantic.rs#L845), semantic tests |
| `Result`, `Option`, `require`, postfix `?` and `match` coverage | Experimental partial checker | [`check_require`](../../src/semantic.rs#L371), [`check_match`](../../src/semantic.rs#L409) |
| Pure E4 interpreter for scalar/list values, records, enums, `Result`, `Option`, calls, control flow and pipelines | Experimental implemented and tested | [`execute`](../../src/runtime.rs#L121), runtime tests |
| CLI `--help`, `--version`, `check` and `run` | Implemented development CLI | [`check_path`](../../src/main.rs#L86), [`run_path`](../../src/main.rs#L132), [CLI tests](../../tests/bootstrap_cli.rs#L61) |

## What is not implemented

| Capability | State | Consequence |
|---|---|---|
| Core IR | **Not implemented** | E4 evaluates the checked AST directly; no lowering or optimizer exists |
| Modules, packages and standard library | **Not implemented** | imports return an explicit unavailable diagnostic |
| Files, HTTP, processes and automation | **Not implemented** | no systems program can run |
| Tensor buffers, kernels and shape execution | **Not implemented** | no numerical model can run |
| Autograd, optimizers, checkpoints and training | **Not implemented** | no model can train |
| GPU, distributed execution and serving | **Not implemented** | no accelerator or production service exists |
| Formatter, LSP, installer and release artifacts | **Not implemented or verified** | no supported installation path exists |

## E3 gate fulfillment

Percentages below are engineering estimates of the design gate, not test coverage and not a version score.

| Gate | Estimate | Evidence | Remaining boundary |
|---|---:|---|---|
| E3.0 — grammar and AST preflight | 80% | [`parser.rs`](../../src/parser.rs#L224), precedence and reserved-syntax tests | Full grammar, closures, indexing, generic shape syntax and runtime meaning remain outside the subset |
| E3.1 — names and scopes | 70% | [`collect_declarations`](../../src/semantic.rs#L101), [`lookup`](../../src/semantic.rs#L1039) | Module visibility/import resolution is unavailable; `pub` is metadata plus signature validation, not a package export system |
| E3.2 — types and inference | 65% | [`type_of_expr`](../../src/semantic.rs#L652), [`type_from_expr`](../../src/semantic.rs#L1005) | No assignment checking, overflow model, user generics, tensor types or shape constraints |
| E3.3 — records, enums and fallibility | 75% | [`check_require`](../../src/semantic.rs#L371), [`type_of_call`](../../src/semantic.rs#L845), Result/Option tests | General error conversion and runtime resource behavior remain outside E3/E4 |
| E3.4 — exhaustive `match` | 70% | [`check_match`](../../src/semantic.rs#L409), [`required_coverage`](../../src/semantic.rs#L632), match tests | Structural/named-field patterns, ranges and execution semantics are not implemented |
| E3.5 — semantic CLI proof | 55% | [`check_path`](../../src/main.rs#L83), 14 CLI integration tests | No JSON diagnostics, property/fuzz campaign, golden corpus or Linux/macOS execution evidence |

## E4 execution evidence

E4 is **experimental implemented** for the validated E3 subset. `seloryn run`
reads UTF-8 source, stops on any lexical, parse or semantic diagnostic, then
executes `fn main()` through the pure tree-walk interpreter. The runtime covers
`Bool`, `I64`, `F64`, `Text`, `Unit`, `List`, nominal records, closed enums,
`Option`, `Result`, `if`, `match`, `require`, calls, recursion, return,
`??`, postfix `?` and the named-function `|>` pipeline. Runtime failures carry
stable `SEL-R***` codes, source spans and a deterministic function stack.

Evidence is [`src/runtime.rs`](../../src/runtime.rs) plus the 14 CLI tests in
[`tests/bootstrap_cli.rs`](../../tests/bootstrap_cli.rs). The interpreter does
not perform filesystem, network, process, module, tensor, GPU or other future
effects. Lists are constructible but indexing/iteration are outside the current
grammar. The core IR and all systems/runtime gates remain future work.

## Gaps versus the design review

E3 is therefore **not fully complete**. The checker proves a meaningful front-end subset, but E3.3, E3.4 and E3.5 still have the boundaries listed above. In particular:

- A successful `check` means syntax and supported static rules passed; `run` adds the separately tested pure E4 execution step.
- Tensor, training, model-serving and GPU text in the repository describes design targets, not available capabilities.
- Examples using `Tensor`, `print`, modules, named record patterns or future libraries remain design concepts unless a page explicitly says otherwise.
- E4 now has a bounded execution result; N1 tensor work still depends on a separate tensor representation and numeric correctness gate.

The implementation policy is staged: a feature becomes available only after source behavior, negative tests and platform evidence exist. The language version remains `0.00`; no release or earned gain is claimed by this status page.
