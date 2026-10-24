# Seloryn Agent Evaluation, Observability and Release Plan

Status: operational design for the `0.12` verified-preview line.

This document is the execution contract for proving AI-agent behavior around
Seloryn. It does not promote an implementation, a benchmark, a local token
count, or a passing fixture into a product capability by itself. A claim is
accepted only when the required artifact, independent observation, limitation,
owner and release decision are recorded together.

The plan follows the repository mission and
[`seloryn-agent-runtime-roadmap.md`](seloryn-agent-runtime-roadmap.md). It is
deliberately vendor-neutral. OpenAI/tiktoken measurements are useful adapters
for the current token experiment, but provider-reported usage remains the
authoritative source for billing and API usage when it is available.

## 1. Decision this plan supports

The release question is:

> Does the candidate Seloryn revision preserve authorized, reproducible and
> bounded agent behavior, while improving the declared roadmap slice without
> introducing an unsafe side effect, privacy failure, authorization bypass,
> unreconciled job effect or unmeasured claim?

The decision is made separately for each roadmap phase and risk tier. A result
can be:

- `approved`: every required hard invariant passed and the statistical or
  categorical evidence is sufficient for the declared claim;
- `approved_with_conditions`: hard invariants passed, but a named limitation,
  monitor, expiry date and rollback owner constrain the release;
- `hold_insufficient_evidence`: no hard violation was found, but required
  evidence is missing, inaccessible, incomparable or contradictory;
- `blocked`: a hard invariant failed or the candidate cannot be safely
  contained.

No weighted average can turn `hold_insufficient_evidence` or `blocked` into a
pass. Unknown is a first-class result, not zero, false, success or “no event”.

## 2. Current evidence boundary

The following is the baseline to which future changes are compared. These are
repository-grounded observations, not promises about unimplemented work.

| Area | Current artifact | Current status | What it proves | What it does not prove |
| --- | --- | --- | --- | --- |
| Source/project context | `src/analysis.rs`, `src/main.rs`, `tests/structured_inspection_probe.py` | `verified` bounded slice | `seloryn inspect` emits symbols, imports, diagnostics and an `seloryn.analysis.v1` envelope without executing source | incremental index, context minimization quality, task acceptance equivalence |
| Job store and direct worker | `src/job.rs`, `src/job_worker.rs`, job lifecycle/lease/recovery/preflight/worker probes | `experimental` direct-process slice | local specs, state, ordered events, atomic lease operations, validated source snapshot, bounded Seloryn execution, lease fencing, timeout/cancellation and committed hashed result receipt | descendant process groups, arbitrary commands, retries/idempotency beyond the direct receipt, or remote execution |
| Agent evaluation suite | `research/agent-token-economy/manifest.json`, `tasks/`, `fixtures/*/accept-*.py`, `runner.py` | `verified` as a frozen instrument | ten bounded task specifications and independent acceptance adapters | provider/model performance; the synthetic replay is not a model evaluation |
| Provider records | `research/agent-token-economy/schema/provider-record.schema.json`, `fixtures/replay/provider-records.jsonl` | `experimental` envelope; current values unknown | schema and missingness behavior for provider exports | real provider usage, cost, success or intervention rate; current replay intentionally has null metrics |
| Visible token reconstruction | `transcript.py`, `token_count.py`, `schema/visible-transcript.schema.json`, `paired/run_pair.py` | `verified` local reconstruction | same visible input can be counted with one declared local tokenizer and separated by role | API billing, hidden context, tools/files serialization, non-visible output, productivity or token savings |
| Python/Seloryn pair | `paired/TOKEN-PAIR-01.manifest.json`, `fixtures/experiments/TOKEN-PAIR-01.pair-report.json` | `experimental` boundary experiment | shared manifest/prompt/declared environment and explicit non-equivalence | a language or token-economy advantage; the conditions are not functionally equivalent |
| Numerical/ML behavior | `src/tensor_autograd.rs`, CPU training and model probes under `tests/` | `verified` only in declared narrow paths | bounded CPU tensor/autodiff/training examples and selected probes | transformer-scale training, complete GPU support, distributed execution or ecosystem parity |

Every run must record the revision and binary hash used. A link to source code
or a green subtest is not sufficient evidence for a broader claim.

## 3. Contracts

### 3.1 Task contract

Each evaluated case is immutable for the decision run and contains, directly or
through the frozen task spec:

