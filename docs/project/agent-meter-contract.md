# Agent meter contract

Status: experimental-implemented, local and provider-neutral.

This contract defines the bounded measurement primitive for Seloryn agent
experiments. It records prompt/response text observables, sanitized execution
metadata and provider-reported usage when a provider export supplies it. It is
an evidence boundary, not a model client, tokenizer oracle for every payload or
billing calculator.

## Scope and invariants

The implementation is [`meter.py`](../../research/agent-token-economy/meter/meter.py)
and its unit tests are [`test_meter.py`](../../research/agent-token-economy/meter/test_meter.py).
The input is a JSON object with optional `record_id`, `prompt`, `response`,
`environment`, `model`, `encoding`, `usage` or `provider_usage` fields.

Every output has:

- `schema: "seloryn.agent-meter"` and integer `schema_version: 1`;
- `local_visible: true`, meaning local values cover visible text fields only;
- `not_provider_billing: true`, which is a hard invariant, not a disclaimer;
- `local_visible_usage.input` and `.output`, with Unicode code points, UTF-8
  bytes, SHA-256 identity and a token count when the optional local tokenizer
  is available;
- `provider_usage`, kept separate from local reconstruction and never filled by
  arithmetic when the provider omitted a field;
- `uncertainty`, including sorted reasons and separate local-text/provider
  status values;
- `privacy`, which states that raw content is redacted by default.

The JSON is rendered with sorted keys, no timestamps and no random identifiers.
The same input and tokenizer environment therefore produce the same serialized
report. Raw prompt and response text never appears in the report. Text identity
is represented by SHA-256 only. Sensitive environment keys such as API keys,
passwords, authorization values, prompts, responses and payloads are replaced
with `<redacted>`. Environment export is allowlist-first: unknown keys are
redacted by default, while long allowlisted metadata is hashed and
length-limited.

## Token semantics

When installed, `tiktoken` is used with an explicit model or encoding (default
`o200k_base`). It counts the extracted visible text with the model-specific BPE
encoding. It does **not** count one token per character, and it does not claim
to reproduce an API request wrapper.

The official OpenAI token-counting guide states that the input-token counting
endpoint accepts the same request shape as the Responses API and includes
message structure, tools, images and files. It also explains that local
tokenizers are limited for these inputs, and that reported output usage can
include non-visible channel, tool-call and other model-generated tokens:
[OpenAI token counting guide](https://developers.openai.com/api/docs/guides/token-counting).

Consequently, local `tokens` are a `local_visible` reconstruction. They are
never renamed to `input_tokens`, `output_tokens` or `total_tokens` unless those
values came from `usage`/`provider_usage` supplied by the provider.

## Provider usage

The meter accepts common provider field names without merging sources:

| Provider field | Output field | Rule |
| --- | --- | --- |
| `input_tokens` or `prompt_tokens` | `input_tokens` | preserve non-negative integer |
| `output_tokens` or `completion_tokens` | `output_tokens` | preserve non-negative integer |
| `total_tokens` | `total_tokens` | preserve only when reported |
| `output_tokens_details.reasoning_tokens` | `reasoning_tokens` | preserve only when reported |
| `input_tokens_details.cached_tokens` | `cached_input_tokens` | preserve only when reported |

Missing, malformed or partial usage remains `null` and creates an uncertainty
reason. The meter does not infer total usage, cost, price, hidden context,
retry count, quality, latency or productivity. A provider's reported output
count can exceed visible response tokens; that difference is not treated as an
error.

## Unicode, tools, files and reasoning

- Unicode code points, UTF-8 bytes and BPE tokens are three different units.
  Combining marks, emoji, non-Latin scripts and invalid input boundaries must
  be tested as separate cases; the meter accepts valid Python Unicode strings.
- Message roles and wrapper delimiters are not serialized into local text
  counts. The report records `message_wrappers_not_counted`.
- Text parts in `content`, `text`, `input_text`, `output_text`, `input`,
  `instructions` and `messages` are eligible for visible counting. Images,
  files, tool schemas, tool arguments and function-call serialization are not
  locally reconstructed. Their presence creates explicit uncertainty reasons.
- Reasoning fields and hidden output are never treated as visible response
  text. A provider's `reasoning_tokens` may be retained separately when
  reported; it is not added to local visible counts.

## Evidence states and release gate

`uncertainty.local_visible_text` is `complete` only when both text fields are
present, the optional tokenizer resolved, and no structured content was
excluded. `uncertainty.provider_billing` is `reported` only when complete
provider usage exists, `partial` when the provider export omits required
fields, and `unknown` when no usage exists; it still does not make the report a
billing invoice. The overall state is `partial` whenever any boundary is
unknown.

This contract is suitable for the public evidence workflow described in the
[Evidence Lab](evidence-lab.md). It does not
earn a language-version credit by itself. A future paired claim requires the
same prompt, tools, files, model, environment, retry policy and quality grader,
plus provider exports for every comparable run. Missing telemetry remains
`inconclusive`, never zero.

Run the focused tests from the repository root:

```text
python research/agent-token-economy/meter/test_meter.py
```

The CLI accepts a record from a file or stdin and writes the redacted report:

```text
python research/agent-token-economy/meter/meter.py run.json
```
