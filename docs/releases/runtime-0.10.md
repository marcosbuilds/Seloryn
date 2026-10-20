# Seloryn runtime 0.10.0 / language 0.10

This local verified-preview candidate records the `N12-SELF-HOSTED-CUSTOM-MLP`
language gain. A complete custom ReLU network is authored in `.sely`: it loads
its own bounded datasets, defines its forward pass and mean-squared-error
objective, requests multi-input gradients, applies SGD, evaluates held-out
data, writes a hash-validated JSON checkpoint, reloads it and performs
inference from the reloaded parameters.

The source-owned `training.progress` events are flushed into the JSON2
hash-chained receipt. The independent
`tests/self_hosted_mlp_probe.py` checks static absence of the native training
functions, loss decrease, holdout quality, exact reload parity, checkpoint
schema/hash, 160 progress events and independent receipt verification. An
independent subagent repeated the black-box probe and the full Rust test suite.

This remains a verified-preview CPU, bounded, single-process workflow. The
runtime still supplies tensor kernels and reverse-mode tape; it does not claim
a public `Module`/`Parameter` framework, GPU MLP training, distributed
execution, a general checkpoint registry, stable ABI or a published artifact.
The package has not been published in this workspace turn.
