# Self-hosted linear training

This project is the first public proof that Seloryn can express a reusable
model, objective, gradient call, parameter update and training loop in `.sely`.
The Rust runtime supplies bounded CPU tensor kernels and reverse-mode autodiff;
the model and SGD orchestration are Seloryn source.

Run it with:

```text
seloryn run examples/projects/self-hosted-linear
```

The model learns `y = 2x + 1` from four tensor rows. `app/model.sely` defines
the objective and `app/training.sely` performs repeated `gradients(...)` calls,
SGD updates and evaluation. It is intentionally small: it does not claim a
general optimizer registry, GPU execution, serialization or production model
serving.
