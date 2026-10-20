# Seloryn VS Code extension 0.10.0 / language 0.10

This local extension candidate follows the `N12-SELF-HOSTED-CUSTOM-MLP`
language line and records the editor-specific
`EXT-N13-SELF-HOSTED-CUSTOM-MLP` gain. Completions expose the source-owned
custom MLP workflow, JSON persistence/hash helpers, flushed progress events,
the `tensor` module and the canonical `from training import gpu` alias while
retaining the compatibility name.

The headless extension test checks the new completion surface, package
metadata and LSP/controller fixtures. A real VS Code host rendering check and
GitHub publication remain pending. The VSIX has not been published in this
workspace turn.
