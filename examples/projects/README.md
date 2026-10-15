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
- `evaluation-pipeline/` combines local modules, JSON validation, typed
  decisions and bounded CPU tensor metrics in one executable project.
