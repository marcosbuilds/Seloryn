# Seloryn Language for VS Code 0.3.0

This verified-preview release aligns the editor package with Seloryn language
`0.03` and runtime package `0.3.0`.

## Included

- completions for `autograd.gradients`, `data.range` and the self-hosted
  tensor-optimization workflow;
- a real snippet that keeps gradient calculation and SGD updates in `.sely`;
- the Seloryn icon for the extension and optional `.sely` file-icon theme;
- the same CLI diagnostics and terminal training command from 0.2.0.

The runtime remains CPU-only and bounded. This release does not claim GPU,
distributed execution, a package registry, a full LSP or a stable installer.
