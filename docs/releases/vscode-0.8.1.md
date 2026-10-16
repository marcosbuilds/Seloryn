# Seloryn Language for VS Code 0.8.1

This extension-only candidate remains compatible with language `0.08` and
runtime package `0.8.0`. It starts the bounded parser-backed `seloryn lsp
--stdio` server by default and sends full-text document updates, enabling
unsaved diagnostics, completion and definition queries in VS Code.

The setting `seloryn.lsp.enabled=false` keeps the saved-file CLI diagnostic
bridge available for constrained environments. Indexed project analysis,
incremental parsing, rename and a stable runtime ABI remain outside this
candidate.

The VSIX is a local verified-preview candidate. It has not been published as a
Marketplace or GitHub release in this workspace turn.
