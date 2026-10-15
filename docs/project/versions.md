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

## Extension versioning

The VS Code package uses numeric SemVer `X.Y.Z`, independently from the
language capability counter. `VERSION.toml` is the source of truth for both
the current/previous comparison and the append-only `extension_gains` ledger.
The release verifier requires a canonical SemVer, a non-decreasing current
version, a unique extension gain with ordered evidence paths when the
extension advances, and a package manifest/lockfile match. A
documentation-only extension change keeps its
package version; a real editor behavior or reliability gain must name its
evidence before a tag is accepted. The extension may remain unchanged while
the runtime advances, but it may never move backwards or repeat a new gain
without evidence. This prevents repeating `0.1.0` or advancing the extension
without a measurable change in Seloryn's behavior.

## Current comparison

The latest earned implementation gain is now recorded in [`VERSION.toml`](../../VERSION.toml)
and checked by [`verify-version.py`](../../scripts/release/verify-version.py):

| Surface | Previous | Current | Reason |
|---|---:|---:|---|
| Language capability | `0.04` | `0.05` | N7 strict project-manifest parsing and future-language rejection |
| Runtime package | `0.4.0` | `0.5.0` | package identity follows the accepted runtime reliability gain |
| VS Code extension | `0.4.0` | `0.4.0` | unchanged: the runtime manifest gain must not be disguised as an editor gain |

The comparison is intentionally visible: a new commit alone cannot advance
the language version. N3B remains the historical reason for `0.01`; N6 is the
historical `0.04` gain, and N7 is the current `0.05` reliability gain. The
extension ledger is checked separately because an editor patch must not be
disguised as a language gain.
