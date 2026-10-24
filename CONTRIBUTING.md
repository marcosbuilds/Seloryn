# Contributing to Seloryn

Seloryn is developed on the public `master` branch. Contributions must keep
the implementation, evidence and public claims aligned; a passing command is
not sufficient evidence for a broader capability claim.

## Before changing code

1. Read the relevant public contracts in `docs/project/`, `docs/reference/`,
   and the tests or scripts named by those documents.
2. Check `master`, the current version ledger and historical tags. Do not
   rewrite history, force-push, delete tags or create a replacement `main`
   branch.
3. Identify the acceptance test and the negative cases for the change. If the
   change is a benchmark or experiment, define the independent reference,
   environment, repetitions and interpretation before measuring it.

## Change workflow

Use a short-lived branch derived from `master` for normal changes. Use names
such as `fix/<issue>-<description>`, `feature/<description>`,
`proof/<experiment>` or `research/<topic>` only when the work actually fits
that purpose. Do not create permanent empty branches.

Every pull request should be reviewable as a complete unit:

1. state the problem and the observed cause or hypothesis;
2. implement the smallest coherent change that addresses it;
3. add or update positive and negative tests;
4. run the repository checks and record the exact commands and exit status;
5. report platform, hardware, toolchain and benchmark conditions where they
   affect the result;
6. state limitations and any evidence that remains experimental or blocked.

The required public workflow is `Verify Seloryn`. Its supported-platform jobs
run on Ubuntu, Windows and macOS. Hardware-dependent probes must remain
capability-aware and must not be converted into unconditional claims.

## Evidence and claims

Use the following labels consistently:

- **Verified:** reproduced by the declared test or experiment at the stated
  scope;
- **Experimental:** implemented but not covered by sufficient independent
  evidence;
- **Partial:** only a bounded slice of the proposed contract exists;
- **Proposed:** design only;
- **Blocked:** verification requires an unavailable platform, permission or
  resource;
- **Unsupported:** explicitly outside the implementation boundary.

Do not claim that Seloryn is generally faster, safer, production-ready,
stable-ABI, distributed or superior to Python from a single fixture. Preserve
raw measurements when possible and distinguish semantic determinism from raw
bytes that intentionally contain process or timestamp identity.

## Versions and releases

The language capability version, runtime package version, editor version and
release channel are separate. Documentation, refactors, CI fixes and
unaccepted experiments do not earn a language-version increment. A version
change requires an independent, user-relevant, implemented, tested and
documented gain recorded in `VERSION.toml` and `docs/project/versions.md`.

Do not publish a release, create a release tag or alter a historical tag from a
normal development change. Release preparation must use the existing scripts
and workflows, include checksums and provenance, and wait for explicit owner
authorization.

## Local checks

At minimum, run the checks relevant to the changed surface. For runtime or
workflow changes, the baseline is:

```text
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked -- --test-threads=1
cargo build --release --locked
```

Run the applicable Python probes and VS Code tests listed in
`.github/workflows/ci.yml`. A local pass does not replace the supported-
platform CI result.

## Pull request review

Reviewers should inspect the diff, rerun the relevant tests, challenge the
scope of every claim and check that no test was weakened merely to match an
incorrect implementation. A change is ready for `master` only when the
required workflow is green and the pull request identifies remaining limits.
