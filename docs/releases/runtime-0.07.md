# Seloryn runtime 0.7.0 / language 0.07

This verified-preview release records two concrete gains beyond 0.06.

## Identity, replay and receipt authentication

- JSON2 `run.start` records program and manifest identity, exact train/test
  dataset hashes and the checkpoint hash present before training.
- JSON2 `run.end` records the canonical `{stdout,value}` result hash and final
  MLP checkpoint hash.
- `scripts/streaming/replay-receipt.py` copies a successful bounded project to a
  temporary directory, re-runs it and compares identity, result and checkpoint
  output. Resume replay requires the original pre-run checkpoint snapshot; the
  helper fails closed when only a post-run checkpoint is available.
- Structured dataset/checkpoint paths are replaced by SHA-256 path identifiers
  in JSON2. This is field-level path redaction, not a guarantee that arbitrary
  application text contains no secrets.
- `--log=json2-signed` adds HMAC-SHA256 tags using `SELORYN_RECEIPT_KEY`.
  This is keyed authentication, not public-key signing or non-repudiation.

## GPU linear training

- `from training import train_linear_gpu` is executable in a `.sely` project.
- The bounded scalar F32 SGD profile keeps parameters in a GPU storage buffer,
  performs the optimization loop in WGSL and performs one final readback.
- It reports adapter/backend, dispatch count, readback count, loss and learned
  parameters. A physical non-fallback adapter is required; CPU fallback is not
  silent or implicit.
- This does not make general tensors, autodiff or MLP training GPU-capable.

Evidence is in [`streaming_v2_probe.py`](../../tests/streaming_v2_probe.py),
[`gpu_training_probe.py`](../../tests/gpu_training_probe.py) and the public
[`gpu-linear-training`](../../examples/projects/gpu-linear-training) project.
