# Model training

Seloryn `0.04` contains executable CPU training profiles, including a
two-layer `F32` MLP with ReLU hidden activation, mean-squared error, SGD,
deterministic mini-batches, holdout evaluation and resumable checkpoints. This
is a verified preview of a small training workflow, not a general neural
network framework. Seloryn source can now own a bounded optimization loop
through multi-input reverse-mode gradients and a bounded integer range.

## Seloryn-authored optimization

The [`self-hosted-linear`](../../examples/projects/self-hosted-linear) project
keeps the model, loss, parameter updates and loop in `.sely` source:

```seloryn
let derivatives: List[Tensor[F32]] = gradients(objective, inputs, weight, bias, targets)
weight = weight - rate * derivatives[1]
bias = bias - rate * derivatives[2]
```

`from autograd import gradients` accepts a named function with multiple
`Tensor[F32]` parameters and returns one gradient tensor per input. `from data
import range` creates a bounded `List[I64]` for ordinary Seloryn `for` loops.
The native runtime still owns the bounded tensor kernels and tape traversal;
the training policy is no longer hidden inside `train_mlp`.

## Executable MLP profile

The `training.train_mlp` standard function accepts exactly eight arguments:

```seloryn
from training import train_mlp

fn main() -> Json:
    return train_mlp("data/train.json", "data/test.json", "checkpoints/model.json", seed=17, steps=240, batch_size=4, hidden_size=8, learning_rate=0.1)
```

The datasets are bounded JSON objects with uniform rows:

```json
{"inputs":[[0,0],[0,1]],"targets":[[0],[1]]}
```

The runtime validates dimensions, finite `F32` values, sample count, batch
size, hidden width, parameter budget, total sample-update budget and a
sample-update × parameter work budget before starting. Each update selects a
deterministic mini-batch from a seeded shuffle order, computes forward and
backward passes for both dense layers, averages MSE gradients over the batch,
and applies SGD. `steps` means optimizer updates, not a hidden wall-clock
budget.

Run the public proof:

```text
seloryn check examples/projects/mlp-training
seloryn run examples/projects/mlp-training --log=human
seloryn run examples/projects/mlp-training --log=json
```

The final JSON result reports `initial_loss`, `final_loss`,
`evaluation_loss`, `parameter_count`, the effective batch/model sizes,
`resumed_from` and the SHA-256 of the checkpoint bytes. The example is
deliberately small so it can run in local development and CI.

## Checkpoint and resume contract

The project must grant `write = ["checkpoints"]` in `seloryn.toml`. A
checkpoint uses the independent storage schema
`seloryn.training.checkpoint.v2` and contains:

- model weights and biases with their dimensions and `F32` values;
- SGD configuration and the completed update step;
- seed, deterministic sampler order, cursor, epoch and RNG state;
- SHA-256 hashes of the exact train and test JSON inputs.
- SHA-256 hashes of the canonical weights, sampler-state payload and complete
  training state (including the completed step).

Writes are flushed and synced to a temporary file before replacement. A
malformed checkpoint, dataset-hash mismatch, seed mismatch, shape mismatch or
hyperparameter mismatch fails with a diagnostic rather than silently
restarting. The checkpoint is intentionally local and bounded; it is not a
portable distributed format.

## Hardware evidence boundary

`seloryn devices --format=json` enumerates adapters exposed by the compiled
platform backend. `seloryn gpu-probe --format=json` selects a high-performance
physical non-fallback adapter, submits a bounded WGSL compute shader,
synchronizes the device with a finite 30-second wait and verifies the readback
against an exact reference. The independent
[`gpu_probe.py`](../../tests/gpu_probe.py) accepts a capability-aware skip on a
host with no physical non-CPU adapter; when an adapter is reported, the shader result
must pass with maximum absolute error at most `1e-6`.

This is deliberately narrower than GPU training. The current `Tensor`,
reverse-mode autodiff tape and `train_mlp` contracts remain CPU-only. GPU
training still requires device-resident tensor storage, backend parity,
determinism policy, memory limits and an independent model-training proof.

## Streaming events

`--log=human` writes readable events to `stderr`; `--log=json` writes one JSON
object per line. Each event is flushed immediately, while the program result
stays on `stdout`. The MLP lifecycle is:

```text
training.start -> training.step* -> training.complete
```

Start identifies the datasets, checkpoint, seed, device, dtype, optimizer,
batch, hidden width and parameter count. Step events report monotonic update
number, epoch, train loss and evaluation loss. Completion reports final
metrics and checkpoint hash. JSON output is deterministic apart from the
external process manifest; timestamps are intentionally not embedded in the
canonical stream.

## What remains outside this profile

The training implementation is CPU-only, single-process and bounded. It does
not claim GPU tensor execution, distributed collectives, AdamW, gradient accumulation, mixed
precision, model serving, a general `Dataset`/`Module` API or transformer/LLM
training. Those capabilities require separate numerical, reproducibility and
platform evidence. The previous `training.train_linear` fixture remains
available for compatibility, but `train_mlp` is the current end-to-end proof
of actual model training.

## Independent verification

[`tests/mlp_training_probe.py`](../../tests/mlp_training_probe.py) executes the
release binary as a black box and checks loss decrease, holdout loss, same-seed
reproducibility, different-seed divergence, split-run equivalence after
resume, corrupted-checkpoint rejection and receipt of `training.start` while
the process is still alive. This is stronger evidence than inspecting a
training loop or a post-process log alone.

[`tests/self_hosted_probe.py`](../../tests/self_hosted_probe.py) independently
executes the Seloryn-authored loop and checks that the loss falls below
`1e-3`, with learned weight and bias within `0.01` of `2` and `1`.
