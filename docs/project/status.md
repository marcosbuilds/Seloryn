# Current capability status

**Primary truth source for Seloryn 0.00.** The repository contains an experimental syntax and semantic checker, a deterministic lowered core IR, a bounded pure E4 interpreter, local project modules, bounded JSON capability, narrow model/training fixtures and bounded pure N1/N2 CPU tensor runtimes. It does not contain a package registry, general effects runtime, accelerator backend, supported installer or public release. This page reports the observed implementation boundary.

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
| N2 CPU numeric closure | Experimental implemented and tested | [`src/tensor/ops.rs`](../../src/tensor/ops.rs), [`n2_probe.py`](../../tests/n2_probe.py), [`tensor-numeric.sely`](../../examples/tensors/tensor-numeric.sely) |
| Deterministic core IR boundary | Experimental implemented and tested | [`src/ir.rs`](../../src/ir.rs), runtime and lowering tests |
| Local project modules and `seloryn.toml` | Experimental implemented and tested | [`src/project.rs`](../../src/project.rs), [`json-config`](../../examples/projects/json-config/README.md) |
| JSON read/parse/encode/get with root and size limits | Experimental implemented and tested | [`src/json.rs`](../../src/json.rs), [`json-config`](../../examples/projects/json-config/README.md) |
| Model contract, scalar reverse-mode tape, finite-difference check, linear training and checkpoint-schema fixtures | Experimental implemented, narrow | [`autograd`](../../examples/autograd/README.md), runtime/N3a probe and [`linear-training`](../../examples/projects/linear-training/README.md); no tensor tape or general trainer |
| CLI `--help`, `--version`, `check`, `run`, `build` and `exec` | Implemented development CLI | [`src/main.rs`](../../src/main.rs), CLI and B1 fixtures |
| Human/JSON diagnostics, stable source spans, `--quiet` and `from io import print` | Experimental F1 subset implemented and tested | [`Diagnostic`](../../src/diagnostic.rs), [`--format=json`](../../src/main.rs), CLI tests |

## What is not implemented

| Capability | State | Consequence |
|---|---|---|
| Core IR optimization and SSA passes | **Not implemented** | the current IR is deterministic and structural; no optimizer or SSA pass is claimed |
| External packages, registry and broad standard library | **Not implemented** | local modules and narrow stdlib fixtures only |
| HTTP, processes and unrestricted file effects | **Not implemented** | only bounded root-confined JSON reads are available |
| General tensor autograd, optimizers, MLPs and model serving | **Not implemented** | N3a is one-input scalar reverse mode; narrow broadcast/linear fixtures do not imply a general ML stack |
| GPU, distributed execution and serving | **Not implemented** | no accelerator or production service exists |
| Formatter, LSP, public installer and release artifacts | **Not implemented or verified** | local B2 scripts install an existing Windows binary; no supported release path exists |

## E3 gate fulfillment

Percentages below are engineering estimates of the design gate, not test coverage and not a version score.

