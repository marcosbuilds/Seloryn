# Tensor walkthrough — design example

**Non-executable target syntax.** This illustrates a typed machine-learning flow; it is not test evidence.

```seloryn
from seloryn.tensor import tensor, matmul, softmax

let inputs: Tensor[F32, (2, 3)] = tensor([
    [1.0, 0.0, 1.0],
    [0.0, 1.0, 1.0]
], dtype=F32)
let weights: Tensor[F32, (3, 2)] = tensor([
    [0.3, 0.7],
    [0.5, 0.2],
    [0.1, 0.6]
], dtype=F32)

let logits: Tensor[F32, (2, 2)] = matmul(inputs, weights)
let probs = softmax(logits, axis=1)
print(probs)
```

Expected structural invariants: `matmul` result shape `(2,2)`, row-wise probability sum approximately `1`, no implicit incompatible casts, no silent CPU→GPU transfer. Exact printed values and dtype tolerances must be specified when the runtime exists.
