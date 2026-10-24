# Engineering program coverage

This matrix is the coordinator's current audit against the execution program
in the original mission document. It distinguishes an implemented bounded
contract from a broader capability that is intentionally still outside the
release claim.

| Phase | Current state | Evidence in this candidate | Remaining gate |
|---|---|---|---|
| P0 — integrity and trust | Verified for the declared platforms when the current PR head's workflow is green | Locked Rust gates, Python probes, release build, [PR checks](https://github.com/marcosbuilds/Seloryn/pull/1/checks), sanitized evidence workflow | Independent PR approval before integration to `master` |
| P1 — repository organization | Verified | `master` preserved, no `main` branch, About metadata, active `Protect master - verified CI` ruleset, naming and filename policies | Apply the same policy to later PRs; no release publication is implied |
| P2 — public proofs | Verified in bounded scope | `tests/conformance_matrix.json`, `scripts/ci/conformance.py`, `benchmarks/runner.py`, six benchmark contracts, token-pair fixtures and schemas | More repeated controlled samples and provider exports before performance/productivity claims |
| P3 — AI developer experience | Verified bounded slice | `seloryn inspect`, `seloryn.analysis.v1`, redacted `seloryn.context.v1`, LSP and structured-inspection probe | Indexed/incremental analysis, rename and broader change tooling remain future work |
| P4 — runtime performance | Experimental evidence | Tensor contiguous-access path and benchmark harness with independent references | Controlled profiling and statistically sufficient before/after evidence for any general claim |
| P5 — AI workflow | Experimental bounded slices | `seloryn job run`, agent evaluation plan, provider-neutral meter, replay fixtures and Python/Seloryn pair | General supervisor, process groups/job objects, retries, remote workers and provider telemetry |
| P6 — release readiness | Not a release candidate | Version ledger and release scripts remain intact; CI is green | Independent approval, clean-install artifact verification and explicit owner authorization for publication |

## Acceptance rules

`verified` in this table means that the named bounded behavior has a real
positive and negative probe. It does not promote neighboring functionality.
`experimental` means implementation plus evidence exists, but the stronger
product claim is not yet accepted. `future`/`remaining gate` is an honest
boundary, not a failed test and not a promise that the feature already exists.

The token experiment follows the same rule. OpenAI tokenization is model-
specific BPE rather than one-character counting, so the local meter uses
`tiktoken` when available and reports code points, UTF-8 bytes and BPE tokens
as different units. Provider billing, hidden context, tool/file serialization
and output accounting remain unknown unless provider telemetry is supplied.

## Naming decision

Published tensor names retain semantic structure. `tensor_get_f32` communicates
the namespace (`tensor`), operation (`get`) and dtype (`f32`) in a form that
matches the naming conventions used by typed numerical libraries. `getf32`,
`ten_get_f32` and `tensor_getr_f32` are rejected because they either hide the
tensor domain or add an unexplained abbreviation. A future namespaced surface
such as `tensor.get_f32` may reduce repetition without weakening meaning, but
it is a separate compatibility decision.

## Reproduction commands

From the repository root:

```text
cargo fmt --all -- --check
cargo check --all-targets --locked
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked -- --test-threads=1
cargo build --release --locked
python scripts/ci/conformance.py --validate
python scripts/ci/conformance.py --run --suite core
python benchmarks/test_benchmark_contracts.py
python research/agent-token-economy/meter/test_meter.py
python research/agent-token-economy/paired/pair_runner_test.py
```

The commands above are evidence collection, not a claim that every future
phase is complete. The authoritative current cross-platform result is the CI
run linked in the P0 row.
