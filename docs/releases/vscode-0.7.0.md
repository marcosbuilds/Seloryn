# Seloryn Language for VS Code 0.7.0

This verified-preview editor release is on the same `0.7.x` compatibility
line as language `0.07` and runtime `0.7.0`. It records an editor-specific
identity and packaging gain; it does not claim a new language capability.

Included:

- the canonical public `assets/brand/seloryn-symbol.svg` as the `.sely` file-icon
  source and the extension mark render;
- a release verifier rule that prevents the extension package from advancing
  beyond the language compatibility line;
- the existing `.sely` association, syntax highlighting, completions, CLI
  diagnostics and bounded training/hardware commands.

The SVG remains the identity source of truth. The PNG in the extension is only
the marketplace-compatible render required by VS Code packaging.

## Verification

- `python scripts/release/verify-version.py`
- `python tests/versioning_probe.py`
- `npm test --prefix packages/vscode`
- `npm run test:cli --prefix packages/vscode`
- `npm run package --prefix packages/vscode`
