# Model training

Seloryn `0.11` contains executable CPU training profiles, including a
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

## Custom network authored in `.sely`

The [`self-hosted-mlp`](../../examples/projects/self-hosted-mlp) project is the
stronger end-to-end proof. Its source defines a custom ReLU network and loss,
loads its own numeric dataset, calls `gradients` with model parameters, applies
every SGD update, evaluates a holdout split, constructs a versioned JSON
checkpoint, writes it through a declared capability, reloads it and performs
inference from the reloaded weights. It does not import or call `train_mlp`.

The small runtime primitives used by that project are explicit:
`from tensor import values` exports finite F32 weights as a typed list,
`from json import object, from_f64_list, sha256, write_file` constructs and
persists JSON values atomically and lets the source validate its weight payload,
while `from training import progress` emits flushed
`training.progress` events. `--log=json2` wraps those source-owned events in
the same independently verifiable hash chain used by native profiles.

The proof is a CPU, bounded, single-process workflow. Rust still supplies the
reference tensor kernels and reverse-mode tape; it does not supply the custom
model's training function, optimizer loop, checkpoint schema or inference
policy.

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

## Batch inference from a checkpoint

The verified MLP checkpoint can be consumed by the bounded
`training.predict_mlp` function:

```seloryn
from training import predict_mlp

fn main() -> Json:
    return predict_mlp("checkpoints/model.json", "data/inputs.json")
```

The input file is a JSON object containing a non-empty `inputs` array of
uniform numeric rows. The checkpoint's hash-protected weights, state and model
dimensions are validated before any prediction. The result includes the
checkpoint/input hashes, checkpoint step, dimensions and predictions. This is
batch inference for the closed CPU MLP profile; it is not an HTTP server,
general model loader or serving ABI.

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

This is deliberately narrower than general GPU training. The current `Tensor`,
reverse-mode autodiff tape and `train_mlp` contracts remain CPU-only. The
`from training import gpu` profile (with the older `train_linear_gpu` import
still accepted) is a separate scalar F32 proof with device-resident parameters,
WGSL SGD and one final readback; GPU tensor,
autodiff and MLP training still require their own backend-parity,
determinism, memory and model-training gates.

## Streaming events

`--log=human` writes readable events to `stderr`; `--log=json` writes the
backward-compatible schema-v1 object per line. `--log=json2` writes the same
training lifecycle inside a versioned, hash-chained NDJSON run envelope. Each
event is flushed immediately, while the program result stays on `stdout`.
The envelope also carries a deterministic identity for the loaded program and
manifest, exact dataset bytes, the checkpoint before training and the canonical
`{stdout,value}` result. Structured dataset/checkpoint paths are replaced by
non-reversible path identifiers in JSON2. The helper
[`replay-receipt.py`](../../scripts/streaming/replay-receipt.py) re-executes a
successful bounded receipt in an isolated copy and compares these identities
and the final result. Resume replay requires the original pre-run checkpoint
snapshot to still be supplied; a post-run checkpoint is rejected instead of
silently treated as the input.
The MLP lifecycle in both JSON formats is:

```text
training.start -> training.step* -> training.complete
```

Start identifies the datasets, checkpoint, seed, device, dtype, optimizer,
batch, hidden width and parameter count. Step events report monotonic update
number, epoch, train loss and evaluation loss. Completion reports final
metrics and checkpoint hash. JSON output is deterministic apart from the
external process manifest; timestamps are intentionally not embedded in the
canonical stream.

### Streaming v2 evidence contract

The v2 record has the fields `schema`, `run`, `seq`, `kind`, `phase`, `data`,
`prev` and `hash`. The first record is `run.start`; the source training events
follow; the final record is `run.end`. `hash` is a SHA-256 digest of the
canonical record without its own `hash` field, and `prev` links it to the
previous record. The run envelope covers the CPU `train_linear`/`train_mlp`
profiles and the closed GPU `train_linear_gpu` profile. It is an integrity
receipt with an execution-plan identity, not a public-key signature, async
backpressure protocol or general execution telemetry.
For keyed authentication, `--log=json2-signed` requires a non-empty
`SELORYN_RECEIPT_KEY`. It adds an HMAC-SHA256 tag to every record; the
independent verifier checks it with `--hmac-key`. This authenticates the stream
to parties that already possess the secret; it is not publisher authentication,
non-repudiation or an Ed25519 signature.
Only a profile that reaches `run.start` receives this envelope. Failures during
manifest loading, static checking, capability resolution or dataset/checkpoint
preflight remain ordinary CLI diagnostics because there is not yet a complete
execution identity from which to construct a truthful receipt. A `training.step`
is progress evidence; checkpoint persistence can still fail afterward and is
represented by an error `run.end`.

The independent verifier can run without the Seloryn binary:

```text
python scripts/streaming/verify-event-chain.py events.ndjson
```

[`tests/streaming_v2_probe.py`](../../tests/streaming_v2_probe.py) generates a
real receipt from a clean project, confirms that `--log=json` keeps the same
stdout, and rejects altered, removed, reordered, truncated and duplicate-key
records. The verifier preserves numeric spellings during canonicalization so
Rust values such as `0.02000000` are checked byte-for-byte. Asynchronous
backpressure, public-key signatures, secret-content redaction and
cross-platform numerical identity remain separate gates. JSON2 path redaction
is structural and limited to the documented path fields; it is not a guarantee
that arbitrary program data or error text contains no secrets.

## What remains outside this profile

The MLP and general tensor/autodiff implementation remains CPU-only,
single-process and bounded. The new `train_linear_gpu` profile is a separate
scalar F32 proof: parameters stay in a GPU storage buffer, the WGSL shader
performs the SGD update, and one final readback returns the result. It does
not claim GPU tensor execution, GPU MLP/autodiff, distributed collectives,
AdamW, gradient accumulation, mixed precision, model serving, a general
`Dataset`/`Module` API or transformer/LLM training. Those capabilities require
separate numerical, reproducibility and platform evidence. The previous
`training.train_linear` fixture remains available for compatibility, and
`train_mlp` remains the CPU end-to-end proof of the native bounded profile;
`self-hosted-mlp` is the source-owned custom-model proof.

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
