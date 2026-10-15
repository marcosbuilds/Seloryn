# Model training

Seloryn `0.02` contains one executable CPU training profile: a bounded,
two-layer `F32` MLP with ReLU hidden activation, mean-squared error, SGD,
deterministic mini-batches, holdout evaluation and resumable checkpoints. This
is a verified preview of a small training workflow, not a general neural
network framework.

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
size, hidden width and parameter budget before starting. Each update selects a
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
`seloryn.training.checkpoint.v1` and contains:

- model weights and biases with their dimensions and `F32` values;
- SGD configuration and the completed update step;
- seed, deterministic sampler order, cursor, epoch and RNG state;
- SHA-256 hashes of the exact train and test JSON inputs.

Writes are flushed and synced to a temporary file before replacement. A
malformed checkpoint, dataset-hash mismatch, seed mismatch, shape mismatch or
hyperparameter mismatch fails with a diagnostic rather than silently
restarting. The checkpoint is intentionally local and bounded; it is not a
portable distributed format.

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

The implementation is CPU-only, single-process and bounded. It does not claim
GPU execution, distributed collectives, AdamW, gradient accumulation, mixed
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
