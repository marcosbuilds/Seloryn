# Earned capability versions

Language versions use **two numeric components** `M.mm`, where `mm` is always two digits, e.g. `0.00`, `0.01`, `0.12`, `0.99`, `1.00`. The version is **not a decimal floating-point number** and is separate from package versions, ABI revisions and build IDs.

| Accepted independent capability/reliability gains | Language version |
|---:|:---|
| 0 | `0.00` — planning baseline, **not a release** |
| 1 | `0.01` |
| 12 | `0.12` |
| 99 | `0.99` |
| 100 | `1.00` |

The version advances only when an independently accepted, evidenced, user-relevant capability or reliability/security improvement has landed. Docs edits, refactor-only work, elapsed time or number of commits do **not** earn credits. Gain IDs are immutable; duplicate work earns zero. No public release is overwritten; every published release has an immutable tag/artifact hash. A build can have a new commit SHA with unchanged language version when no new gain is accepted.

`1.00` means *100 accepted gains*, **not** that compatibility magically becomes stable. Compatibility and breaking-change classification are separate. Package ecosystem tools may need a different conventional version format. See [status](status.md).
