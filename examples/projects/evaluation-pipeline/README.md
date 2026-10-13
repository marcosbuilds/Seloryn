# Evaluation pipeline

This is the first multi-file project example that exercises the full
implemented boundary in one flow: a root-confined JSON read, local modules,
typed records and enums, `Result` validation, iteration, `match`, and CPU
tensor reductions plus softmax.

```text
seloryn check examples/projects/evaluation-pipeline
seloryn run examples/projects/evaluation-pipeline
seloryn build examples/projects/evaluation-pipeline --profile release --output build/evaluation-pipeline
seloryn exec build/evaluation-pipeline
```

The checked result is deterministic: one order is accepted, two are rejected,
and the accepted total is `120.0`. The labels are
`A-100:accepted`, `B-200:below minimum`, and `C-300:order disabled`. The
release probe asserts these business outcomes for both a direct project run
and a bundled `exec` run.

The mean is intentionally a rank-0 `Tensor[F32]`, because tensor reductions
remain tensors. Use `tensor_item_f32` when a rank-0 F32 result must cross back
into scalar control or reporting code. The project is an experimental example,
not a package, registry, HTTP service or general training framework.
