# MLP training

This project runs a bounded, real CPU training loop for a two-layer `F32`
multilayer perceptron on a small two-dimensional affine regression problem. It
loads disjoint training and holdout inputs and targets,
shuffles deterministic mini-batches, performs ReLU forward/backward passes,
updates weights with SGD, evaluates the holdout dataset and writes a versioned
checkpoint containing model weights, tamper-detecting sampler state, dataset
hashes and the optimizer contract. The holdout rows are not copied from the
training rows, so the reported evaluation loss is a real generalization check.

The checkpoint is written only because `seloryn.toml` grants the `checkpoints`
write capability. A second invocation with the same data, seed and
hyperparameters resumes from the saved step instead of restarting.

Run it with live terminal events:

```text
seloryn run examples/projects/mlp-training --log=human
seloryn run examples/projects/mlp-training --log=json
```

The current implementation is CPU-only, bounded and single-process. The
repository also provides `seloryn devices` and `seloryn gpu-probe` for hardware
evidence, but they do not move this MLP to a GPU. The project does not claim
distributed training, AdamW, mixed precision or a general model registry.
