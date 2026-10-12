# Tensors

**Current availability (`0.00`):** Seloryn has an experimental N1 CPU tensor
runtime. It allocates owned, bounded buffers for `Tensor[F32]` and
`Tensor[I64]`; it does not use a GPU, perform training or expose arbitrary
views.

## Verified language bridge

The E4 front end knows the following builtins. They are pure expressions: the
returned tensor is a fresh value and no file, network, process or device effect
is performed.

| Form | Behavior |
|---|---|
| `tensor_f32(values, shape)` | Converts a `List[F64]` explicitly into a finite F32 tensor |
| `tensor_i64(values, shape)` | Creates an I64 tensor from `List[I64]` |
| `tensor_zeros_f32(shape)` / `tensor_zeros_i64(shape)` | Creates a bounded zero tensor |
| `reshape(tensor, shape)` | Copies the contiguous value layout into an equal-size shape |
| `tensor_shape(tensor)` | Returns dimensions as `List[I64]` |
| `tensor_rank(tensor)` / `tensor_elements(tensor)` | Returns rank or element count as `I64` |
| `tensor_dtype(tensor)` | Returns `"F32"` or `"I64"` |
| `tensor_sum(tensor)` | Reduces all elements to a rank-0 tensor; I64 overflow is checked |
| `tensor_mean(f32_tensor)` | Reduces finite F32 values to a rank-0 tensor; empty means fail |
| `matmul(left, right)` | Computes rank-2 F32 matrix multiplication |
| `+`, `-`, `*`, `/` | Same-dtype tensor arithmetic with right-aligned broadcasting |

The runnable example is [`examples/tensor-matmul.sely`](../../examples/tensor-matmul.sely):

```seloryn
fn main() -> Tensor[F32]:
    let left: Tensor[F32] = tensor_f32([1.0, 2.0, 3.0, 4.0], [2, 2])
    let right: Tensor[F32] = tensor_f32([5.0, 6.0, 7.0, 8.0], [2, 2])
    return matmul(left, right)
```

`seloryn run examples/tensor-matmul.sely` prints:

```text
Tensor[F32, shape=[2, 2], values=[19.0, 22.0, 43.0, 50.0]]
```

## Safety boundary

Shapes are ordered nonnegative dimensions with rank at most 32. Rank zero has
one element; any zero dimension has zero elements. Shape products, byte counts,
reshape counts, integer arithmetic and F32 finite results are checked. The
default per-tensor allocation budget is 64 MiB. Broadcast dimensions must be
equal or one; zero and one are compatible, while other incompatible dimensions
produce `SEL-R204`.

N1 stores contiguous row-major strides, owns CPU memory and uses fresh outputs.
It rejects negative or overflowing shapes, wrong data lengths, dtype mismatches,
invalid axes, empty means, division by zero and non-finite F32 values with
structured runtime diagnostics (`SEL-R201` through `SEL-R208`). A lexical,
syntactic or semantic diagnostic still stops execution before any tensor is
created.

## Not available yet

There are no symbolic dimensions, arbitrary strided views, indexing, device
transfers, GPU backends, autograd, optimizers, checkpoints or training APIs.
N1 supplies a correctness-oriented CPU reference path; it makes no performance
or portability claim against established numerical runtimes.
