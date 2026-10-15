# Seloryn Language for VS Code 0.4.0

This verified-preview release aligns the editor package with language/runtime
package `0.4.0`. It adds direct terminal commands for the real CPU training
workflow, hardware adapter inventory and the bounded GPU compute/readback
probe. The `.sely` grammar, native completions, diagnostics and Seloryn SVG
file-icon theme remain included.

The GPU command is evidence of adapter selection and one synchronized compute
dispatch; it does not claim GPU tensor training or distributed execution.
Training remains a bounded CPU contract with deterministic checkpoint/resume
validation.

## Verification

- `npm test --prefix tooling/vscode`
- `npm run test:cli --prefix tooling/vscode`
- `npm run package --prefix tooling/vscode`
- `python tests/mlp_training_probe.py`
- `python tests/gpu_probe.py`

The VSIX is a verified preview and is not a stable runtime installer.
