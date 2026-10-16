# Seloryn runtime 0.9.0 / language 0.09

This local candidate adds `training.predict_mlp`, a bounded CPU batch-inference
operation for `seloryn.training.checkpoint.v2` files produced by
`training.train_mlp`.

Before prediction, the runtime validates the checkpoint schema, optimizer
contract, dimensions, finite parameters, weights hash, sampler-state hash and
complete-state hash. Input rows are bounded, numeric and dimension-checked.
The result carries checkpoint/input hashes, checkpoint step, dimensions and
predictions; JSON/JSON2 lifecycle events make the operation observable.

The independent `tests/mlp_inference_probe.py` covers deterministic repeated
predictions, an external numerical reference, receipt verification, corrupted
weights and invalid dimensions. This remains a verified-preview CPU profile,
not a general model format, GPU inference backend, HTTP server, registry or
stable ABI release.

The runtime package has not been published in this workspace turn.
