# Scalar autograd examples

These examples exercise the narrow N3a scalar reverse-mode gate. They use an
explicit one-variable function and return a real derivative from the Seloryn
runtime.

```text
seloryn check examples/autograd/scalar-gradient.sely
seloryn run examples/autograd/scalar-gradient.sely
```

The current gate supports scalar arithmetic with F32/F64 annotations (the
runtime scalar representation is F64), helper calls, simple conditions and
constant-exponent powers. `gradcheck` independently compares
the reverse tape with a finite-difference estimate. Tensor `backward` remains
a separate broadcast fixture; this directory does not claim tensor autograd,
optimizers or general training.
