# AI-assisted development evaluation

Seloryn now carries a frozen, vendor-neutral evaluation kit for the AI-first
workflow question. The suite has ten bounded tasks: five isolate hypothesis A
(language advantage) and five isolate hypothesis B (ecosystem advantage).

The executable kit is under
[`research/agent-token-economy`](../../research/agent-token-economy). It contains
versioned task specs, independent acceptance adapters, a manifest and schemas,
a deterministic local runner, replay fixtures and a report format.

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
```

The current result is intentionally inconclusive. The kit records acceptance
status separately from provider success, preserves missing metrics as unknown,
and reports A and B independently.
