# Seloryn runtime 0.06

This prerelease advances the language capability from `0.05` to `0.06` and the
runtime package from `0.5.0` to `0.6.0`.

## Accepted gain: verifiable CPU training receipts

- `seloryn run <project> --log=json2` emits a schema-v2 NDJSON run envelope for
  the bounded `train_linear` and `train_mlp` profiles.
- Every record carries a sequence number, previous hash and SHA-256 record
  hash; the envelope ends with a structured `run.end` record.
- The independent verifier works without the Seloryn binary and rejects field
  alteration, missing/reordered records, truncation, append-after-completion
  and duplicate JSON keys.
- The black-box probe covers clean MLP and linear projects, v1 stdout
  compatibility, error receipts, manifest limit enforcement and cleanup after a
  failed atomic checkpoint replacement.
- Event sink failures now abort training at the emission boundary. Checkpoint
  temporary files are removed on write/replace failure, and project JSON
  limits apply to training datasets and checkpoints.

## Explicit non-claims

This release does not provide replay, signatures, redaction, asynchronous
backpressure, cross-platform numerical identity, GPU tensor training,
distributed execution, serving or general execution telemetry. `--log=json`
remains the schema-v1 compatibility stream. Preflight failures before
`run.start` remain ordinary CLI diagnostics; JSON2 receipts currently cover
executions that entered one of the two bounded training profiles.

The release is a verified preview, not a stable ABI or installer release.
