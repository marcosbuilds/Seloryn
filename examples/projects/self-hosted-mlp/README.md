# Self-hosted MLP

This is the end-to-end proof that the training policy can live in Seloryn
source. The program defines its own three-unit ReLU network, reads its own
training and holdout datasets, calls the tensor reverse-mode `gradients`
primitive, performs every SGD update in `.sely`, evaluates before and after
training, writes a versioned JSON checkpoint atomically, reloads that
checkpoint, and performs inference with the reloaded weights.

Rust supplies bounded tensor kernels and reverse-mode autodiff. It does not
provide this model's training function, optimizer loop, dataset split,
checkpoint schema or inference policy.

Run with a human stream:

```text
seloryn check examples/projects/self-hosted-mlp
seloryn run examples/projects/self-hosted-mlp --format=json --log=human
```

For machine verification, use `--log=json`; each `training.progress` event is
flushed while the `.sely` loop is still running, and the final result remains
on stdout. The generated `checkpoints/custom-model.json` is intentionally
ignored as a runtime artifact; the program verifies its schema and evaluates
the reloaded model before returning.