1. `case_id`, hypothesis, task class and risk tier;
2. the exact prompt or instruction hash; raw text is not exported by default;
3. authorized objective and explicit non-objectives;
4. allowed tools, paths, network policy, write boundary, time/resource budget
   and human-authority boundary;
5. expected artifact, state and side-effect contract;
6. success, abstention, escalation, timeout and refusal conditions;
7. independent acceptance command and fixture reset method;
8. required provider metrics, with a null/unknown policy for each;
9. privacy class, retention, consent/rights source and contamination risk;
10. owner, creation/review revision and changelog.

The existing `research/agent-token-economy/manifest.json` is the frozen index
for `A-01` through `B-05`; each task spec points to an independent adapter.
Before a provider comparison, the manifest hash, task hashes and tool-parity
profile must be captured in the run record. A task spec or fixture may not be
silently edited after candidate results exist. Changes create a new suite
version and changelog entry.

### 3.2 Trajectory contract

The trajectory is the minimum redacted record needed to answer whether the
agent reached the outcome through an authorized and recoverable path. It must
include:

- run, case, experiment, parent/child and attempt identifiers;
- step sequence, bounded step type, start/end or elapsed duration, outcome,
  error category, retry relation and stop reason;
- selected tool name and validated argument/result summaries or opaque digests,
  never raw arguments/results by default;
- permission check and authority decision before every effectful operation;
- state transition, planned effect, committed effect and rollback/compensation;
- obligation status: `met`, `missed`, `abstained`, `escalated` or `unknown`;
- grounding/evidence references resolvable only under access control;
- final outcome and linked grader results.

The trace must show omissions as well as actions. A clean no-tool trace is not
success when the task required a receipt, notification, artifact or handoff.
An escalation is a valid boundary outcome only when it has a destination,
owner, acknowledgement deadline and terminal disposition. Acknowledgement is
not proof that the original action completed.

The canonical machine-readable shape is
[`research/agent-token-economy/schema/agent-run.v1.json`](../../research/agent-token-economy/schema/agent-run.v1.json).
It is a record contract for future runs; its existence does not mean the
runtime currently emits every field.

### 3.3 Immutable dataset and manifest

Before a comparison, freeze:

- suite ID/version, manifest SHA-256, task/spec/fixture hashes and commit;
- provenance, rights/consent, creation and review owners;
- hypothesis, task taxonomy, declared slices and source class;
- expected outputs, side effects, reset/sandbox method and protected paths;
- contamination/leakage risk and whether a case is prevalence-oriented,
  curated, adversarial, synthetic replay or regression;
- access class, retention/deletion process, redaction version and changelog;
- exclusion rules, provider/model drift policy and missing-data policy.

The ten current fixtures are bounded local acceptance cases. They are not
production prevalence samples and must not be silently reweighted as one.
`fixtures/replay/provider-records.jsonl` is a synthetic no-metrics replay;
nulls there intentionally remain unknown. A real provider export must be
identified by source, configuration, authorization and run IDs before it can
support a comparison.

Required pre-run checks:

```text
python research/agent-token-economy/runner.py validate
python research/agent-token-economy/runner.py dry-run --output <immutable-plan>.json
```

The plan file is evidence of the intended comparison, not its result. The
runner must reject a record whose task, manifest, condition or tool-parity
identity does not match the frozen plan.

### 3.4 Grader contract

Graders are complementary and each may gate only the property it observes.
Every grader has an ID/version, visible inputs, output schema, scale and
direction, abstention/error behavior, reference material, valid slices,
failure costs and owner.

| Grader | Observes directly | Used for | Cannot establish alone |
| --- | --- | --- | --- |
| Deterministic schema/arithmetic | JSON shape, hashes, exact counts, allowed IDs and numeric tolerances | contract integrity and local token arithmetic | semantic quality, safety or grounding |
| Acceptance adapter | fixture-specific observable result and exit status | task acceptance | provider success, agent quality or regression absence |
| Execution/environment | sandbox state, permissions, files, job events and side effects | idempotency, path safety, cleanup and recovery | subjective answer quality |
| Trajectory review | order, authority, retries, stopping, escalation and first divergence | authorized path and recovery | unobserved effects when telemetry is missing |
| Human/domain/safety review | risk-sensitive or domain criteria | hard policy and ambiguous cases | universal truth; disagreement must remain visible |
| Model judge, if later approved | rubric-bounded answer property from allowed evidence | scalable screening, never sole authorization | external facts, hidden state or hard safety invariants |
| Pairwise/ranking, if later approved | relative quality under blinded randomized comparison | exploratory preference | absolute correctness or equivalence |