| Gate | Estimate | Evidence | Remaining boundary |
|---|---:|---|---|
| E3.0 — grammar and AST preflight | 80% | [`parser.rs`](../../src/parser.rs#L224), precedence and reserved-syntax tests | Full grammar, closures, generic shape syntax and runtime meaning remain outside the subset |
| E3.1 — names and scopes | 70% | [`collect_declarations`](../../src/semantic.rs#L101), [`lookup`](../../src/semantic.rs#L1039) | Module visibility/import resolution is unavailable; `pub` is metadata plus signature validation, not a package export system |
| E3.2 — types and inference | 65% | [`type_of_expr`](../../src/semantic.rs#L652), [`type_from_expr`](../../src/semantic.rs#L1005) | No user generics or symbolic shape constraints |
| E3.3 — records, enums and fallibility | 75% | [`check_require`](../../src/semantic.rs#L371), [`type_of_call`](../../src/semantic.rs#L845), Result/Option tests | General error conversion and runtime resource behavior remain outside E3/E4 |
| E3.4 — exhaustive `match` | 70% | [`check_match`](../../src/semantic.rs#L409), [`required_coverage`](../../src/semantic.rs#L632), match tests | Structural/named-field patterns and ranges remain outside; the validated constructor subset executes |
| E3.5 — semantic CLI proof | 70% | [`check_path`](../../src/main.rs#L83), structured diagnostics and CLI integration tests | Property/fuzz campaign, golden corpus and Linux/macOS execution evidence remain absent |

## E4 execution evidence

E4 is **experimental implemented** for the validated E3 subset. `seloryn run`
reads UTF-8 source, stops on any lexical, parse or semantic diagnostic, then
executes `fn main()` through the deterministic lowered IR interpreter. The runtime covers
`Bool`, `I64`, `F64`, `Text`, `Unit`, `List`, nominal records, closed enums,
`Option`, `Result`, `if`, `match`, `require`, calls, recursion, return,
`??`, postfix `?`, list indexing, list iteration, mutable `var` assignment, list
concatenation and the named-function `|>` pipeline. Runtime failures carry
stable `SEL-R***` codes, source spans and a deterministic function stack. Call
depth, loop size and list size are bounded to keep user programs from
overflowing the host runtime. Large values are rendered with a deterministic
truncation marker; storage and computation remain bounded by their separate
runtime limits.

Evidence is [`src/ir.rs`](../../src/ir.rs) and [`src/runtime.rs`](../../src/runtime.rs) plus the CLI tests in
[`tests/bootstrap_cli.rs`](../../tests/bootstrap_cli.rs). The interpreter does
not perform filesystem, network, process, module or GPU effects. The N1 bridge
adds owned CPU buffers and numeric kernels without adding effects or a `print`
API. The IR is structural and deterministic; optimization, effects and broader runtime gates remain future work.

## N1 CPU tensor evidence

N1 is experimentally implemented for owned contiguous F32/I64 CPU tensors. The
runtime checks rank (maximum 32), nonnegative dimensions, checked element and
byte products, a 64 MiB per-tensor and process-wide resident-buffer budget
(not a total-process-memory limit), row-major strides, reshape counts, same-dtype
arithmetic, right-aligned
broadcasting, finite F32 results, checked I64 arithmetic, total and axis
reductions and rank-2 F32 matrix multiplication. The language bridge is covered
by positive and negative CLI tests, including CRLF, Unicode paths, static dtype
rejection, bad shapes, incompatible matmul, invalid axes, per-tensor and global
budget overflow. No GPU, autograd, training, tensor indexing, symbolic shape or
performance claim follows from N1.

## N2 CPU numeric evidence

N2 is **experimental implemented** for the bounded pure CPU subset. It adds
rank-2 shape/stride transpose views for both supported dtypes, F32 `relu`, `exp` and
positive-domain `log`, stable axis `softmax` and `logsumexp`, plus F32
`tensor_sum_to_shape` for reducing broadcasted results to a compatible operand
shape. The runtime rejects empty selected reduction axes, invalid log domains,
incompatible target shapes and non-finite results with `SEL-R210`/`SEL-R211`
where applicable. [`tests/n2_probe.py`](../../tests/n2_probe.py) compares real
CLI output against an independent Python standard-library reference and repeats
negative diagnostics.

N2 does not provide a general tape, optimizer, arbitrary tensor views, symbolic
shapes, GPU or effects. N3a adds only the scalar reverse-mode gate described in
[`docs/compute/autograd.md`](../compute/autograd.md); it does not differentiate
tensors or build a reusable tensor computation graph. The current N3/N4/CKPT
fixtures are narrow, explicitly tested contracts and do not claim a general
training framework. A rank-0 F32 reduction can be converted explicitly with
`tensor_item_f32`; non-scalar shapes and wrong dtypes fail with a structured
diagnostic. The language version remains `0.00`.

## Gaps versus the design review

E3 is therefore **not fully complete**. The checker proves a meaningful front-end subset, but E3.3, E3.4 and E3.5 still have the boundaries listed above. In particular:

- A successful `check` means syntax and supported static rules passed; `run` adds the separately tested pure E4 execution step.
- General training, model-serving and GPU text in the repository describes design targets, not available capabilities.
- Examples using broad effects, named record patterns or future libraries remain design concepts unless a page explicitly says otherwise.
- E4, N1, N2, F3/F4 and the narrow F5/N3a/N3/N4/CKPT/B1 fixtures have bounded experimental results; broader tensor autograd remains future work.

The implementation policy is staged: a feature becomes available only after source behavior, negative tests and platform evidence exist. The language version remains `0.00`; no release or earned gain is claimed by this status page.
