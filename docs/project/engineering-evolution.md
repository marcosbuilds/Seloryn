# Engineering evolution and evidence program

This page is the execution ledger for the Seloryn engineering program. It is
not a substitute for implementation or evidence: every completed item must
point to code, tests, a workflow, or a reproducible external result.

## Objective

Keep `master` as the public source of truth while improving Seloryn through
audited, bounded and reproducible work. The program is complete only when the
applicable acceptance criteria below are evidenced, or an external blocker is
recorded with the exact action required to remove it.

The operating loop is:

```text
AUDIT -> PLAN -> IMPLEMENT -> TEST -> REVIEW -> MEASURE -> INTEGRATE -> VERIFY
```

## Stage board

| Stage | Acceptance scope | State | Evidence or blocker |
|---|---|---|---|
| P0 — Integrity | Baseline preserved, CI failures investigated, supported-platform CI green, no weakened correctness tests | **Complete for the current restoration slice** | [CI run 40](https://github.com/marcosbuilds/Seloryn/actions/runs/40058223002), commits [`fccdbf4`](https://github.com/marcosbuilds/Seloryn/commit/fccdbf4916a5aac4e09e3c9dc4db7145ae39acbd), [`66e4c93`](https://github.com/marcosbuilds/Seloryn/commit/66e4c93517ca9a610a43a6ca374cdb115f04eafb), [`e0f22b7`](https://github.com/marcosbuilds/Seloryn/commit/e0f22b72ad747141db6f1051eb91edee7fe5fcc5) |
| P1 — Organization | Public documentation agrees with implementation; `master` and historical refs preserved; integration and release policy are explicit | **In progress** | Documentation corrections are landed. GitHub reports `master` unprotected and no rulesets; owner/admin configuration is still required. |
| P2 — Evidence | Conformance, benchmark and comparison proofs are reproducible, independent and honest about limits | **Conformance foundation implemented; cross-platform integration pending** | [`conformance.md`](conformance.md), [`conformance_matrix.json`](../../tests/conformance_matrix.json) and [`conformance.py`](../../scripts/ci/conformance.py) map 23 capabilities to 38 existing probes, source paths, platform requirements and limits. The isolated Windows run passed core, editor, hardware, release and benchmark suites; this unpushed worktree still needs the new workflow to run on Linux/macOS. No general performance superiority or AI token-economy claim is accepted. |
| P3 — Developer experience | Diagnostics, CLI/LSP and agent-facing inspection improvements have measurable acceptance criteria | **Bounded slices verified; continuation open** | Current LSP, diagnostics and project-contract probes are documented in [`status.md`](status.md) and [`next-gates.md`](next-gates.md). |
| P4 — Performance | Changes are selected from measurements and compared under controlled conditions | **Evidence-only baseline** | Shared-runner benchmarks are non-blocking. No version gain is recorded without a controlled before/after result. |
| P5 — AI workflow | Any productivity or token-efficiency claim has a defined experiment and independent measurement | **Not demonstrated** | Existing auditable local-inference proofs do not measure agent tokens, cost, retries or task success. |
| P6 — Release | Version ledger, compatibility, artifacts, checksums and authorization are complete | **Candidate only** | Language remains `0.12`, runtime/extension remain `0.12.0`; no release publication is authorized by this program. |

## Non-negotiable controls

- `master` remains the default public branch. Do not create or use `main` as a
  replacement, and do not rewrite history, force-push, delete tags or retag
  historical releases.
- A documentation, reliability correction or refactor does not earn a language
  version increment. A gain must be independent, user-relevant, implemented,
  verified, documented and accepted.
- Correctness tests block regressions. Hardware tests declare capability
  requirements. Benchmarks report measurements but do not turn shared-runner
  noise into a product claim.
- No release is published merely because CI passes. A release candidate must
  match the version ledger and receive explicit owner authorization.
- Every material change is reviewed against the repository contracts and is
  committed only with its relevant local and remote evidence.

## Active queue

1. Obtain or document GitHub owner/admin action for branch protection and the
   required `Verify Seloryn` status checks.
2. Run the public conformance matrix on all supported CI platforms after an
   authorized integration, then preserve raw benchmark/probe evidence and
   separate verified results from experimental or hardware-dependent results.
3. Select the next technical gate only after its acceptance contract is
   written; the current job-lifecycle boundary is local coordination only and
   does not authorize a worker, retry system or remote execution feature.
4. Build the final report from this ledger, the contract files and the linked
   executions. Do not mark an item complete from source presence alone.

## Current release boundary

Seloryn is a bounded `0.12` verified-preview language/runtime. The current
evidence does not establish a stable ABI, public package registry, general GPU
training, distributed execution, production release, general model serving or
AI development cost/token superiority. Those remain explicit boundaries until
new evidence satisfies their acceptance criteria.
