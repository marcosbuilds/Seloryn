# Type system

Seloryn proposes sound static checking for public module boundaries, with local inference where information is sufficient. The type checker must distinguish values from computation effects and host resources.

| Kind | Proposed contract | Avoid |
|---|---|---|
| Scalars | `Bool`, `I8/I16/I32/I64`, `U8/U16/U32/U64`, `F16/BF16/F32/F64`, `Text`, `Bytes` | Silent numeric/text conversion |
| Text vs bytes | UTF-8 logical text; explicit encoding/decoding | Broken boundary assumptions |
| Containers | `List[T]`, `Map[K,V]`, `Set[T]`, `Tuple[...]` | Untyped heterogeneous mutation |
| Outcomes | `Result[T,E]`, `Option[T]`, `T?` | Silent error dropping or unknown nulls |
| Domain | `record`, `enum`; planned `trait`, `class` | Unsafe implicit inheritance |
| Tensor | `Tensor[DType, (Dim1, ...)]` with device metadata | Incorrect broadcasting or dtype promotion |
| Resources | File/socket/model/context handles with deterministic close | Leaks and cross-thread unsafety |
| Callable | `fn(A,B) -> Result[C,E]` | Hidden uncaught errors |

### Type inference

```seloryn
let batch_size = 32               # inferred integer with defined default width
let learning_rate: F32 = 0.0003
var steps: I64 = 0
```

Numeric literal default types and overflow semantics must be explicitly frozen before preview. Proposed baseline: integer literals default `I64` when unannotated; floating literals default `F64`; explicit `F32` context must be range-checked. Integer overflow in safe mode yields deterministic failure; unchecked wrap requires explicit API. No float equality surprises are disguised as tolerant comparisons.

### Generics and types

`Result[User, DbError]` and `Tensor[F32, (Batch, 768)]` share generic brackets but `Tensor` shape tuple is a compile-time symbolic shape language, **not** a runtime value list. Symbolic `Batch` unifies; conflicting concrete dimensions fail before execution when statically knowable, otherwise at the operation boundary with a structured ShapeError.

### Null / Result distinction

`T?` encodes nullable values and `?.` safe member access; `Option[T]` is explicit absence with `Some` and `None`. `Result[T,E]` encodes fallibility. There is no truthiness of `None`, `0` or an empty list. `??` has explicit null/Option rules and never conceals `Err`.

### Extension discipline

Traits, classes, lifetimes and tensor ownership must not be exposed publicly until dispatch, generic resolution, object safety, aliasing and ABI are specified. Shape checking must include dtype, memory layout, device placement and named/symbolic dimensions separately.
