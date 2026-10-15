# MLP training

This project runs a bounded, real CPU training loop for a two-layer `F32`
multilayer perceptron on XOR. It loads two-dimensional inputs and targets,
shuffles deterministic mini-batches, performs ReLU forward/backward passes,
updates weights with SGD, evaluates the holdout dataset and writes a versioned
checkpoint containing model weights, sampler state, dataset hashes and the
optimizer contract.

The checkpoint is written only because `seloryn.toml` grants the `checkpoints`
write capability. A second invocation with the same data, seed and
hyperparameters resumes from the saved step instead of restarting.

Run it with live terminal events:

```text
seloryn run examples/projects/mlp-training --log=human
seloryn run examples/projects/mlp-training --log=json
```

The current implementation is CPU-only, bounded and single-process. It does
not claim GPU, distributed training, AdamW, mixed precision or a general model
registry.
