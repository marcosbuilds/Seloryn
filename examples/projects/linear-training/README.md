# Linear training fixture

This project exercises the narrow `training.train_linear` contract against
the included JSON dataset and holdout set:

```text
seloryn run examples/projects/linear-training
```

The result reports `loss_decreased`, a bounded holdout error and the explicit
seed. To inspect measured progress without changing the result, use
`seloryn run examples/projects/linear-training --log=human` (or `--log=json`)
to stream sampled events to `stderr`. It is an executable fixture for CPU
gradient updates, not a general dataset API, optimizer library, MLP or
model-serving runtime.
