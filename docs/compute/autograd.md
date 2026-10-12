# Automatic differentiation

The first ML-ready gradient engine uses **reverse-mode automatic differentiation** over an explicit computation graph, with leaf parameters, intermediate values, saved tensors and one derivative rule per differentiable primitive.

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

### Required derivation tests

1. Scalar and vector finite-difference checks (bounded numeric tolerance).
2. Broadcast-reduction gradient shape inversion, including summed broadcast axes.
3. Matrix-multiplication transposed gradient and batched gradients.
4. Zero-sized axes, NaN/Inf, accumulation dtype and mixed precision correctness.
5. Graph reuse rules, in-place operation invalidation, detached tensors, no-grad scopes.
6. Gradient checkpoint recomputation with identical semantics.
7. Backward error propagation without leaked buffers.

Forward-mode/JVP and higher-order differentiation can be added only after the reverse-mode contract holds. GPU derivatives must match CPU semantics within published tolerances and supported dtype coverage.
