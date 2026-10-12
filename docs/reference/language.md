# Language philosophy

Seloryn is an **AI-first language design for development, integration and
serving**, while remaining expression-centered rather than expression-only.
Branching is a basic primitive, not its governing design. The language aims to
make model, data and service boundaries explicit using named types, checked
values, pattern matching, declarative validation and composable pipelines.

The proposed advantage is fewer invalid states, fewer adaptations between
development and production and better diagnostics across a complete AI
workflow. That advantage is a hypothesis: later gates must compare equivalent
workloads and revise the architecture if the evidence does not support it.

### Principles

1. **Read the program like a contract:** named inputs, output types, explicit fallibility and visible side effects.
2. **Make invalid states hard to represent:** non-null types, algebraic variants, exhaustive matches and shape constraints.
3. **Organize AI systems by capabilities:** model/data/service modules, records, traits, packages and narrow exports; no gigantic script mandate.
4. **Prefer describing data flow:** iterators, `match`, `require` for typed validation, `?` for Result propagation, `|>` for pure transformations.
5. **Protect numerical integrity:** dtypes, dimensions, devices, precision, reproducibility and resource ownership are explicit.
6. **Handle failure deliberately:** errors are structured values with diagnostic codes; no silent fallback.
7. **Make advanced hardware optional:** portable CPU contracts first; accelerated backends are capabilities, not assumptions.

### Language surface sketch

```seloryn
record Order:
    total: Decimal
    currency: Text

fn validate(order: Order) -> Result[Order, ValidationError]:
    require order.total > 0 else ValidationError("total must be positive")
    require order.currency in ["BRL", "USD"] else ValidationError("unsupported currency")
    return Ok(order)

fn display(order: Order) -> Text:
    return order.currency |> upper |> trim
```

The pipe operator is intentionally constrained to a callable RHS with one required incoming argument and no ambiguous implicit side effects. `Decimal` is a planned standard numeric type with specified precision semantics. The example is illustrative until runtime support is implemented.

`class`, `trait` and `impl` are deliberately staged features; a design example is not a promise that they work today.
