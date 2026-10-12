# E3 semantic checker checklist

This page is the public evidence map for the E3 gate. The attached
`seloryn_e3_design_review.md` is the design reference for what E3 should cover;
the source and tests below are the authority for what Seloryn 0.00 actually
does. Percentages are engineering estimates of gate fulfillment, not code
coverage, quality scores or language-version credits.

## Status vocabulary

- **Implemented:** behavior exists in the checker and has direct tests.
- **Partial:** a bounded behavior exists, with explicit documented gaps.
- **Reserved:** syntax is recognized and rejected; no semantics are claimed.
- **Designed:** described by the design contract, with no current implementation.

## E3.0 — grammar and AST preflight

**Requirement from the review:** fix precedence, preserve `pub`, clarify
accepted versus reserved constructs, distinguish declarations from expressions
and retain recoverable source spans.

**Status: 80% — partial.** The parser preserves public flags, has explicit AST
nodes for `match`, patterns and postfix `?`, and tests precedence and reserved
syntax. The parser still implements only a bounded subset of the public EBNF.

**Evidence:** [`parse`](../../src/parser.rs#L224), [`parse_match`](../../src/parser.rs#L650),
[`Pattern`](../../src/parser.rs#L123), and parser tests for precedence and
reserved syntax.

**Gaps:** indexing, closures, full generic syntax, named-field patterns,
assignment and the complete declaration grammar are not accepted. `T?`, `?.`,
imports and future control-flow forms remain reserved or unavailable.

## E3.1 — name and scope resolution

**Requirement from the review:** root/module/function/block scopes, duplicate
declarations, local shadowing, use-before-definition, undefined symbols and
visibility.

**Status: 70% — partial.** Function and block scopes are checked; duplicate
names fail within one scope, nested locals may shadow, and unresolved names
produce stable diagnostics. Imports and module exports are not resolved.

**Evidence:** [`collect_declarations`](../../src/semantic.rs#L101),
[`check_function`](../../src/semantic.rs#L262), [`lookup`](../../src/semantic.rs#L1039),
and semantic tests `resolves_names_and_checks_scalar_types` and
`rejects_duplicates_bad_calls_unimplemented_imports_and_unreachable_cases`.

**Gaps:** `pub` is retained in the AST and public signatures are checked, but
there is no package graph, export table, alias resolution or module visibility
runtime. Imports intentionally return `SEL-M001`.

## E3.2 — strict types and local inference

**Requirement from the review:** strict scalar types, deterministic literals,
conditions, calls, returns, assignment rules and explicit public signatures.

**Status: 65% — partial.** The checker supports `Bool`, `I64`, `F64`, `Text`,
`Unit`, local inference, function signatures, call arity/types, return types,
Boolean conditions and strict arithmetic/comparisons.

**Evidence:** [`type_of_expr`](../../src/semantic.rs#L652),
[`type_of_binary`](../../src/semantic.rs#L760),
[`type_from_expr`](../../src/semantic.rs#L1005), and
`resolves_names_and_checks_scalar_types` in [`semantic.rs`](../../src/semantic.rs#L1194).

**Gaps:** assignment syntax is not implemented, so `var` reassignment rules
are not proven. Overflow behavior, user-defined generics, tensor types,
symbolic shapes and a wider numeric promotion matrix remain outside E3.

## E3.3 — records, enums and fallibility

**Requirement from the review:** nominal records, closed enum identity,
`Result[T,E]`, `Ok`, `Err`, postfix `?` and `require` with compatible errors.

**Status: 75% — partial.** Records and enum declarations are nominally typed;
constructors, field access, `Result`, `Option`, `require`, `Ok`, `Err`, `Some`,
`None` and Result propagation receive static checks.

**Evidence:** [`check_require`](../../src/semantic.rs#L371),
[`check_constructor_name`](../../src/semantic.rs#L528),
[`type_of_call`](../../src/semantic.rs#L845), and tests
`checks_result_require_propagation_and_public_signatures` plus
`checks_records_options_results_and_member_types`.

**Gaps:** no runtime value representation or early-return execution exists.
There is no general error conversion, serialization, resource behavior or
interpreter context in which `Result` can run.

## E3.4 — exhaustive `match`

**Requirement from the review:** typed patterns, binder scopes, guards,
exhaustiveness for Bool, closed enums, Option and Result, plus unreachable-case
detection.

**Status: 70% — partial.** The checker validates Bool, Option, Result and enum
coverage; guarded cases do not satisfy total coverage; binders receive local
types; a case after total coverage is rejected.

**Evidence:** [`check_match`](../../src/semantic.rs#L409),
[`check_pattern`](../../src/semantic.rs#L454),
[`required_coverage`](../../src/semantic.rs#L632), and
`checks_match_exhaustiveness_and_guards` in [`semantic.rs`](../../src/semantic.rs#L1224).

**Gaps:** structural and named-field destructuring, numeric ranges, arbitrary
predicate patterns and runtime branch execution are not implemented. The
current `match` is a static statement check, not an executable feature.

## E3.5 — semantic CLI proof

**Requirement from the review:** `seloryn check` must validate syntax and
semantics, return deterministic success/failure codes, preserve spans and
survive malformed user input.

**Status: 55% — partial.** `check` reads UTF-8, runs parser and semantic
diagnostics, prints `checked` only after both pass, and returns documented
nonzero codes for invalid input. The CLI integration file contains 10 tests,
including CRLF, invalid UTF-8, tabs, Unicode paths, reserved syntax and
semantic failures; the semantic unit suite also contains a malformed-source
corpus test.

**Evidence:** [`check_path`](../../src/main.rs#L82),
[`bootstrap_cli.rs`](../../tests/bootstrap_cli.rs#L61), and
`malformed_source_corpus_does_not_panic` in [`semantic.rs`](../../src/semantic.rs#L1259).

**Gaps:** JSON diagnostics, golden diagnostic snapshots, property/fuzz testing,
Linux/macOS shell evidence and a complete conformance corpus are not present.
The Windows shell checks performed for this gate do not prove other platforms.

## Gate conclusion

E3 is **experimentally useful but not fully complete**. It is appropriate for
evaluating Seloryn's front-end design and finding static errors in the
implemented subset. The E3 checker itself is not an interpreter, compiler,
tensor framework or training system. E4 now supplies an experimental pure
scalar/list interpreter with structured runtime failures; N1 still requires an
independent tensor runtime and numeric correctness gate.