Model/human graders require calibration material held out from tuning, blinded
candidate identity where applicable, disagreement analysis and revalidation
after prompt/model/rubric/task-distribution changes. A judge may abstain;
abstention is not failure and is not a pass.

### 3.5 Hard invariants

The following are categorical release blockers for any applicable case:

1. unauthorized tool, path, network, data access or state mutation;
2. sensitive-data disclosure outside the declared redacted/consented boundary;
3. credentials, raw prompts, raw outputs, hidden reasoning or unbounded tool
   arguments written to default telemetry;
4. path traversal, symlink/reparse escape or write outside the sandbox;
5. stale/expired job ownership finalizing an effect;
6. duplicate or unreconciled effect after retry/recovery;
7. forged, reordered or corrupted receipt/event history accepted as valid;
8. a required obligation silently abandoned without owned escalation;
9. a release artifact promoted without matching commit, checksums, required CI
   checks and recorded approval;
10. a claim of provider usage, cost, productivity, parity or token savings
    derived from local reconstruction or missing telemetry.

`unknown` for a hard invariant is not a pass. If the evidence cannot answer
whether an invariant held, the gate is `hold_insufficient_evidence`; if the
missing evidence itself results from an unsafe or unauthorized path, the gate
is `blocked`.

## 4. Metrics and missingness

Keep these dimensions separate:

### Correctness and trajectory

- independent acceptance status and return code;
- obligation completion, correct abstention/escalation and first-divergence
  category;
- hard-invariant outcomes and observed side effects;
- retry, timeout, cancellation, recovery and duplicate-effect counts;
- grader result, abstention, disagreement and slice coverage.

### Provider and local measurements

Provider records copy authoritative fields exactly:

`tokens.input`, `tokens.output`, `tokens.total`, calls, attempts, provider
duration, cost, provider success, typed intervention events and provider
regressions. The runner does not calculate total from input plus output, derive
cost from a price table, infer provider success from local acceptance, or turn
an absent intervention list into zero.

Local transcript records separately contain visible input-role tokens,
assistant-role tokens, UTF-8 bytes and Unicode code points. `transcript.py` and
the pair runner mark this as a local reconstruction and `not_provider_billing`.
OpenAI-style wrappers, roles/boundaries, tools/files and non-visible generated
content may change provider accounting. Tokenization is not character-by-
character billing.

For every metric report:

- numerator, denominator and eligibility rule;
- `observed`, `not_collected`, `not_applicable`, `redacted` or `error` status;
- missingness by condition and slice;
- paired case outcomes, distribution, uncertainty/effect size and confounders;
- retries/failures/timeouts separately from successful cases;
- baseline/candidate config, environment and instrumentation changes.

Do not interpret lower intervention, token or latency counts as success without
the corresponding quality, authorization and side-effect evidence. Do not use
non-significance as equivalence. Do not pool hypothesis A and B: language
semantics and ecosystem/tooling are separate claims.

## 5. Privacy and observability contract

The default export is metadata only: opaque IDs, bounded operation class,
outcome/error category, duration, resource counters from an authoritative
source, configuration versions, side-effect category and evaluator result.

The default export excludes raw prompts, outputs, tool arguments/results,
retrieved content, hidden reasoning, credentials, filesystem contents and
personal data. Redact at the first boundary using an allowlist, then validate
the redactor with positive and negative fixtures. Hashes and opaque references
are not automatically private; review them for re-identification risk.

For every signal, record purpose, producer, schema version, sensitivity,
cardinality, sampling, redaction, access role, retention/deletion, correlation
key, owner and failure behavior. Keep high-cardinality case IDs out of metric
labels. Store case-level evidence in restricted artifacts with auditable
access, not in general logs.

Intervention detail is opt-in and question-scoped. The aggregate count is
distinct from typed events; missing detail is not zero. A `complete` detail
coverage record must equal the number of human events; `partial` cannot exceed
the scalar; `not_collected` has an empty event list. Do not retain human text
or free-form explanations by default.

When telemetry collection fails, emit an explicit collection error and make
the affected decision inconclusive. Never broaden production capture merely to
make a diagnosis easier. For an incident: contain harm, preserve the smallest
authorized evidence, classify the failure (agent/tool/data/policy/
infrastructure/grader/telemetry), reproduce in a safe fixture, fix, then add a
transformed regression case only after rights, privacy, retention and
contamination review.

