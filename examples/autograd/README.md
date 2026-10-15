# Autograd examples

These examples exercise the N3a scalar reverse-mode gate and the N3b bounded
CPU Tensor[F32] reverse-mode gate. They return real derivatives from the
Seloryn runtime; they do not claim GPU, distributed execution or a general
training framework.

```text
seloryn check examples/autograd/scalar-gradient.sely
seloryn run examples/autograd/scalar-gradient.sely
```

The scalar gate supports scalar arithmetic with F32/F64 annotations (the
runtime scalar representation is F64), helper calls, simple conditions and
constant-exponent powers. `gradcheck` independently compares the reverse tape
with a finite-difference estimate.

`tensor-gradient.sely` differentiates a matrix-and-mean objective through
`matmul` and elementwise multiplication. Tensor `backward` builds a real
bounded reverse tape for one Tensor[F32] input and a rank-zero objective; it is
not a fixed shape fixture. Optimizers and general training remain future work.
