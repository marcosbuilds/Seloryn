# Earned capability versions

Language versions use **two numeric components** `M.mm`, where `mm` is always two digits, e.g. `0.00`, `0.01`, `0.12`, `0.99`, `1.00`. The version is **not a decimal floating-point number** and is separate from package versions, ABI revisions and build IDs.

| Accepted independent capability/reliability gains | Language version |
|---:|:---|
| 0 | `0.00` — planning baseline, **not a release** |
| 1 | `0.01` |
| 2 | `0.02` |
| 3 | `0.03` |
| 4 | `0.04` |
| 12 | `0.12` |
| 99 | `0.99` |
| 100 | `1.00` |

The version advances only when an independently accepted, evidenced, user-relevant capability or reliability/security improvement has landed. Docs edits, refactor-only work, elapsed time or number of commits do **not** earn credits. Gain IDs are immutable; duplicate work earns zero. No public release is overwritten; every published release has an immutable tag/artifact hash. A build can have a new commit SHA with unchanged language version when no new gain is accepted.

`1.00` means *100 accepted gains*, **not** that compatibility magically becomes stable. Compatibility and breaking-change classification are separate. Package ecosystem tools may need a different conventional version format. See [status](status.md).

## Coordinated package versioning

The language capability counter is the release-line authority. For a language
version `M.mm`, runtime and extension packages must use the same SemVer
major/minor base `M.m`; the package patch may only change when that package has
its own evidenced gain. For example, language `0.07` authorizes the `0.7.x`
runtime and extension line. A package cannot jump ahead of the language line,
and a package update cannot be justified by a commit count, documentation-only
edit or an unproven feature. Runtime patch releases remain blocked until a
runtime-specific gain ledger is added; the current runtime line therefore
uses patch `0`.

`VERSION.toml` is the source of truth for the current/previous comparison and
the append-only `extension_gains` ledger. Every extension gain records the
language compatibility line, package version, unique ID and tracked evidence.
The release verifier checks the package base, ordered ledger, manifest,
lockfile, Cargo metadata, CLI language constant and evidence paths before a
tag is accepted. A runtime update follows the accepted language gain; an
extension update requires an additional editor-specific evidenced gain on the
same language line.

Historical tags remain immutable. Runtime tags use `seloryn-vM.mm`; extension
tags use `vscode-vM.m.P`. Those namespaces are retained for artifact tooling,
but both tags must resolve to the same language compatibility line. Do not
delete or retag the existing `0.01`–`0.07` history to make it look uniform.

Before publication, `scripts/release/prepare-release.py` creates a single
staging directory containing the runtime archive, the VSIX, `release-manifest.json`
and `SHA256SUMS`. The manifest is the promotion unit: its runtime and extension
versions must match the language line, its artifact hashes are independently
recomputed, and its `abi = "unstable"` field prevents a preview artifact from
being mistaken for a stable ABI release. Staging is local and non-publishing;
the remote release still requires explicit publication authorization.

## Current comparison

The latest earned implementation gain is now recorded in [`VERSION.toml`](../../VERSION.toml)
and checked by [`verify-version.py`](../../scripts/release/verify-version.py):

| Surface | Previous | Current | Reason |
|---|---:|---:|---|
| Language capability | `0.06` | `0.07` | N9 identity-bearing replay/authenticated receipts plus explicit GPU linear training |
| Runtime package | `0.6.0` | `0.7.0` | package identity follows the accepted runtime reliability and GPU gain |
| VS Code extension | `0.4.0` | `0.7.0` | canonical public symbol and strict language-based release line, with its own extension evidence |

The comparison is intentionally visible: a new commit alone cannot advance
the language version. N3B remains the historical reason for `0.01`; N6 is the
historical `0.04` gain, N7 is the `0.05` reliability gain, N8 is the `0.06`
streaming/reliability gain and N9 is the current `0.07` identity/replay/GPU
gain. The extension ledger is checked separately
because an editor patch must carry its own evidence while remaining compatible
with the language line.
