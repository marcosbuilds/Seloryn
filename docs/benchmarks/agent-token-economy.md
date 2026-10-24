# AI-assisted development evaluation

Seloryn now carries a frozen, vendor-neutral evaluation kit for the AI-first
workflow question. The suite has ten bounded tasks: five isolate hypothesis A
(language advantage) and five isolate hypothesis B (ecosystem advantage).

The executable kit is under
[`research/agent-token-economy`](../../research/agent-token-economy). It contains
versioned task specs, independent acceptance adapters, a manifest and schemas,
a deterministic local runner, replay fixtures and a report format.

The optional [`token_count.py`](../../research/agent-token-economy/token_count.py)
adapter uses OpenAI's `tiktoken` BPE encodings to count visible text. It reports
characters, UTF-8 bytes and tokens separately; it does not assume one character
equals one token and it is not a provider billing oracle. A transcript can be
measured with `--model` or an explicit encoding after installing the optional
requirements file.

For an agent experiment, export a transcript using the
`seloryn.agent-visible-transcript.v1` shape and run
`research/agent-token-economy/transcript_token_count.py`. The result separates
visible input-role and assistant-role text and is labelled
`visible-transcript-only`; it cannot establish billed totals or a language
advantage without paired provider telemetry.

The first real bounded validation is preserved as
[`TOKEN-AGENT-01.json`](../../research/agent-token-economy/fixtures/experiments/TOKEN-AGENT-01.json).
One subagent executed the fixed three-string task against `gpt-4o` and reported
`hello world` as 2 tokens, `caractere por caractere` as 6, and
`Tensor[F32, shape=[2, 2]]` as 11 under `o200k_base`/`tiktoken 0.14.0`.
This proves the local instrument distinguishes visible BPE tokens from
codepoints and bytes; it does not prove provider billing, hidden-context
accounting, productivity, or token economy.

This change adds the measurement instrument only. No external model was
controlled, no provider result was fabricated, no token total or cost was
estimated, and no productivity or token-efficiency advantage has been
demonstrated. The committed replay is synthetic and is explicitly marked
`not_an_evaluation: true`; real comparisons require provider exports for both a
baseline and a candidate under the declared tool-parity profile.

Run the local contract and replay checks from the repository root:

```text
python research/agent-token-economy/runner.py validate
python research/agent-token-economy/runner.py dry-run
python research/agent-token-economy/runner.py replay --records research/agent-token-economy/fixtures/replay/provider-records.jsonl --workspace research/agent-token-economy/fixtures/replay/workspaces
python research/agent-token-economy/tests/test_runner.py
python research/agent-token-economy/tests/test_token_count.py
python research/agent-token-economy/tests/test_transcript_token_count.py
```

The current result is intentionally inconclusive. The kit records acceptance
status separately from provider success, preserves missing metrics as unknown,
and reports A and B independently.
