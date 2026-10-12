# Grammar and syntax contract

> **E3 semantic subset — implemented and tested portions only.** The current `seloryn check` command validates UTF-8, layout, a bounded parser subset and selected semantic rules. It does not execute `.sely`, validate tensor types or provide the complete grammar below. The attached `seloryn_e3_design_review.md` is the design reference; the [public E3 checklist](../project/e3-checklist.md) records implementation evidence and gaps.

The complete grammar remains a design contract until each construct has runtime and negative-test coverage. Keywords, primitive types and standard public APIs are English. Case-sensitive symbols use `snake_case` for functions and variables, `PascalCase` for named types, `UPPER_SNAKE_CASE` for constants, and `lowercase.dotted` module paths. Identifiers follow Unicode XID rules; exported package names use ASCII for portability. Module namespaces are distinct from filesystem traversal.

## Compound statements

```seloryn
module services.health
from seloryn.http import Response

record Health:
    healthy: Bool

fn assess(health: Health) -> Text:
    match health:
        case Health(healthy=true):
            return "ready"
        case Health(healthy=false):
            return "unhealthy"
```

The example above is a **design concept**, not a current acceptance example: modules, imports and named record-field patterns are not available in the E3 checker.

### Simplified EBNF

```ebnf
program        = [module_decl], { top_decl | simple_stmt } ;
module_decl    = 'module', dotted_name, NEWLINE ;
top_decl       = fn_decl | record_decl | enum_decl | trait_decl | impl_decl | class_decl ;
fn_decl        = ['pub'], ['async'], 'fn', IDENT, '(', [params], ')', ['->', type], ':', suite ;
record_decl    = ['pub'], 'record', IDENT, ':', suite ;
enum_decl      = ['pub'], 'enum', IDENT, ':', suite ;
trait_decl     = ['pub'], 'trait', IDENT, ':', suite ;     (* reserved *)
impl_decl      = 'impl', type, ':', suite ;              (* reserved *)
class_decl     = ['pub'], 'class', IDENT, ':', suite ;   (* reserved *)
suite          = NEWLINE, INDENT, { statement }, DEDENT ;
statement      = compound | simple_stmt ;
compound       = if_stmt | while_stmt | for_stmt | loop_stmt | match_stmt | try_stmt ;
if_stmt        = 'if', expr, ':', suite, { 'elif', expr, ':', suite }, ['else', ':', suite] ;
for_stmt       = 'for', pattern, 'in', expr, ':', suite ;
while_stmt     = 'while', expr, ':', suite ;
loop_stmt      = 'loop', ':', suite ;
match_stmt     = 'match', expr, ':', NEWLINE, INDENT, { 'case', pattern, [ 'if', expr ], ':', suite }, DEDENT ;
try_stmt       = 'try', ':', suite, { 'catch', pattern, ':', suite }, ['finally', ':', suite] ;  (* reserved *)
simple_stmt    = binding | assignment | require_stmt | return_stmt | break_stmt | continue_stmt | expr_stmt | import_decl ;
binding        = ('let'|'var'|'const'), IDENT, [':', type], '=', expr, NEWLINE ;
require_stmt   = 'require', expr, 'else', expr, NEWLINE ;
return_stmt    = 'return', [expr], NEWLINE ;
import_decl    = 'import', dotted_name, [ 'as', IDENT ], NEWLINE
              | 'from', dotted_name, 'import', IDENT, { ',', IDENT }, NEWLINE ;
expr           = pipeline ;
pipeline       = coalesce, { '|>', callable } ;
coalesce       = logical_or, { '??', logical_or } ;
logical_or     = logical_and, { 'or', logical_and } ;
logical_and    = comparison, { 'and', comparison } ;
comparison     = range_expr, [ compare_op, range_expr ] ;
range_expr     = addition, [ ('..'|'..<'), addition ] ;
addition       = multiplication, { ('+'|'-'), multiplication } ;
multiplication = unary, { ('*'|'/'|'//'|'%'), unary } ;
unary          = { 'not'|'-'|'+' }, power ;
power          = postfix, ['**', unary] ;
postfix        = primary, { call | '.' IDENT | '?.' IDENT | '[' expr ']' | '?' } ;
```

This EBNF summarizes accepted design intent; production-ready conformance grammar must additionally define literal forms, tuples, map/sequence expressions, generics, spread rules, multiline continuation, numeric suffixes, operator associativity, method calls and exact lexical edge cases. Reserved syntax MUST fail explicitly before implementation, not silently parse incorrectly.

