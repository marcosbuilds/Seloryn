# Grammar and syntax contract

> **Implemented and tested portions only.** The current `seloryn check` command validates UTF-8, layout, a bounded parser subset and selected semantic rules. `seloryn run` additionally executes the E4 scalar subset and the bounded N1/N2 CPU tensor subset; it does not provide the complete grammar below.

The complete grammar remains a design contract until each construct has runtime and negative-test coverage. The bounded E4 subset is executable through `seloryn run`; constructs outside that subset remain reserved or unavailable. Keywords, primitive types and standard public APIs are English. Case-sensitive symbols use `snake_case` for functions and variables, `PascalCase` for named types, `UPPER_SNAKE_CASE` for constants, and `lowercase.dotted` module paths. Identifiers follow Unicode XID rules; exported package names use ASCII for portability. Module namespaces are distinct from filesystem traversal.

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
| `record` declarations | Implemented parser, nominal field checks and E4 construction/member access | [`src/parser.rs`](../../src/parser.rs), [`src/semantic.rs`](../../src/semantic.rs); named-field construction is not implemented |
| `enum` declarations and variants | Implemented parser, constructor checks and E4 values/matching | [`src/parser.rs`](../../src/parser.rs), [`src/semantic.rs`](../../src/semantic.rs); structural patterns remain outside |
| `match` statements | Partial checker plus E4 execution | [`src/parser.rs`](../../src/parser.rs), [`src/semantic.rs`](../../src/semantic.rs), [`src/runtime.rs`](../../src/runtime.rs); Bool/Option/Result/enum coverage and constructor cases execute, but structural patterns are absent |
| `Result` and postfix `?` | Static checks plus E4 propagation | [`src/semantic.rs`](../../src/semantic.rs), runtime tests; `?` returns a compatible `Err` from the current function |
| `Option` and `??` | Static checks plus E4 values/fallback | [`src/semantic.rs`](../../src/semantic.rs), runtime tests; nullable `T?` and `?.` remain reserved |
| Local modules and standard imports | Experimental local-project subset | `seloryn.toml` resolves local modules and the bounded `json`/other standard fixtures; external packages, registry and lockfiles remain unavailable |
| `Tensor[F32]` and `Tensor[I64]` | N1/N2 experimental | Runtime shapes and CPU kernels are checked by [`src/tensor`](../../src/tensor) and CLI tests; symbolic shape parameters remain unavailable |
| Multiple tensor gradients and bounded `range` | Verified preview | `from autograd import gradients` returns one gradient per Tensor[F32] input; `from data import range` supports bounded `for` loops |
| List indexing, tensor element reads, `for` iteration and `var` assignment | E4 experimental | `List[T]` indexing, typed read-only tensor coordinates, bounded iteration, mutable bindings and list concatenation execute; slicing and tensor mutation remain unavailable |

Status labels mean: **implemented** is present and tested in the checker; **partial** has explicit gaps; **reserved** produces a diagnostic rather than silently working; **designed** has no current implementation.

### Implemented checker rules in the E3 subset

- Four **ASCII spaces** per block level; tabs as indentation produce a diagnostic. Blank/comment-only lines do not emit indentation transitions; grouping delimiters suppress layout tokens. CRLF and LF accepted.
- A `match` is statically checked for exhaustiveness for `Bool`, `Option`, `Result` and closed `enum` variants. A reachable `case` after a total match is diagnosed as unreachable. The validated constructor/binder subset executes in `run`.
- `require condition else error` is accepted only in a function typed as `Result[T,E]`; `condition` must be `Bool` and `error` must be assignable to `E`. In `run`, a false condition returns `Err(error)` from that function.
- Postfix `?` is checked and executed as Result propagation: `Result[T,E]` becomes `T` in a Result-returning function, while `Err(E)` returns from the current function. Optional access is `?.` and nullability is `T?`, both reserved for later semantics.
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
| Training | `Tensor` and bounded `GradientTape` behavior are implemented; general `Optimizer` APIs are design-only and not accepted |
| Resource safety | Scoped `using` (reserved pending lifetime rules) |
| Concurrency | Structured `async` and `await` (reserved pending runtime) |

All future declarations get syntax, semantics, diagnostics and a golden test suite before being marked accepted. The current implementation boundary and known gaps are tracked in the [status page](../project/status.md).