## 6. Phase execution and gates

### Phase 0 — Truth, contracts and baseline

**Evidence:** this plan; the roadmap; frozen manifest; task hashes; capability
matrix; local/CI commands; privacy inventory; baseline run record.

**Gate:** every public claim links to implementation, independent test and
limitation; all supported CI platforms are green; every unavailable provider,
cost, quality or intervention value is explicitly unknown; no hard invariant
is unknown for a release claim.

**Validation:** `runner.py validate`, all token tests, all relevant probes,
`cargo fmt --all -- --check`, `cargo clippy --all-targets --locked -- -D warnings`,
`cargo test --all-targets --locked -- --test-threads=1`, and the three-platform
CI run for the exact candidate commit.

### Phase 1 — Deterministic context compiler

**Evidence:** versioned `inspect` context package, source hash, bounded byte/
symbol limits, deterministic serialization, affected-file list and reduced-vs-
full context acceptance comparison.

**Gate:** identical snapshot produces identical bytes; limits reject or bound
oversized input; source is never executed; package references resolve to the
same revision; reduced context does not silently change task acceptance for the
declared comparison set.

**Validation:** run `seloryn inspect <source-or-project> --format=json` twice
from a clean fixture and compare canonical bytes and hashes; run the structured
inspection probe; include malformed UTF-8, parse error, semantic error,
symlink/reparse and path-boundary cases; compare full/reduced context using
the independent task adapters. Until these tests exist, the phase remains
`proposed` beyond the current bounded inspect slice.

### Phase 2 — Safe local worker

**Evidence:** worker identity, process ownership, lease fencing, timeout,
cancellation, crash recovery, idempotency key, pre/run/post receipts and
environment postconditions.

**Gate:** an old worker cannot finalize after fencing; expiration cannot leave
a live process unowned; retry cannot duplicate a committed effect; crash,
cancel, corrupted event log and path escape have independent outcomes.

**Validation:** retain the current lifecycle, preflight, 100-way claim,
recovery, tamper, write-failure and direct-worker probes as evidence, then add
process-group/job-object failure injection and inspect the sandbox after every
attempt. The current `job run` proves only one bounded Seloryn child; the
administrative `status`, `claim`, `heartbeat`, `release` and `recover` APIs do
not by themselves prove general automatic execution.

### Phase 3 — Model ports, tokens and budgets

**Evidence:** provider-neutral request/response adapter, deterministic mock,
optional provider export, separate transcript/usage/cost/retry records,
redacted export, per-agent/task budget and quality grader.

**Gate:** baseline/candidate share prompt, tools, files, environment, retry
policy and grader; every comparable attempt has provider usage or the decision
is inconclusive; failures/retries are included; missing cost/quality remains
unknown; sensitive content is opt-in.

**Validation:** execute `TOKEN-PAIR-01` only as a local boundary experiment;
validate `paired/run_pair.py`, compare manifest/prompt/tokenizer identities,
and confirm `functionally_equivalent` is false until the task behaviors match.
For a real claim, import provider records that satisfy the provider schema,
run paired baseline/candidate tasks, review per-case traces and perform
privacy/redaction checks before aggregating.

### Phase 4 — Numerical performance substrate

**Evidence:** controlled benchmark manifest, independent Python oracle,
allocation/profile records, explicit device/layout/dtype contracts and small
CPU/GPU parity cases.

**Gate:** declared numerical tolerance, memory/time distributions and hardware
requirements pass; no aggregate advantage is published from one noisy shared
runner or one synthetic case.

**Validation:** use the current CPU tensor/autodiff probes as correctness
baseline; add paired reference outputs and controlled repeats; separate
correctness failures from performance variance; mark GPU-dependent cases
`not_applicable` only with a recorded hardware reason, never as a pass.

### Phase 5 — Minimal transformer path

**Evidence:** tokenizer/vectorization cases including Unicode, embeddings,
causal attention, normalization/residual/MLP, checkpoint integrity, inference
with bounded KV cache and an independent Python reference.

**Gate:** loss/perplexity and outputs match the reference within declared
tolerance; corrupted checkpoints reject; context, memory and time limits are
explicit; GPU follows CPU parity.

**Validation:** freeze a held-out corpus and checkpoint set, compare outputs
and failure behavior across implementations, run Unicode and malformed-input
cases, and review memory/time distributions. No transformer or subword
capability is implied by the current bigram/CPU evidence.

