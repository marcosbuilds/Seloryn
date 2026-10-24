# Agent token economy evaluation suite

This directory is the frozen, vendor-neutral suite for measuring AI-assisted
development in Seloryn. It is an evaluation instrument, not a model client and
not evidence that a model, provider or hypothesis has won.

The suite is frozen at revision `2026-10-24` and contains exactly ten bounded
tasks:

| Hypothesis | Tasks | What is isolated |
| --- | --- | --- |
| A — language advantage | `A-01` … `A-05` | typed semantics, diagnostics, data/model correctness and invalid-state prevention |
| B — ecosystem advantage | `B-01` … `B-05` | project, editor, receipt, lifecycle and documentation/tooling integration |

The two hypotheses must be reported separately. A result for A is not evidence
for B, and a result for B is not evidence that the language semantics caused
the outcome. Tool parity is declared in `manifest.json`: the same repository
snapshot, task prompt, shell/search/test/diff access, network policy and time
budget must be used for every compared condition.

## Files

- `manifest.json` — immutable suite index and measurement contract.
- `schema/manifest.schema.json` — machine-readable manifest shape.
- `schema/provider-record.schema.json` — provider export contract.
- `schema/report.schema.json` — runner report shape.
- `tasks/` — one frozen specification per task.
- `fixtures/<task>/accept-<task>.py` — independent, read-only acceptance adapter per task.
- `fixtures/replay/workspaces/` — synthetic local workspaces used only to test
  the runner. They are not implementation evidence or benchmark results.
- `fixtures/replay/provider-records.jsonl` — a no-metrics replay envelope. It
  intentionally contains no invented token, cost or success values.
- `runner.py` — deterministic manifest validator, dry-run planner, replay
  runner and report renderer.
- `token_count.py` — optional local adapter using OpenAI's `tiktoken` BPE
  encodings. It reports Unicode code points, UTF-8 bytes and tokens as
  separate units and marks the result as a local reconstruction, never as API
  billing.
- `transcript.py` — canonical CLI/library that applies the adapter to a visible
  agent transcript and separates input-role and assistant-role text. Hidden
  context, tool serialization and provider billing remain explicitly unknown.
- `transcript_token_count.py` — compatibility shim for the historical filename;
  new integrations should use `transcript.py`.
- `schema/visible-transcript.schema.json` — contract for the minimized visible
  transcript envelope.
- `fixtures/experiments/TOKEN-AGENT-01.json` — real bounded subagent transcript
  used to validate the documentation claim. It captures only the user prompt
  and final assistant answer; it is not provider telemetry.
- `paired/TOKEN-PAIR-01.manifest.json` — shared prompt/input/environment for
  the Python/Seloryn paired experiment.
- `paired/python_task.py` and `paired/seloryn_task.sely` — the two bounded
  implementations of the same declared task.
- `paired/run_pair.py` — executes both conditions and feeds both visible
  transcripts through the same local tokenizer.
- `fixtures/experiments/TOKEN-PAIR-01.pair-report.json` — reproducible local
  result with separate transcript/token-count evidence for each condition.
- `tests/test_runner.py` — standard-library tests for validation, dry-run,
  replay and missing-token semantics.
- `tests/test_token_count.py` — token/character/byte separation checks when the
  optional tokenizer dependency is installed.

The replay workspaces contain an explicit task-named submission adapter artifact.
A real harness may point the runner at a candidate checkout instead;
the acceptance adapters remain independent of provider telemetry. The adapter
artifact records only task-observable acceptance facts. It is not allowed to
carry token, cost, attempt or provider-success claims.

## Commands

Run these commands from the repository root:

```text
python research/agent-token-economy/runner.py validate
python research/agent-token-economy/runner.py dry-run --output dry-run.json
python research/agent-token-economy/runner.py replay \
  --records research/agent-token-economy/fixtures/replay/provider-records.jsonl \
  --workspace research/agent-token-economy/fixtures/replay/workspaces \
  --output replay.json
python research/agent-token-economy/runner.py report --input replay.json --format markdown
python research/agent-token-economy/tests/test_runner.py
python research/agent-token-economy/tests/test_token_count.py
python research/agent-token-economy/tests/test_transcript.py
python research/agent-token-economy/transcript.py \
  --input research/agent-token-economy/fixtures/experiments/TOKEN-AGENT-01.json \
  --model gpt-4o
python research/agent-token-economy/paired/python_task_test.py
python research/agent-token-economy/paired/pair_runner_test.py
python research/agent-token-economy/paired/seloryn_task_test.py
python research/agent-token-economy/paired/run_pair.py \
  --manifest research/agent-token-economy/paired/TOKEN-PAIR-01.manifest.json \
  --model gpt-4o \
  --output-dir research/agent-token-economy/fixtures/experiments
```

Install the optional tokenizer adapter with
`python -m pip install -r research/agent-token-economy/requirements-tokenizer.txt`.
Use `token_count.py --model <model> --text <text>` or an explicit
`--encoding`. A character count, byte count or local reconstruction is never
substituted for provider-reported usage; chat wrappers, tool calls and hidden
context can make API billing differ from visible-text tokenization.
The model-specific BPE premise follows [OpenAI's official token-counting
documentation](https://developers.openai.com/api/docs/guides/token-counting),
the [OpenAI tokenizer](https://platform.openai.com/tokenizer) and the official
[`tiktoken`](https://github.com/openai/tiktoken) implementation notes; it is
not a character-by-character accounting rule. OpenAI documents that exact API
input counts include request structure, tools and files, while local
`tiktoken` is a plain-text reconstruction.

## Paired Python/Seloryn experiment

`TOKEN-PAIR-01` uses one manifest, one prompt, one declared environment and one
`gpt-4o`/`o200k_base` local counter for both conditions. The observed Windows
run recorded 99 visible input tokens for each condition. Python returned its
full deterministic codepoint/UTF-8 result in 210 visible output tokens (309
visible total); Seloryn returned its currently supported codepoint total in 7
visible output tokens (106 visible total). This is a capability-boundary
measurement, not a token-economy win: the conditions are explicitly marked
functionally non-equivalent because Seloryn does not yet implement OpenAI BPE,
UTF-8 byte reporting, hidden-context accounting or provider billing.

The pair report preserves the observed platform and binary hash, but it does
not claim that either visible total equals API usage or cost. Provider
telemetry, message wrappers, tools, files and non-visible output remain
unknown.

`replay` does not infer totals from input/output tokens, infer zero from a
missing field, infer files or lines from a workspace, infer human intervention
from an absent event list, or convert acceptance pass into provider success.
The report keeps provider metrics, runner observations and acceptance outcomes
in separate fields.

## Decision policy

The suite emits `inconclusive` when a comparison lacks provider measurements,
paired baseline/candidate runs, complete success observations, or regression
evidence. Non-significance is not equivalence. The local replay is always
labelled `synthetic-replay` and `not_an_evaluation: true`, even when all ten
synthetic acceptance adapters pass. A real provider export must identify its
source and run configuration before it can be compared.

No command here controls an external model, sends prompts, installs a provider
SDK, publishes a release, changes `VERSION.toml`, or pushes Git history.
