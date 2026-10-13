# CPU tensor examples

These standalone programs use the bounded, pure `Tensor[F32]` CPU bridge.
They do not use a GPU, filesystem, network, process or autodiff runtime.

| File | Demonstrates |
|---|---|
| `tensor-matmul.sely` | rank-2 matrix multiplication |
| `tensor-numeric.sely` | numerically stable softmax |
| `tensor-reduce.sely` | broadcasting followed by reduction |
| `linear-forward.sely` | fixed-weight linear forward pass |

The fixed weights are intentional: these examples are forward computation,
not model training.
