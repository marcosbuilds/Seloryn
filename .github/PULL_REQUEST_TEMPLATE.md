## Problem and scope

Describe the observed problem, affected contract and intended scope.

## Cause or hypothesis

Explain the root cause, or identify the hypothesis being tested.

## Implementation

- Files changed:
- Behavior changed:
- Compatibility impact:

## Verification

List exact commands, platforms and exit results. Include positive and negative
tests where applicable.

```text
# command
# exit 0
```

## Evidence and limitations

State what is verified, experimental, partial, blocked or unsupported. For
benchmarks include workload, reference implementation, repetitions, raw
summary, environment and whether the result is suitable for a product claim.

## Version and release impact

- Language capability gain ID, if any:
- Runtime/editor package impact:
- `VERSION.toml` changed: yes/no
- Release or tag requested: yes/no

If a release is requested, include the artifact, checksum, provenance and
authorization evidence. Do not publish from this pull request automatically.

## Reviewer checklist

- [ ] The diff is scoped and does not rewrite history or weaken a test.
- [ ] Required positive and negative tests are present.
- [ ] `Verify Seloryn` is green on supported platforms.
- [ ] Claims match the demonstrated scope and limitations.
- [ ] Version and release decisions follow `docs/project/versions.md`.
- [ ] No release publication or historical tag mutation is implied.