### Phase 6 — Stability and distribution

ABI, registry, external servers, distributed execution and public ecosystem
claims are out of scope until Phases 0–5 have release evidence. The gate must
include compatibility fixtures, rollback of stateful artifacts, installation
verification and a maintained support policy.

## 7. Regression and trajectory review procedure

For every candidate change:

1. freeze baseline/candidate revisions, dataset/manifest, configuration,
   grader versions, environment, slices and exclusions;
2. run the same cases and comparable conditions; preserve failures, timeouts,
   retries and missing records;
3. inspect the first trajectory divergence, not only the final error;
4. distinguish task-contract, trajectory, tool/environment, data, policy,
   deployment, grader and telemetry failures;
5. query sandbox state for planned/committed/rolled-back effects and idempotency;
6. review hard invariants before quality, cost or latency metrics;
7. document confounders, uncertainty, grader disagreement and evidence gaps;
8. if confirmed, transform the incident into a privacy-reviewed regression
   case without tuning the decision set into a contaminated oracle;
9. assign owner, containment, rollback and next review trigger.

The current ten task adapters are suitable for regression acceptance of their
declared properties. They do not cover every language, worker, model, provider,
security or production condition. New cases must name the escaped behavior and
the property they directly observe.

## 8. Release-gate matrix

| Dimension | Required evidence | Decision rule | Hard/statistical | Owner | Missing/failure action |
| --- | --- | --- | --- | --- | --- |
| Task outcome | immutable cases, independent adapters, paired results | declared slice and task-contract rule | categorical/statistical | QA/documentation lead | hold; inspect adapter and fixture |
| Trajectory/authority | redacted run records, obligation ledger, permission checks | no unauthorized path; required obligations evidenced | hard | runtime/security lead | block or contain; never average away |
| Privacy | redaction tests, access/retention record, export sample | no prohibited field in default export | hard | security/privacy owner | block export/release; delete/quarantine |
| Side effects/recovery | sandbox postconditions, receipts, failure injection | no duplicate/unreconciled harmful effect | hard | runtime worker owner | block; reconcile and rollback |
| Reliability | crash/cancel/timeout/retry distributions and probes | workload-specific SLO plus hard recovery cases | mixed | runtime/release owner | hold or conditional approval with monitor |
| Tokens/cost/resources | provider authority, local reconstruction separated, distributions | only claim what authoritative data supports | statistical/categorical | model telemetry owner | inconclusive; keep unknown |
| Numerical correctness | independent oracle, tolerance, corrupted-input cases | all declared cases within tolerance | hard/statistical | numerical owner | block affected phase |
| Operability | correlation, alert, retention, rollback rehearsal | owner can detect, contain and revert | categorical | release integrator | hold until rehearsal passes |
| CI/artifact integrity | exact commit, required Linux/macOS/Windows checks, hashes | protected-branch rules and artifact identity match | hard | release integrator | do not promote |

The authority for the final decision is the protected-branch release owner.
Each gate record must link the evidence owner, exact revision, decision,
conditions, expiration, rollback/containment action and next review trigger.
Conditional approval is invalid without a monitor and a named halt authority.

## 9. Concrete run and release checklist

Before evaluation:

- clean checkout; record branch, commit, tags and uncommitted-state check;
- validate frozen manifest, task/spec/fixture hashes and resettable sandboxes;
- record model/provider/config/tool versions and permission boundary;
- verify redaction and retention policy; opt in to content only when required;
- generate immutable plan and register baseline/candidate identities.

During evaluation:

- use the same task order policy and comparable environment;
- write only bounded run records conforming to `agent-run.v1`;
- preserve failures, timeouts, retries, abstentions and telemetry errors;
- do not edit the decision dataset or tune graders on held-out cases;
- inspect hard invariants and environment state after effectful cases.

Before release:

- run the independent task adapters and applicable probes;
- run Python token/replay/pair tests and separate provider-record validation;
- run Rust formatting, clippy, locked tests and release build;
- verify Linux/macOS/Windows required CI on the exact candidate commit;
- complete trajectory/privacy/security review and resolve discussions;
- publish only claims supported by the gate matrix; record unsupported and
  unknown fields explicitly;
- rehearse rollback for code, receipts, job state and generated artifacts.

The plan is complete for a phase only when its gate has a recorded verdict.
The existence of this document or schema is itself only `planned` evidence;
execution artifacts must be produced by the corresponding phase owner.
