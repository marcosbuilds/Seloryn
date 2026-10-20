# Project examples

Project examples have a `seloryn.toml`, a source root and external data. They
are the public examples for module resolution and bounded standard-library
capabilities; they are not packages published to a registry.

- `json-config/` loads `data/config.json` through `from json import ...` and
  formats the result through a local `app.format` module.
- `checkpoint-validation/` validates a bounded checkpoint JSON schema only.
- `linear-training/` runs the narrow `training.train_linear` fixture against
  small JSON datasets. It proves a bounded fixture contract, not a general
  optimizer or model framework.
- `mlp-training/` trains a real bounded two-layer CPU MLP with mini-batches,
  evaluation, checkpoint/resume and flushed terminal events.
- `self-hosted-mlp/` defines a custom ReLU network and owns its dataset load,
  reverse-mode update loop, holdout evaluation, checkpoint persistence,
  checkpoint reload and inference in `.sely`; Rust supplies only tensor kernels
  and the gradient primitive.
- `self-hosted-language-model/` defines a modular 14-token bigram language model
  in `.sely`, including tokenizer, dataset, cross-entropy objective, SGD,
  evaluation, hash-validated checkpoint reload, JSON streaming and inference.
- `mlp-inference/` consumes a hash-validated MLP checkpoint with deterministic
  batch predictions; copy a checkpoint from `mlp-training/` before execution.
- `evaluation-pipeline/` combines local modules, JSON validation, typed
  decisions and bounded CPU tensor metrics in one executable project.
