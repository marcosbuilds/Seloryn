Runtime checkpoints are generated here by the source-owned MLP example and
are intentionally not tracked as model artifacts.

The directory is part of the manifest's explicit read/write capability. The
black-box proof copies this project to an isolated workspace, trains the model,
reloads the generated checkpoint and validates its JSON2 receipt.
