# Tensors

**Design target only:** Seloryn 0.00 has no tensor type, allocated numeric
buffer or tensor operation. A usable AI language needs a real numerical
execution engine. A `Tensor` must be a typed, shaped value backed by an
allocated buffer with documented lifetime, layout and device residency. A list
of numbers with operator overloads does **not** meet this contract.

### Proposed type model

```text
Tensor[DType, Shape]
DType = Bool | I8 | I16 | I32 | I64 | F16 | BF16 | F32 | F64 (staged)
Shape = tuple of positive / zero / symbolic dimensions
Device = Cpu | Accelerator(provider, index)
Layout = Contiguous | Strided(...) (staged)
```

Illustrative, unimplemented:

```seloryn
from seloryn.tensor import zeros, matmul

let weights: Tensor[F32, (784, 512)] = zeros((784, 512), dtype=F32)
let pixels: Tensor[F32, (Batch, 784)] = load_training_batch()?
let activations: Tensor[F32, (Batch, 512)] = matmul(pixels, weights)
```

### Semantic obligations

| Operation | Required semantics |
|---|---|
| Construction | Bound allocation sizes, checked shape product, alignment |
| Arithmetic | Explicit dtype promotion table, finite/non-finite rules |
| Broadcasting | Right-alignment + dimension compatibility; zero dimensions defined |
| Matrix multiply | Rank, inner dimension, batch broadcasting validated |
| Views | Shared buffer lifetime/alias restrictions; no use-after-free |
| Mutation | Defined alias policy and in-place gradient restrictions |
| Reduction | Axis, keepdims, numerical accumulation precision, deterministic options |
| Indexing | Bounds, slicing, negative indices and contiguous-copy rules |
| Serialization | Data type, shape, byte order, checksum, metadata and limits |
| Device movement | Explicit transfer, sync/error policy, ownership |

### Mathematical completeness gates

CPU baseline must pass reference tests for elementwise operations, reductions, broadcasting, matrix multiplication and gradient checks before GPU claims. Compare numerical results within dtype-specific tolerances; exact bit equality only for operations specified as deterministic. Measure memory peak, throughput, cold start and transfer overhead separately.

### Differentiation boundary

Not all tensor operations are differentiable. Gradients require declared derivative rules and must never silently return zeros for unsupported operations. [Autograd](autograd.md) defines this contract.
