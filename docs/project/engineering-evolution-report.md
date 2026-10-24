# Seloryn Engineering Evolution — Final Evidence Report

> **Current continuation snapshot (PR):** this report preserves the historical
> `master` snapshot below and adds the continuation evidence from branch
> `proof/token-pair-and-governance`. The current candidate is the head of pull
> request
> [#1](https://github.com/marcosbuilds/Seloryn/pull/1). Its required CI run is
> shown on the [PR checks page](https://github.com/marcosbuilds/Seloryn/pull/1/checks);
> the three-platform workflow must be successful for the current head. The pull request is
> intentionally still open because the active `Protect master - verified CI`
> ruleset requires one independent approval; no approval is fabricated here.

The historical snapshot is retained so that the report does not rewrite the
project's prior evidence. The current phase coverage and exact residual gates
are summarized in [`program-coverage.md`](program-coverage.md).

Date: 2026-10-24
Repository: `marcosbuilds/Seloryn`
Historical evidence branch: `master`
Evidence snapshot commit: `ac4b7a30a568662c31bf2954686198651859fd4d`
Report commits: `a091ef3`, `235df5c`

Current candidate branch: `proof/token-pair-and-governance`
Current candidate commit: see the PR head and corresponding CI artifact manifest

## Executive result

Sections A–G below are the historical `master` evidence snapshot identified
above. Section H and the linked program-coverage matrix describe the current
continuation candidate.

The restoration and evidence program was executed through implementation,
independent review, local verification, remote CI and documentation. The
repository remains on `master`, historical tags and releases were preserved,
the runtime version ledger remains `0.12`/`0.12.0`, and no new release was
published.

The main technical gains are:

- hardened CI evidence and job lifecycle/recovery contracts;
- executable public conformance and benchmark suites;
- an independently corrected F32 numerical oracle with all six benchmark
  correctness contracts passing;
- a bounded `seloryn inspect` command for agent-facing project context;
- an OpenAI-compatible, model-specific local BPE token measurement adapter;
- one real bounded agent transcript experiment preserved as machine-readable
  evidence.

The program does not claim general performance superiority, provider billing
accuracy, productivity improvement, or token-economy advantage. Those claims
remain unsupported without controlled paired runs and provider telemetry.

## A. Initial state and preserved history

- Baseline before the restoration slice: `5c30e30fbb58b34f51c7e42628499fd4a2203702`.
- Final state: `ac4b7a30a568662c31bf2954686198651859fd4d`.
- `master` remained the public/default branch; no `main` branch was created.
- Historical tags preserved: `seloryn-v0.05`, `seloryn-v0.06`,
  `seloryn-v0.07`, and `vscode-v0.1.0` through `vscode-v0.4.0`.
- Existing GitHub releases were preserved; no release/tag was deleted, moved or
  retagged.
- `VERSION.toml` stayed at language `0.12`, runtime `0.12.0`, extension
  `0.12.0`, channel `verified-preview`.
- The adjacent private `operator` workspace was not touched or committed.

The initial CI defects were investigated rather than hidden: cross-platform
lint/reliability failures, Windows workspace reuse, evidence artifact handling,
and a benchmark oracle mismatch. The benchmark mismatch was caused by a
Python `float64` oracle accumulating updates against a candidate using
`Tensor[F32]`; the independent oracle was corrected to round binary32 at each
primitive operation without widening the tolerance.

## B. Implemented changes

Key integrated commits on `master`:

| Commit | Result |
|---|---|
| `fccdbf4`, `66e4c93`, `e0f22b7` | CI reliability and POSIX/macOS job-directory reuse fixes |
| `38b3a2a`, `5af4ef1`, `57b0505`, `a34f552`, `68b01d0` | Evidence ledger, contribution controls, conformance matrix and sanitized CI artifacts |
| `4c25323` | Atomic job commit journal, recovery and symlink/reparse protections |
| `9ddf63e` | Frozen vendor-neutral AI development evaluation suite |
| `117dc93`, `94e303f` | Reproducible six-case benchmark suite and corrected F32 oracle |
| `b596259` | Benchmark and AI evaluation gates in CI |
| `19a707a` | OpenAI-compatible local token measurement adapter |
| `c866cf7` | `seloryn inspect` structured source/project inspection slice |
| `ac4b7a3` | Real agent transcript fixture, token report, schema and CI validation |
| `a091ef3` | Consolidated final evidence report and documentation index |

The developer-experience change was independently implemented in worktree
`structured-inspection` and reviewed before cherry-pick. It reuses the parser,
project resolver, semantic checker and `seloryn.analysis.v1` envelope; it does
not claim indexed analysis, incremental parsing, rename or formatter support.

## C. Historical GitHub organization and release governance

This section describes the historical `master` snapshot named at the top of
the report. The current PR configuration is recorded in Section H and in the
program-coverage matrix.

- Public source of truth remains `master`.
- Research/proof work was isolated in agent worktrees and integrated only after
  review and local validation. No public research branch was required for the
  completed slices.
- The final CI workflow run is [run #48](https://github.com/marcosbuilds/Seloryn/actions/runs/40063341549),
  triggered by the final commit, with Ubuntu, macOS and Windows jobs successful.
- Historical snapshot: GitHub returned no repository rulesets and `master` was
  still unprotected at that time; the current ruleset is recorded in Section H.
  This is the remaining owner/admin blocker. The required action is to enable
  branch protection or a ruleset requiring the `Verify Seloryn` checks, review
  policy, no force-push and no deletion.
- No pull request or release publication was created by this execution. Direct
  integration to `master` is recorded in the commit history; publication of a
  release remains owner-authorized work only.

## D. Verification evidence

Local Windows validation at the final implementation state:

| Command/evidence | Result |
|---|---|
| `cargo fmt --all -- --check` | exit `0` |
| `cargo test --all-targets --locked -- --test-threads=1` | exit `0`; 85 unit + 37 CLI + 13 tensor tests |
| `cargo clippy --all-targets --all-features --locked -- -D warnings` | exit `0` |
| `cargo build --release --locked` | exit `0` |
| `python tests/structured_inspection_probe.py` | passed |
| `python tests/lsp_probe.py` | passed |
| `python scripts/ci/conformance.py --validate` | 24 capabilities / 39 probes validated |
| `python scripts/ci/conformance.py --run --suite core` | 30 probes passed; four documented boundaries |
| Benchmark contract tests and full runner | 5 contract tests; 6/6 cases passed |
| Token adapter tests | 3 tests passed with `tiktoken 0.14.0` |
| Visible transcript test and schema validation | passed; transcript report generated |

The historical remote run #48 passed all three supported CI matrix jobs, including the
release build/tests, benchmark contract validation, structured inspection,
editor checks, reliability probes and sanitized evidence finalization. It
retained these artifacts according to the workflow's configured retention:

- `seloryn-ci-evidence-ubuntu-24.04`, SHA-256
  `7ecc113b50abb6829e732cae14ccadd0b09ad80325a93655a00bd50e03b3f45a`;
- `seloryn-ci-evidence-macos-15`, SHA-256
  `a5fcf94b52a5a51982804471cc4e6b507c1d55e579eeab21d06bc74497f8e051`;
- `seloryn-ci-evidence-windows-2022`, SHA-256
  `9834aea034bc53ab241d43111daa6a1cc69c3154dd2ea2a2b02b2ff861d23431`;
- one `seloryn-benchmark-report-<os>` artifact for each matrix runner.

Hardware-dependent GPU and unsupported distributed/registry/HTTP transformer
capabilities remain explicit documented boundaries; they are not silently
treated as passes.

## E. Benchmark and comparison findings

The benchmark suite uses independent standard-library Python references and
reports correctness, median and p95 timings without an aggregate score. The
final local report has:

| Case | Result |
|---|---|
| BENCH-01 JSON validation | passed |
| BENCH-02 data transformation | passed |
| BENCH-03 numerical computation | passed |
| BENCH-04 small neural network | passed with F32-aligned oracle |
| BENCH-05 language model | passed |
| BENCH-06 error handling | passed |

The measurements are descriptive, host-specific and advisory. The workloads
are deliberately small, CPU-only and not a claim about general Python,
framework-scale training or transformer-serving performance. Memory was not
accepted as a product claim, and no version increment was earned by the
benchmark results.

## F. AI development and token measurement

The implementation is in
[`research/agent-token-economy`](../../research/agent-token-economy). It keeps
Unicode code points, UTF-8 bytes and model-specific BPE tokens as separate
units. The adapter uses `tiktoken` locally, records the resolved encoding and
marks every result `not_provider_billing: true`.

The real bounded experiment is
[`TOKEN-AGENT-01.json`](../../research/agent-token-economy/fixtures/experiments/TOKEN-AGENT-01.json),
with its sanitized report in
[`TOKEN-AGENT-01.token-count.json`](../../research/agent-token-economy/fixtures/experiments/TOKEN-AGENT-01.token-count.json).
A subagent executed the fixed task with `gpt-4o` mapped to `o200k_base` and
reported:

| Text | Unicode code points | UTF-8 bytes | BPE tokens |
|---|---:|---:|---:|
| `hello world` | 11 | 11 | 2 |
| `caractere por caractere` | 23 | 23 | 6 |
| `Tensor[F32, shape=[2, 2]]` | 25 | 25 | 11 |

The visible transcript itself measured 183 input-role tokens, 199 assistant
tokens and 382 visible tokens total. Cost, provider-billed usage, hidden
context, tool serialization, number of attempts and a paired baseline/candidate
productivity comparison were not available. Therefore the AI result is a
successful instrument validation, not evidence of token economy or a language
advantage.

OpenAI's tokenizer documentation describes tokens as model-processed character
sequences rather than one-character units, and the official `tiktoken` project
describes the BPE implementation: [OpenAI tokenizer](https://platform.openai.com/tokenizer),
[official tiktoken repository](https://github.com/openai/tiktoken).

The continuation phase added the frozen paired experiment
[`TOKEN-PAIR-01.pair-report.json`](../../research/agent-token-economy/fixtures/experiments/TOKEN-PAIR-01.pair-report.json).
It uses one manifest, one visible prompt, `network=disabled`, `seed=0`, the
same local `gpt-4o`/`o200k_base` tokenizer and the same release binary for the
Seloryn condition. An independent subagent reproduced the command with exit
code `0` without changing files. The observed Windows evidence is:

| Condition | Visible input tokens | Visible assistant tokens | Visible total | Observable result |
|---|---:|---:|---:|---|
| Python | 99 | 210 | 309 | codepoints, UTF-8 bytes and hashes for all three inputs |
| Seloryn | 99 | 7 | 106 | Unicode codepoint total `59`; stdout `{"stdout":"","value":59}` |

The pair deliberately records `functionally_equivalent: false`: Seloryn does
not yet implement OpenAI-compatible BPE, native token counting, UTF-8 byte
reporting, hidden-context accounting, provider telemetry or billing. These are
visible-text local reconstructions, not provider usage, cost or productivity
evidence. OpenAI's current documentation states that exact input counts can
include message structure, tools and files, and that reported output can
include non-visible generated tokens; the local pair therefore cannot be
promoted to an API billing claim. See the [official token-counting guide](https://developers.openai.com/api/docs/guides/token-counting).

## G. Conclusions and remaining decisions

Improved and verified:

- CI and evidence handling are reliable across Ubuntu, macOS and Windows.
- Job persistence/recovery has explicit integrity and path-safety contracts.
- Conformance and benchmark correctness are executable and independently
  reviewed.
- Agent-facing project inspection now exposes bounded symbols, imports and
  diagnostics without executing source, plus a deterministic redacted
  `seloryn.context.v1` package for valid inspections.
- A bounded local `seloryn job run` worker now executes only validated
  `seloryn-run.v1` sources through the Seloryn binary, with lease fencing,
  timeout/cancel handling, bounded output hashes and an atomic result receipt.
- Token measurement now has a real local instrument, schema, tests, a real
  subagent experiment and explicit non-billing limitations.
- The Python/Seloryn paired harness now reproduces one shared-input experiment,
  preserves both visible transcripts and token reports, and exposes the
  current Seloryn tokenization boundary without fabricating equivalence.
- The GitHub About metadata is configured with a capability-accurate public
  description and seven topics; the active `Protect master - verified CI`
  ruleset applies to `master` with review, current-branch, three-platform CI,
  conversation-resolution and force-push/deletion protections.

Not demonstrated:

- general runtime or Python performance superiority;
- provider billing totals or token-cost savings;
- AI productivity or language/ecosystem advantage;
- stable ABI, distributed execution, general GPU training, production model
  serving or a public package registry;
- provider-billed token totals, hidden context, tool/file serialization or
  output reasoning accounting;
- token economy, productivity or language/ecosystem advantage; the paired
  conditions are explicitly not functionally equivalent;
- general-purpose job orchestration, descendant process-group fencing, retries
  or remote execution; the direct worker is intentionally narrower.

The exact next research action is to obtain paired, consented,
provider-exported baseline/candidate runs for the frozen AI suite. Until those
exports exist, the comparison remains `inconclusive` for billing, cost,
productivity and token economy. The GitHub governance blocker is resolved for
the current repository state; future direct pushes to `master` are intentionally
blocked by the active ruleset and must go through the declared review/CI path.

The forward execution plan is maintained in the [Seloryn Agent Runtime
Roadmap](seloryn-agent-runtime-roadmap.md). Its three prioritized jumps are an
auditable agent-execution substrate, provider-neutral model ports with honest
telemetry, and a small numerically correct model core before broad GPU or
distribution work.

This report is an evidence record, not a release announcement. No release was
published.

## H. Continuation delta — current candidate

The continuation phase was deliberately scoped to the executable gaps found by
the agent-runtime and token-economy audit. It adds:

- a bounded `seloryn job run` worker with validated source snapshots, lease
  fencing, timeout/cancellation cleanup, bounded stdout/stderr evidence and an
  atomic committed receipt;
- a deterministic, redacted `seloryn.context.v1` context package for the
  existing inspection path;
- a provider-neutral meter that keeps Unicode code points, UTF-8 bytes, local
  model-specific BPE tokens and provider-reported usage separate;
- the frozen `TOKEN-PAIR-01` Python/Seloryn experiment with identical visible
  prompt, manifest, model and encoding, while explicitly recording
  `functionally_equivalent: false`, the source commit and the Seloryn binary
  hash;
- the filename audit and naming policy. `tensor_get_f32` remains canonical:
  its `tensor_` namespace, operation and dtype are all part of the published
  meaning. Ambiguous aliases such as `getf32`, `ten_get_f32` and
  `tensor_getr_f32` are not introduced;
- GitHub About metadata and the protected `master` ruleset, including required
  review, conversation resolution and three-platform CI checks.

The current candidate was validated locally with formatting, locked Rust
check/test/clippy/release-build gates, the meter and pair tests, structured
inspection, job-worker/lifecycle/recovery/preflight probes, and the frozen
agent-evaluation suite. The current PR checks page is the cross-platform
authority for its head; CI artifact manifests identify the exact commit,
runner and binary hashes for each completed run.

The current candidate still does **not** claim general LLM/transformer
training, full GPU tensor/autodiff/MLP support, distributed execution, a
package registry, stable ABI, general job supervision, descendant
process-group fencing, retries, remote workers, provider billing or a measured
token-economy advantage. Those are documented next gates, not silently
converted into passes.
