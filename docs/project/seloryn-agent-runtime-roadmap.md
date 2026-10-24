# Seloryn Agent Runtime Roadmap

Status: strategic execution plan for the `0.12` verified-preview line.

This roadmap turns the engineering-evolution evidence into an ordered program
for an AI-first language and runtime. It deliberately separates demonstrated
capabilities from hypotheses and future work. A feature is not promoted by
syntax, a fixture, or a passing happy path alone: it needs a contract, an
independent test, bounded failure behavior and an evidence record.

## North star

Seloryn should become excellent at **verifiable execution of AI workflows and
agent teams**: typed tasks, explicit authority, bounded tools, deterministic
inspection, recoverable execution and privacy-aware evidence.

This is a narrower and more defensible target than claiming to replace Python,
provide general LLM training, or reproduce a complete GPU ecosystem.

## What is true today

The repository currently demonstrates, in bounded scope:

- a typed lexer/parser/semantic pipeline and structured diagnostics;
- scalar, collection, project, CPU tensor and autodiff behavior with tests;
- small CPU training and self-hosted `.sely` examples;
- deterministic/replay-oriented receipts, hashes and HMAC contracts;
- `seloryn inspect` that exposes bounded symbols, imports and diagnostics
  without executing source;
- job state, leases, recovery and preflight contracts, but not an automatic
  worker that executes jobs;
- a GPU probe and narrow numerical paths, not complete GPU support;
- a local visible-transcript token instrument and `TOKEN-PAIR-01`.

