# Tensors

**Current availability (`0.03`):** Seloryn has an experimental N1/N2 CPU
tensor runtime. It allocates owned, bounded buffers for `Tensor[F32]` and
`Tensor[I64]`; N2 adds a small pure numeric closure needed before autograd. It
does not use a GPU, perform training or expose arbitrary views or slicing.

## Verified language bridge

The E4 front end knows the following builtins. They are pure expressions: the
returned tensor is a fresh value and no file, network, process or device effect
is performed.

| Form | Behavior |
|---|---|
| `tensor_f32(values, shape)` | Converts a `List[F64]` explicitly into a finite F32 tensor |
| `tensor_i64(values, shape)` | Creates an I64 tensor from `List[I64]` |
| `tensor_zeros_f32(shape)` / `tensor_zeros_i64(shape)` | Creates a bounded zero tensor |
| `reshape(tensor, shape)` | Preserves the contiguous value layout under an equal-size shape |
| `tensor_shape(tensor)` | Returns dimensions as `List[I64]` |
| `tensor_rank(tensor)` / `tensor_elements(tensor)` | Returns rank or element count as `I64` |
| `tensor_dtype(tensor)` | Returns `"F32"` or `"I64"` |
| `tensor_to_f32(i64_tensor)` | Materializes `Tensor[F32]` only when every I64 value is exactly representable; otherwise fails |
| `tensor_to_i64(f32_tensor)` | Materializes `Tensor[I64]` only for finite integral F32 values in range; otherwise fails |
| `tensor_item_f32(tensor)` | Extracts one rank-0 `Tensor[F32]` as an `F32`; other shapes or dtypes fail |
| `tensor_get_f32(tensor, indices)` / `tensor_get_i64(tensor, indices)` | Reads one element with a full `List[I64]` coordinate; dtype, rank and bounds are checked |
| `tensor_sum(tensor[, axis])` | Reduces all elements or one nonnegative `I64` axis; I64 overflow is checked |
| `tensor_mean(f32_tensor[, axis])` | Reduces all elements or one axis of finite F32 values; empty means fail |
| `matmul(left, right)` | Computes rank-2 F32 matrix multiplication |
| `transpose(tensor)` | Rank-2 shape/stride view, preserving dtype and sharing owned storage |
| `tensor_relu(f32_tensor)` / `tensor_exp(f32_tensor)` / `tensor_log(f32_tensor)` | Pure F32 unary operations; `log` requires positive inputs and non-finite results fail |
| `tensor_softmax(f32_tensor, axis)` | Numerically stable F32 softmax over one explicit axis |
| `tensor_logsumexp(f32_tensor, axis)` | Numerically stable F32 log-sum-exp reduction over one explicit axis |
| `tensor_sum_to_shape(f32_tensor, shape)` | Reduces a broadcasted F32 result back to a compatible operand shape |
| `+`, `-`, `*`, `/` | Same-dtype tensor arithmetic with right-aligned broadcasting |

The runnable example is [`examples/tensors/tensor-matmul.sely`](../../examples/tensors/tensor-matmul.sely):

```seloryn
fn main() -> Tensor[F32]:
    let left: Tensor[F32] = tensor_f32([1.0, 2.0, 3.0, 4.0], [2, 2])
    let right: Tensor[F32] = tensor_f32([5.0, 6.0, 7.0, 8.0], [2, 2])
    return matmul(left, right)
```

`seloryn run examples/tensors/tensor-matmul.sely` prints:

```text
Tensor[F32, shape=[2, 2], values=[19.0, 22.0, 43.0, 50.0]]
```

Read-only indexing is also executable:

```seloryn
fn main() -> F32:
    let values: Tensor[F32] = tensor_f32([1.0, 2.0, 3.0, 4.0], [2, 2])
    return tensor_get_f32(values, [1, 0])
```

`seloryn run examples/tensors/tensor-index.sely` prints `3.0`. Indexing is
element access only; slicing, mutation and arbitrary view construction remain
outside the current contract.

## Safety boundary

Shapes are ordered nonnegative dimensions with rank at most 32. Rank zero has
one element; any zero dimension has zero elements. Shape products, byte counts,
reshape counts, integer arithmetic and F32 finite results are checked. The
default per-tensor allocation budget is 64 MiB, and the process-wide resident
tensor-buffer budget is also 64 MiB. This is a resident-buffer limit, not a
total-process-memory limit for source lists, conversion scratch or metadata.
Broadcast dimensions must be equal or one;
zero and one are compatible, while other incompatible dimensions produce
`SEL-R204`. An axis is a zero-based `I64` and must be less than the tensor rank;
invalid axes and tensor coordinates produce `SEL-R205`. `tensor_log` rejects non-positive values with
`SEL-R210`; `tensor_softmax` and `tensor_logsumexp` reject an empty selected
axis with `SEL-R211`. `transpose` is a bounded view with permuted shape and
strides; consumers that require contiguous storage materialize a checked copy.
Other N2 computational outputs are fresh owned CPU buffers.

N1 stores contiguous row-major strides, owns CPU memory and uses fresh outputs.
N2 keeps that representation and adds stable reference kernels; it does not add
an execution graph or gradient metadata. The runtime rejects negative or
overflowing shapes, wrong data lengths, dtype mismatches, invalid axes, empty
means/reductions, invalid domains, division by zero and non-finite F32 values
with structured runtime diagnostics (`SEL-R201` through `SEL-R212`). A lexical,
syntactic or semantic diagnostic still stops execution before any tensor is
created. The CLI renders at most 4096 tensor values and appends a deterministic
`... (N more)` marker for larger results; this limits presentation memory and
does not change the tensor contents used by computation.

## Not available yet

There are no symbolic dimensions, arbitrary strided views, device transfers or
GPU backends. Tensor reverse-mode `backward` and multi-input `gradients` are now available for bounded
Tensor[F32] input and a rank-zero objective; it is documented separately in
[`autograd.md`](autograd.md). Optimizers and a general training API remain
unavailable; the separate bounded CPU MLP profile documents the current
training/checkpoint contract.
N1/N2 supply a correctness-oriented CPU reference path; they make no performance
or portability claim against established numerical runtimes. The runnable N2
smoke example is [`examples/tensors/tensor-numeric.sely`](../../examples/tensors/tensor-numeric.sely).
