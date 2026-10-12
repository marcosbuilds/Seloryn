# Type system

> **Checker/design boundary.** Seloryn's E3 checker provides an experimental
> soundness baseline for local scopes and public function boundaries, with
> local inference where information is sufficient. It validates `Bool`, `I64`,
> `F64`, `Text` and `Unit`, plus a limited set of containers and outcome types.
> It also accepts the bounded N1 tensor types described below; symbolic shapes,
> ownership parameters and device metadata remain future work.

| Kind | Proposed contract | Avoid |
|---|---|---|
| Scalars | E3: `Bool`, `I64`, `F64`, `Text`, `Unit`; other widths remain proposed | Silent numeric/text conversion |
| Text vs bytes | UTF-8 logical text; explicit encoding/decoding | Broken boundary assumptions |
| Containers | `List[T]`, `Map[K,V]`, `Set[T]`, `Tuple[...]` | Untyped heterogeneous mutation |
| Outcomes | `Result[T,E]`, `Option[T]`, `T?` | Silent error dropping or unknown nulls |
| Domain | `record`, `enum`; planned `trait`, `class` | Unsafe implicit inheritance |
| Tensor | **N1 experimental:** `Tensor[F32]` and `Tensor[I64]` with runtime shapes | Incorrect broadcasting or dtype promotion |
| Resources | File/socket/model/context handles with deterministic close | Leaks and cross-thread unsafety |
| Callable | `fn(A,B) -> Result[C,E]` | Hidden uncaught errors |

### Type inference

```seloryn
let batch_size = 32               # inferred integer with defined default width
let learning_rate: F32 = 0.0003
var steps: I64 = 0
```

E3 defaults integer literals to `I64` and floating literals to `F64`, with no
implicit Bool/Text/numeric coercion. Overflow behavior and additional scalar
widths remain outside this gate.

### Generics and types

`Result[User, DbError]` is part of the checker subset when its component types are supported. `Tensor[F32]` and `Tensor[I64]` are accepted as bounded N1 types; shapes are supplied to runtime builtins and are checked at execution. Symbolic shape parameters, device metadata and implicit dtype conversion are not accepted. See the [tensor page](../compute/tensors.md) and the [current status](../project/status.md).

### Null / Result distinction

`Option[T]` is E3's explicit absence model with `Some` and `None`.
`Result[T,E]` encodes fallibility. Nullable `T?` and `?.` remain reserved
until their coercion and member-access rules are frozen. There is no
truthiness of `None`, `0` or an empty list. E3's `??` accepts an `Option` on
the left and never conceals `Err`.

### Extension discipline

Traits, classes, lifetimes and tensor ownership must not be exposed publicly until dispatch, generic resolution, object safety, aliasing and ABI are specified. Shape checking must include dtype, memory layout, device placement and named/symbolic dimensions separately.
