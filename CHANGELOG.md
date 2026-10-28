# Changelog

All entries describe implemented behavior and point to the evidence that
accepted it. Historical tags and releases are immutable; this file does not
rewrite their metadata. An entry marked **candidate** exists locally and is
not a promise that GitHub publication already happened.

## [Unreleased]

- **Candidate:** the dependency-free Python bridge invokes an installed
  Seloryn binary through the existing JSON/stdio `{stdout,value}` contract,
  with typed diagnostic errors, timeout/input/output bounds and a copied
  external-project proof. The bridge is process-per-request and does not claim
  HTTP serving, retries, descendant fencing or a stable ABI.
- **Candidate:** JSON strings now decode raw UTF-8 code points instead of
  treating each non-ASCII byte as a separate character; the Python bridge
  proof covers Portuguese Unicode input end to end.
- **Candidate:** the local job worker now attaches each child to a POSIX
  process group or Windows Job Object with kill-on-close semantics. A native
  descendant probe proves timeout cleanup of the tree after fence installation;
  checkpoint resume and supervisor-crash recovery remain outside the claim.
- **Candidate:** `seloryn run --log=json2` now emits a bounded
  `run.start` → `preflight.error` → `run.end` receipt for source-loading,
  manifest-loading and static-check failures. The receipt preserves the
  stable diagnostic code and a message fingerprint without exposing paths or
  message text; dataset/checkpoint failures remain outside this preflight
  contract.
- **Candidate:** release staging now groups a runtime archive and VSIX on one
  language compatibility line, writes `release-manifest.json` and external
  `SHA256SUMS`, and refuses a mismatched version base.
- **Candidate:** Windows and POSIX installers validate the binary's canonical
  language/runtime version before installation; the Windows lifecycle probe
  covers install, upgrade, rollback and uninstall.
- **Candidate:** structured diagnostics identify the `seloryn.analysis.v1`
  schema, language version and runtime package so editor integrations can
  reject stale or incompatible output.
- **Candidate:** the VS Code extension starts the parser-backed LSP for
  unsaved diagnostics, completion and definition by default, while retaining
  the saved-file CLI bridge as an explicit fallback.
- **Candidate:** the Windows installer verifies supplied `seloryn.provenance.v1`
  metadata, including the runtime version and binary hash, before copying a
  binary into the install root.
- **Candidate:** `seloryn job` now includes a bounded local worker for validated
  `seloryn-run.v1` sources. It snapshots the validated source, claims and
  fences a lease, applies timeout/cancellation cleanup, bounds stdout/stderr
  evidence and commits an atomic result receipt. `job retry` archives the
  previous attempt, clears the active result, requeues the same canonical spec
  and increments the next claimed attempt. The worker now fences descendants
  through a POSIX process group or Windows Job Object; checkpoint resume,
  supervisor-crash recovery and remote execution remain outside the slice.

## [0.9.4] — local runtime patch, 2026-10-16

- `.selyapp` bundles now carry an explicit `seloryn.bundle.v1` format version;
  `build` writes `seloryn-build.json` with language/runtime metadata, byte
  length and SHA-256, and `exec <build-directory>` rejects tampered metadata.
- The LSP now tracks document versions, ignores stale or ranged full-sync
  changes atomically, clears diagnostics on close and validates framed headers.
- The language remains `0.09` and the extension remains `0.9.0`.

## [0.9.1] — local runtime patch, 2026-10-16

- Added read-only `seloryn doctor` project diagnostics with stable JSON output.
- Added transactional `seloryn init` minimal and tabular-MLP templates with
  no-overwrite protection and generated-project validation.
- Kept the language at `0.09` and the extension at `0.9.0`; this is an explicit
  runtime-only gain under the stricter version ledger.

## [0.9.2] — local runtime patch, 2026-10-16

- Runtime staging now embeds and verifies `seloryn.provenance.v1` with source
  tree, toolchain, lockfile, binary and artifact hashes.
- The candidate remains local `verified-preview`; provenance is not a public
  signature and does not make the ABI stable.

## [0.9.3] — local runtime patch, 2026-10-16

- Windows installation now rejects unsupported provenance, version mismatch and
  binary-hash mismatch before changing the installation root.
- The language remains `0.09` and the extension remains `0.9.0`; this is an
  installer-integrity gain on the existing compatibility line.

## [0.09] — local candidate, 2026-10-16

- `training.predict_mlp` loads a hash-validated bounded CPU MLP checkpoint and
  produces deterministic batch predictions with checkpoint/input identity and
  JSON2 lifecycle events.
- The coordinated language/runtime/extension versions are `0.09`, `0.9.0`
  and `0.9.0`; public publication remains pending.

## [0.9.0] — local candidate, 2026-10-16

- Extension gain `EXT-N12-MLP-BATCH-INFERENCE` adds completion and a snippet for
  the verified batch-inference workflow.

## [0.8.1] — local candidate, 2026-10-16

- Extension-only gain `EXT-N11-LSP-CLIENT` on the language `0.08` line.
- The package is `0.8.1`; runtime `0.8.0` and language `0.08` remain unchanged.

## [0.08] — local candidate, 2026-10-16

- Bounded JSON number arrays can be converted to typed `List[I64]` or
  `List[F64]` values and consumed by normal Seloryn iteration.
- The candidate runtime, extension and language versions are coordinated as
  `0.8.0`, `0.8.0` and `0.08`; public publication remains pending.

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
  their earlier compatibility lines. The current local candidate is tracked
  in `packages/vscode/package.json`; historical tags are not candidates for the
  current `0.12` compatibility line.
