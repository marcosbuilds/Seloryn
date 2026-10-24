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
| P2 — Evidence | Conformance, benchmark and comparison proofs are reproducible, independent and honest about limits | **Conformance and benchmark correctness contracts verified on the current host** | [`conformance.md`](conformance.md) and the public [`Benchmark Suite`](../../benchmarks/README.md) provide executable contracts. The local six-case run passes BENCH-01 through BENCH-06 after the independent BENCH-04 oracle was aligned to the candidate's declared `Tensor[F32]` arithmetic. CI validates both suites, publishes sanitized evidence and retains advisory per-OS reports; no general performance superiority or AI token-economy claim is accepted. |
| P3 — Developer experience | Diagnostics, CLI/LSP and agent-facing inspection improvements have measurable acceptance criteria | **Bounded inspection slice verified; continuation open** | `seloryn inspect` now provides compact resolved-file symbols, imports and diagnostics in `seloryn.analysis.v1`; the release probe and LSP regression pass. Indexed/incremental analysis and rename remain outside the gate. |
| P4 — Performance | Changes are selected from measurements and compared under controlled conditions | **Measured baseline; optimization not accepted** | Commit [`117dc93`](https://github.com/marcosbuilds/Seloryn/commit/117dc93) adds six paired stdlib-reference cases with median/p95 reporting and no aggregate score. The current correctness run passes all six cases; timing remains case-local, descriptive and advisory, so no version gain is recorded. |
| P5 — AI workflow | Any productivity or token-efficiency claim has a defined experiment and independent measurement | **Measurement instrument and one real visible-transcript validation complete; comparison not demonstrated** | [AI-assisted development evaluation](../benchmarks/agent-token-economy.md) and [`research/agent-token-economy`](../../research/agent-token-economy) define ten tasks, independent acceptance and provider-metric missingness. Real `TOKEN-AGENT-01` validates local model-specific BPE counting only; no productivity, billing or token-economy advantage exists. |
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

## CI evidence artifact contract

The public workflow emits one `seloryn-ci-evidence-<os>` artifact per matrix
runner. It contains only JSON files following the source-owned
`seloryn.ci-evidence.v1` schema: one record per benchmark/probe and a
`manifest.json` with the source commit, runner label, exit status, duration,
sanitized summary and explicit `passed`, `failed` or `not-run` states. Raw
stdout/stderr is never persisted. Workspace paths and credential-shaped values
are redacted, while binary hashes and numerical results remain available for
review. The wrapped command's exit status remains blocking, and a missing or
failed expected record makes the manifest incomplete; artifact retention does
not weaken correctness gates.

## Active queue

1. Obtain or document GitHub owner/admin action for branch protection and the
   required `Verify Seloryn` status checks.
2. Increase controlled benchmark samples on a declared host when a performance
   decision is needed; preserve the existing correctness and timing reports and
   do not convert shared-runner timing into a product claim.
3. Supply paired provider exports to the frozen AI suite for hypotheses A and
   B; until then its result remains inconclusive by design.
4. Select the next technical gate only after its acceptance contract is
   written; the current job-lifecycle boundary is local coordination only and
   does not authorize a worker, retry system or remote execution feature.
5. Build the final report from this ledger, the contract files and the linked
   executions. Do not mark an item complete from source presence alone.

## Current release boundary

Seloryn is a bounded `0.12` verified-preview language/runtime. The current
evidence does not establish a stable ABI, public package registry, general GPU
training, distributed execution, production release, general model serving or
AI development cost/token superiority. Those remain explicit boundaries until
new evidence satisfies their acceptance criteria.
