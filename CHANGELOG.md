# Changelog

All entries describe implemented behavior and point to the evidence that
accepted it. Historical tags and releases are immutable; this file does not
rewrite their metadata. An entry marked **candidate** exists locally and is
not a promise that GitHub publication already happened.

## [Unreleased]

- **Candidate:** release staging now groups a runtime archive and VSIX on one
  language compatibility line, writes `release-manifest.json` and external
  `SHA256SUMS`, and refuses a mismatched version base.
- **Candidate:** Windows and POSIX installers validate the binary's canonical
  language/runtime version before installation; the Windows lifecycle probe
  covers install, upgrade, rollback and uninstall.
- **Candidate:** structured diagnostics identify the `seloryn.analysis.v1`
  schema, language version and runtime package so editor integrations can
  reject stale or incompatible output.

## [0.07] — 2026-10-16

- Identity-bearing JSON2 receipts with replay verification, structured path
  redaction and optional HMAC-SHA256 authentication.
- Explicit device-resident scalar F32 GPU linear training with one final
  readback and no silent CPU fallback.
- Public canonical Seloryn symbol used by the VS Code package and `.sely`
  file-icon theme.

## [0.06] — 2026-10-15

- Hash-chained JSON2 training receipts, cancellation-aware event streaming,
  checkpoint cleanup and stricter training limits.

## [0.05] — 2026-10-14

- Strict TOML project manifests and rejection of future language versions
  before source loading.

## Historical editor releases

- `vscode-v0.4.0`, `vscode-v0.3.0`, `vscode-v0.2.0` and `vscode-v0.1.0`
  remain preserved as prerelease history. They are not relabeled to hide
  their earlier compatibility lines; the current local candidate is
  `vscode-v0.7.0` and still needs explicit publication authorization.
