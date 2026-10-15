# Seloryn Language for VS Code 0.2.0

This verified-preview release aligns the editor package with Seloryn language
`0.02` and runtime package `0.2.0`.

It adds real completion and snippet entries for `training.train_mlp`, expands
the built-in syntax grammar, keeps `.sely` files associated with the Seloryn
language icon theme sourced from the project SVG, and points users to the
bounded CPU MLP training workflow.

The extension remains a thin editor integration: saved-file diagnostics use
the Seloryn CLI, and training is run in a terminal with the CLI's flushed
human/JSON event stream. It does not claim a full LSP, GPU tooling, a hosted
model provider or an interactive training panel.
