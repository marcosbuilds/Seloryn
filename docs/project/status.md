# Current capability status

**Primary truth source for Seloryn 0.00.** The repository contains an experimental syntax and semantic checker, a bounded pure E4 interpreter and a bounded pure N1 CPU tensor runtime. It does not contain a core IR, effects runtime, accelerator backend or public release. This page reports the observed implementation boundary.

## What works today

| Area | State | Evidence |
|---|---|---|
| UTF-8 lexer, layout and parser subset | Experimental implemented and tested | [`parse`](../../src/parser.rs#L224), parser tests |
| Name resolution and local scopes | Partial E3 checker | [`collect_declarations`](../../src/semantic.rs#L101), [`lookup`](../../src/semantic.rs#L1039) |
| Scalar types and local inference | Partial E3 checker | [`type_of_expr`](../../src/semantic.rs#L652), semantic tests |
| Records, enums and field/constructor checks | Partial E3 checker | [`type_of_call`](../../src/semantic.rs#L845), semantic tests |
| `Result`, `Option`, `require`, postfix `?` and `match` coverage | Experimental partial checker | [`check_require`](../../src/semantic.rs#L371), [`check_match`](../../src/semantic.rs#L409) |
| Pure E4 interpreter for scalar/list values, list indexing and iteration, mutable `var` assignment, records, enums, `Result`, `Option`, calls, control flow and pipelines | Experimental implemented and tested | [`execute`](../../src/runtime.rs#L121), runtime and CLI tests |
| N1 CPU tensors (`Tensor[F32]`/`Tensor[I64]`) | Experimental implemented and tested | [`src/tensor`](../../src/tensor), [`tensor_cpu.rs`](../../tests/tensor_cpu.rs) |
| CLI `--help`, `--version`, `check` and `run` | Implemented development CLI | [`check_path`](../../src/main.rs#L86), [`run_path`](../../src/main.rs#L132), [CLI tests](../../tests/bootstrap_cli.rs#L61) |

## What is not implemented

| Capability | State | Consequence |
|---|---|---|
| Core IR | **Not implemented** | E4 evaluates the checked AST directly; no lowering or optimizer exists |
| Modules, packages and standard library | **Not implemented** | imports return an explicit unavailable diagnostic |
| Files, HTTP, processes and automation | **Not implemented** | no systems program can run |
| CPU tensor buffers, shape checks and reference kernels | **Experimental N1** | bounded F32/I64 construction, broadcasting, reductions and rank-2 F32 matmul; no model/training claim |
| Autograd, optimizers, checkpoints and training | **Not implemented** | no model can train |
| GPU, distributed execution and serving | **Not implemented** | no accelerator or production service exists |
| Formatter, LSP, installer and release artifacts | **Not implemented or verified** | no supported installation path exists |

## E3 gate fulfillment

Percentages below are engineering estimates of the design gate, not test coverage and not a version score.

| Gate | Estimate | Evidence | Remaining boundary |
|---|---:|---|---|
| E3.0 — grammar and AST preflight | 80% | [`parser.rs`](../../src/parser.rs#L224), precedence and reserved-syntax tests | Full grammar, closures, generic shape syntax and runtime meaning remain outside the subset |
| E3.1 — names and scopes | 70% | [`collect_declarations`](../../src/semantic.rs#L101), [`lookup`](../../src/semantic.rs#L1039) | Module visibility/import resolution is unavailable; `pub` is metadata plus signature validation, not a package export system |
| E3.2 — types and inference | 65% | [`type_of_expr`](../../src/semantic.rs#L652), [`type_from_expr`](../../src/semantic.rs#L1005) | No user generics or symbolic shape constraints |
| E3.3 — records, enums and fallibility | 75% | [`check_require`](../../src/semantic.rs#L371), [`type_of_call`](../../src/semantic.rs#L845), Result/Option tests | General error conversion and runtime resource behavior remain outside E3/E4 |
| E3.4 — exhaustive `match` | 70% | [`check_match`](../../src/semantic.rs#L409), [`required_coverage`](../../src/semantic.rs#L632), match tests | Structural/named-field patterns and ranges remain outside; the validated constructor subset executes |
| E3.5 — semantic CLI proof | 55% | [`check_path`](../../src/main.rs#L83), 15 CLI integration tests | No JSON diagnostics, property/fuzz campaign, golden corpus or Linux/macOS execution evidence |

## E4 execution evidence

E4 is **experimental implemented** for the validated E3 subset. `seloryn run`
reads UTF-8 source, stops on any lexical, parse or semantic diagnostic, then
executes `fn main()` through the pure tree-walk interpreter. The runtime covers
`Bool`, `I64`, `F64`, `Text`, `Unit`, `List`, nominal records, closed enums,
`Option`, `Result`, `if`, `match`, `require`, calls, recursion, return,
`??`, postfix `?`, list indexing, list iteration, mutable `var` assignment, list
concatenation and the named-function `|>` pipeline. Runtime failures carry
stable `SEL-R***` codes, source spans and a deterministic function stack. Call
depth, loop size and list bounds are bounded to keep user programs from
overflowing the host runtime.

Evidence is [`src/runtime.rs`](../../src/runtime.rs) plus the CLI tests in
[`tests/bootstrap_cli.rs`](../../tests/bootstrap_cli.rs). The interpreter does
not perform filesystem, network, process, module or GPU effects. The N1 bridge
adds owned CPU buffers and numeric kernels without adding effects or a `print`
API. The core IR and all systems/runtime gates remain future work.

## N1 CPU tensor evidence

N1 is experimentally implemented for owned contiguous F32/I64 CPU tensors. The
runtime checks rank (maximum 32), nonnegative dimensions, checked element and
byte products, a 64 MiB per-tensor and process-wide resident-buffer budget,
row-major strides, reshape counts, same-dtype arithmetic, right-aligned
broadcasting, finite F32 results, checked I64 arithmetic, total and axis
reductions and rank-2 F32 matrix multiplication. The language bridge is covered
by positive and negative CLI tests, including CRLF, Unicode paths, static dtype
rejection, bad shapes, incompatible matmul, invalid axes, per-tensor and global
budget overflow. No GPU, autograd, training, tensor indexing, symbolic shape or
performance claim follows from N1.

## Gaps versus the design review

E3 is therefore **not fully complete**. The checker proves a meaningful front-end subset, but E3.3, E3.4 and E3.5 still have the boundaries listed above. In particular:

- A successful `check` means syntax and supported static rules passed; `run` adds the separately tested pure E4 execution step.
- Training, model-serving and GPU text in the repository describes design targets, not available capabilities.
- Examples using `print`, modules, named record patterns or future libraries remain design concepts unless a page explicitly says otherwise.
- E4 and N1 have bounded execution results; the core IR and later numeric gates remain future work.

The implementation policy is staged: a feature becomes available only after source behavior, negative tests and platform evidence exist. The language version remains `0.00`; no release or earned gain is claimed by this status page.
