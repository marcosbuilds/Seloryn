# Linear training fixture

This project exercises the narrow `training.train_linear` contract against
the included JSON dataset and holdout set:

```text
seloryn run examples/projects/linear-training
```

The result reports `loss_decreased`, a holdout error that must remain at or
below the enforced `0.05` cap, and the explicit seed. A run that exceeds the
cap fails with a structured runtime diagnostic and does not report successful
completion. To inspect measured progress without changing the result, use
`seloryn run examples/projects/linear-training --log=human` (or `--log=json`)
to stream sampled events to `stderr`. It is an executable fixture for CPU
gradient updates, not a general dataset API, optimizer library, MLP or
model-serving runtime.
