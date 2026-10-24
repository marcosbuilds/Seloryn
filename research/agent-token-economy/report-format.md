# Report format

The runner emits `seloryn.agent-token-economy.report` JSON and can render the
same record as Markdown. A report has these top-level fields:

| Field | Meaning |
| --- | --- |
| `suite` | Frozen suite ID, version, revision and task count. |
| `mode` | `dry-run` or `replay`; neither invokes a model. |
| `evidence_class` | `planned`, `synthetic-replay` or `provider-replay`. |
| `not_an_evaluation` | `true` for dry-runs and synthetic fixtures. |
| `results` | One row per frozen task, in ordinal order. |
| `hypotheses.A` / `hypotheses.B` | Separate conclusion and evidence gaps. |
| `limitations` | Explicit missingness and interpretation boundaries. |

Each task result separates:

- `provider_metrics`: values copied from the provider record. `tokens.input`,
  `tokens.output`, `tokens.total`, `calls`, `attempts`, provider
  `duration_seconds`, `cost_usd`, `success`, `human_intervention`,
  `files_changed`, `lines_added`, `lines_deleted` and `regressions` stay null
  unless the provider supplied them.
- `acceptance`: the local fixture's independent exit status and runner-observed
  adapter time. It is not provider time and it cannot establish provider
  success.
- `provider`: provider name/model metadata supplied in the record.

The runner does not calculate `tokens.total` from input plus output, calculate
cost from a price table, count files/lines from a workspace, turn absent human
events into zero, or infer regressions from a failed acceptance test. It also
does not store raw prompts, model outputs, hidden reasoning or credentials.

## Example task row

```json
{
  "task_id": "A-01",
  "hypothesis": "A",
  "provider_record_id": "run-001",
  "provider": {"name": "example-provider", "model": "example-model"},
  "provider_metrics": {
    "tokens": {"input": null, "output": null, "total": null},
    "calls": null,
    "attempts": null,
    "duration_seconds": null,
    "cost_usd": null,
    "success": null,
    "human_intervention": null,
    "files_changed": null,
    "lines_added": null,
    "lines_deleted": null,
    "regressions": null
  },
  "acceptance": {"status": "passed", "returncode": 0, "runner_duration_seconds": 0.012345},
  "evaluation_status": "observed-provider-success"
}
```

This example is a schema illustration, not a recorded run. The null total is
intentional: a provider that does not export a total does not authorize the
runner to invent one.

## Interpretation

The conclusion for each hypothesis is `inconclusive` until the report contains
paired baseline/candidate observations under the same tool-parity profile,
independent acceptance for the relevant tasks, provider success and the
provider metrics needed by the preregistered comparison. Missingness is an
evidence gap, not a zero and not a pass.