The paired token experiment uses the same manifest, prompt, declared
environment and local `gpt-4o`/`o200k_base` tokenizer. It measured 99 input /
210 assistant / 309 visible tokens for Python and 99 input / 7 assistant /
106 visible tokens for Seloryn. The conditions are explicitly not functionally
equivalent. These are local visible-text reconstructions, not provider usage,
billing, productivity or token-economy evidence. Exact provider input counts
can include message structure, tools and files, while output usage may include
non-visible generated tokens; see the [official token-counting guide](https://developers.openai.com/api/docs/guides/token-counting).

The following claims remain outside the demonstrated boundary:

- general LLM training or production model serving;
- full GPU training and distributed execution;
- automatic job execution;
- a Python-comparable library/package ecosystem;
- a stable ABI, registry or production runtime guarantee;
- token savings, lower cost, higher productivity or provider billing parity.

## Design laws

1. **Authority is explicit.** Every agent action declares its task, capability,
   tool, input boundary, expected effect, budget and evidence.
2. **Inspection is non-execution.** Agents can build context and diagnostics
   without running untrusted source.
3. **Unknown stays unknown.** Missing provider telemetry, cost or quality is
   recorded as missing, never inferred as zero or success.
4. **Evidence is part of the product.** Program, data, model, tool, artifact,
   attempt and environment identities are hashable and replayable where the
   contract permits.
5. **Local before distributed.** Prove a bounded worker, fencing and recovery
   contract in one process before adding queues, servers or remote workers.
6. **Reference before optimization.** Every numerical or tokenization gain has
   an independent reference and a declared tolerance or exactness rule.
7. **Privacy by default.** Prompts, outputs, tool arguments and retrieved data
   are not persisted or exported unless the task explicitly opts in.

## Three aggressive but safe jumps

### Jump 1 — Auditable agent execution substrate

Unify `seloryn inspect`, task contracts, jobs, leases, preflight, a local
worker, cancellation, fencing, idempotent commit and execution receipts.

The outcome is a real foundation for agent teams: an agent can propose a task,
the runtime can verify authority before execution, and a later agent can
inspect exactly what happened without trusting free-form prose.

### Jump 2 — Real model ports and honest measurement

Introduce provider-neutral request/response ports, deterministic fixtures and
one optional external adapter. Keep these records separate:

- visible transcript;
- provider-reported usage;
- local tokenizer reconstruction;
- latency and retry events;
- cost, when the provider reports it;
- tool/file serialization and redaction metadata.

The objective is not to claim token savings. It is to make a paired claim
answerable under the same prompt, tools, environment, retry policy and quality
grader.

### Jump 3 — Small correct numerical core for models

Add explicit device/layout/dtype/memory contracts, a real byte-level/BPE
tokenizer, interoperable checkpoints and a minimal transformer reference. GPU
acceleration follows CPU parity and memory-safety evidence; it does not lead
the work.

## Execution phases and gates

### Phase 0 — Truth, contracts and baselines

**Deliver:** capability matrix; task/trajectory/evidence schemas; limit and
privacy inventory; latency/memory/failure baselines; provider-metric missingness
policy.

**Gate:** every public claim links to code, test and limitation; all supported
CI platforms are green; no `unknown` becomes a success value.

### Phase 1 — Deterministic context compiler

**Deliver:** versioned and hashable `inspect` context packages containing
symbols, imports, diagnostics, contracts, capabilities, affected files and a
bounded minimum context for one agent task; incremental cache by source hash.

**Gate:** identical snapshot produces identical bytes; byte/symbol limits are
enforced; source is never executed; reduced context preserves task acceptance
against a full-context reference.

### Phase 2 — Safe local worker

**Deliver:** worker identity; process-group ownership; lease fencing; timeout;
cancel; crash recovery; success/failure/cancel states; idempotent effect
commit; pre-run, run and post-run receipts.

**Gate:** an old worker cannot finalize after fencing; an expired lease cannot
silently abandon a live process; retry cannot duplicate effects; crash,
cancel, corruption and path escape have independent tests; artifacts have
hashes and owners.

### Phase 3 — Model ports, tokens and budgets

**Deliver:** provider-neutral model interface; deterministic mock; optional
adapter; separated transcript/usage/cost records; local BPE marked as a
reconstruction; per-agent/task budgets; redacted export.

**Gate:** baseline and candidate share prompt, tools, environment, retry policy
and quality grader; provider usage is present for every comparable run; retries
and failures are counted; missing cost/quality remains inconclusive; sensitive
transcripts are opt-in.

### Phase 4 — Numerical performance substrate

**Deliver:** controlled benchmark harness; allocation profiling; explicit
layout/stride/device/dtype contracts; high-impact fused kernels; CPU/GPU parity
for small kernels.

**Gate:** numerical output is within declared tolerance; memory and time are
measured in controlled environments; no aggregate advantage is published from
one shared runner or one synthetic case.

### Phase 5 — Minimal transformer path

**Order:** byte-level/BPE tokenizer; embeddings and positions; causal
attention; normalization/residual/MLP; checkpoint format; inference with KV
cache; GPU after parity.

**Gate:** loss/perplexity and inference outputs match an independent Python
reference within declared tolerance; Unicode cases are covered; corrupted
checkpoints are rejected; context, memory and time limits are explicit.

### Phase 6 — Stability and distribution

Only after Phases 0–5: evaluate ABI stability, package registry, external
servers, distributed execution, public ecosystem and a stable release channel.
None of these are earned by syntax coverage alone.

## Agent-team operating model

Use disjoint, evidence-producing roles rather than many agents editing the same
surface:

1. **Contract auditor:** maps claims to code, tests, limits and missingness.
2. **Context/compiler agent:** owns inspect/context artifacts and determinism.
3. **Runtime worker agent:** owns jobs, leases, fencing, cancellation and
   idempotent effects.
4. **Model telemetry agent:** owns provider ports, token adapters, usage/cost
   separation and redaction.
5. **Numerical reference agent:** owns Python oracles, tokenizer/model parity,
   tolerances and benchmarks.
6. **Adversarial verifier:** attacks path safety, stale workers, corrupted
   receipts, prompt/tool boundaries and privacy leaks.
7. **Release integrator:** runs the full matrix, checks docs and merges only
   through protected CI.

Every agent handoff must include: exact files inspected/changed, command and
environment, artifact hashes, known limitations, and an explicit status of
`verified`, `experimental`, `unsupported` or `unknown`. Agents must not claim
completion from a plan, a fixture, a local-only run or a green subtest.

## Immediate backlog order

1. Freeze Phase 0 schemas and the capability matrix.
2. Implement the deterministic context package on top of `seloryn inspect`.
3. Implement a single-process worker with fencing and crash tests.
4. Add provider-neutral model ports and separate usage/cost records.
5. Extend the paired token harness to tool/file/retry scenarios only when
   provider exports are available.
6. Add explicit device/layout contracts and CPU reference kernels.
7. Implement the tokenizer and transformer reference before broad GPU work.

Do not prioritize registry, broad HTTP, Python compatibility, distributed
training or general LLM training until the earlier gates are met. The fastest
credible route is to make Seloryn the runtime where an AI team can prove what
it was allowed to do, what it did, what it consumed and why the result is
reproducible.
