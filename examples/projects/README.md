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
- `seloryn init --template=source-mlp` creates a fresh copy of this style of
  source-owned MLP project, including bounded data, declared checkpoint
  capabilities and a reload check. It is the shortest path from an empty
  directory to an editable training workflow.
- `seloryn init --template=trainable-mlp` creates a contract-first modular
  train/eval/infer starter. Its `src/app` tree is only an example: the
  manifest entry and imported module graph define the project, so developers
  can choose their own names and directories.
- `self-hosted-language-model/` defines modular contextual language-model
  workflows in `.sely`, including a one-hot linear baseline and a selectable
  embedding/ReLU MLP path with tokenizer, indexed dataset, cross-entropy objective,
  SGD, evaluation, hash-validated checkpoint reload, JSON streaming and inference.
- `auditable-local-inference/` defines a contract-first four-feature ReLU MLP
  in `.sely`, including dataset/model/run validation, source-owned training,
  holdout evaluation, hash-bound checkpoint reload, separate inference and a
  structured audit event.
- `mlp-inference/` consumes a hash-validated MLP checkpoint with deterministic
  batch predictions; copy a checkpoint from `mlp-training/` before execution.
- `evaluation-pipeline/` combines local modules, JSON validation, typed
  decisions and bounded CPU tensor metrics in one executable project.
