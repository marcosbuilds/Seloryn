# Editor integration

Seloryn exposes a bounded parser-backed LSP through
`seloryn lsp --stdio`. Its protocol lifecycle and diagnostics are tested in
[`tests/lsp_probe.py`](../../tests/lsp_probe.py), independently of any editor
package.

The verified-preview VS Code client is distributed as a release asset, not as
source in this repository. Install the exact
[`seloryn-language-0.12.0.vsix`](https://github.com/marcosbuilds/Seloryn/releases/tag/vscode-v0.12.0)
asset and verify the SHA-256 from the release's `SHA256SUMS` file.

The package is compatible with the language `0.12` line and remains preview
software. Indexed/incremental analysis, rename, Marketplace publication and a
stable editor API are not claimed. The extension's executable contents remain
inspectable after download; only its source build tree is outside the public
language repository.
