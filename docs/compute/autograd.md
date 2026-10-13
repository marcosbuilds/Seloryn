# Automatic differentiation

**General reverse-mode autodiff remains a design target:** Seloryn 0.00 has no
automatic-differentiation engine or executable computation graph. The runtime
does expose a narrow, executable `autograd.gradcheck` finite-difference
contract for a scalar function. N2 supplies the tested CPU forward
primitives needed by that work—2D transpose, F32 unary operations, stable
softmax/logsumexp and broadcast sum-to-shape—but it does not record gradients.
The first ML-ready gradient engine is
intended to use **reverse-mode automatic differentiation** over an explicit
computation graph, with leaf parameters, intermediate values, saved tensors and
one derivative rule per differentiable primitive.

![Gradient dataflow](../../assets/training-flow.png)

### Target semantics

```seloryn
using tape = GradientTape():
    let output = model.forward(batch.inputs)
    let loss = mean_squared_error(output, batch.targets)
let gradients = tape.backward(loss)?
optimizer.apply(gradients)?
```

`using` is reserved grammar pending an ownership/cancellation design. This example describes the intended API only; it is not executable.

### Executable scalar check

The current finite-difference fixture evaluates the supplied function at two
nearby points; it does not build a tape or perform reverse-mode backpropagation:

```seloryn
from autograd import gradcheck

fn f(x: F32) -> F32:
    return x * x + x

fn main() -> F32:
    return gradcheck(f, 4.0)
```

`seloryn run` returns `9.0` for this source. The implementation and test are
in [`src/runtime.rs`](../../src/runtime.rs); tensor `backward` remains a
narrow broadcast fixture and is not a general gradient engine.

### Required derivation tests

1. Scalar and vector finite-difference checks (bounded numeric tolerance).
2. Broadcast-reduction gradient shape inversion, including summed broadcast axes.
3. Matrix-multiplication transposed gradient and batched gradients.
4. Zero-sized axes, NaN/Inf, accumulation dtype and mixed precision correctness.
5. Graph reuse rules, in-place operation invalidation, detached tensors, no-grad scopes.
6. Gradient checkpoint recomputation with identical semantics.
7. Backward error propagation without leaked buffers.

Forward-mode/JVP and higher-order differentiation can be added only after the reverse-mode contract holds. GPU derivatives must match CPU semantics within published tolerances and supported dtype coverage.