## Grammar feature readiness

| Feature | Current status | Evidence and limit |
|---|---|---|
| `record` declarations | Implemented parser and nominal field checks | [`parse_record`](../../src/parser.rs#L395), [`type_of_call`](../../src/semantic.rs#L845); named-field construction is not implemented |
| `enum` declarations and variants | Implemented parser and partial constructor checks | [`parse_enum`](../../src/parser.rs#L436), [`check_constructor_name`](../../src/semantic.rs#L528); no runtime values |
| `match` statements | Partial semantic checker | [`parse_match`](../../src/parser.rs#L650), [`check_match`](../../src/semantic.rs#L409); Bool/Option/Result/enum coverage is checked, but execution and structural patterns are absent |
| `Result` and postfix `?` | Minimal static checks | [`type_of_expr`](../../src/semantic.rs#L652); propagation is typed, not executed |
| `Option` and `??` | Minimal static checks | [`type_of_binary`](../../src/semantic.rs#L760); nullable `T?` and `?.` remain reserved |
| Modules and imports | Reserved/unavailable in E3 | `seloryn check` emits `SEL-M001`; no resolver or package graph exists |
| Tensor and symbolic shape types | Designed only | No tensor type is accepted by [`type_from_expr`](../../src/semantic.rs#L1005) |

Status labels mean: **implemented** is present and tested in the checker; **partial** has explicit gaps; **reserved** produces a diagnostic rather than silently working; **designed** has no current implementation.

### Implemented checker rules in the E3 subset

- Four **ASCII spaces** per block level; tabs as indentation produce a diagnostic. Blank/comment-only lines do not emit indentation transitions; grouping delimiters suppress layout tokens. CRLF and LF accepted.
- A `match` is statically checked for exhaustiveness for `Bool`, `Option`, `Result` and closed `enum` variants. A reachable `case` after a total match is diagnosed as unreachable. No match body executes.
- `require condition else error` is accepted only in a function typed as `Result[T,E]`; `condition` must be `Bool` and `error` must be assignable to `E`. The checker does not perform the early return because no interpreter exists.
- Postfix `?` is checked as Result propagation: `Result[T,E]` becomes `T` in a Result-returning function. No runtime propagation occurs; optional access is `?.` and nullability is `T?`, both reserved for later semantics.
- `|>` requires a function or callable with exactly one first argument consumed by the LHS; further arguments require an explicit lambda (proposed syntax `value |> fn(x) => ...` is **reserved** until closure syntax is chosen).
- `and` and `or` short-circuit and require Bool operands; `??` consumes nullable/Option values, not arbitrary falsey values.
- No implicit conversion between integer, Bool, Text, F32 or F64. No implicit broadcasting across incompatible tensor dimensions.
- `return` has function-context and return-type checks. `break`, `continue` and `try/catch` remain reserved and cannot replace `Result` in normal APIs.

### Operator precedence, high to low

1. Calls, indexing, member access, `?.`, Result `?`.
2. Power (`**`, right-associative), unary `+`, `-`, `not` subject to unambiguous parsing.
3. `*`, `/`, `//`, `%`.
4. `+`, `-`.
5. Half-open/full ranges (`..<`, `..`) non-associative.
6. Single comparisons / `in` / `is` (no implicit comparison chaining).
7. `and`, then `or`.
8. `??` null/Option coalescing, right-associative.
9. `|>` pipeline, left-associative.

`-2 ** 2` parses as `-(2 ** 2)`; this exception must be asserted in grammar conformance tests. Ambiguous chained comparisons are forbidden until formally specified.

### Meaningful constructs beyond branching

| Concern | First-class form |
|---|---|
| Validation | `require ... else ValidationError(...)` |
| Fallible workflow | `Result` + `?` + `match` |
| Data transformation | Iteration/typed collections and `|>` |
| Domain variants | `enum` with exhaustive pattern matching |
| Training | `Tensor`, `GradientTape`, `Optimizer` APIs |
| Resource safety | Scoped `using` (reserved pending lifetime rules) |
| Concurrency | Structured `async` and `await` (reserved pending runtime) |

All future declarations get syntax, semantics, diagnostics and a golden test suite before being marked accepted. The current implementation boundary and known gaps are tracked in the [E3 checklist](../project/e3-checklist.md).
