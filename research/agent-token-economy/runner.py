#!/usr/bin/env python3
"""Validate and replay the frozen agent-token-economy suite.

This runner is deliberately provider-neutral. It never sends a prompt or
starts an external model. A replay consumes provider records supplied by the
caller and runs only local, read-only acceptance adapters.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
from pathlib import Path
from typing import Any


SUITE_ROOT = Path(__file__).resolve().parent
REPO_ROOT = SUITE_ROOT.parents[1]
MANIFEST_PATH = SUITE_ROOT / "manifest.json"
SCHEMA_DIR = SUITE_ROOT / "schema"
METRIC_FIELDS = (
    "tokens",
    "calls",
    "attempts",
    "duration_seconds",
    "cost_usd",
    "success",
    "human_intervention",
    "files_changed",
    "lines_added",
    "lines_deleted",
    "regressions",
)
TOKEN_FIELDS = ("input", "output", "total")
EVIDENCE_CLASSES = {"provider-export", "synthetic-replay"}


class EvaluationError(ValueError):
    """A suite, provider record or report contract is invalid."""


def read_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise EvaluationError(f"cannot read JSON {path}: {error}") from error


def relative(path: Path) -> str:
    try:
        return path.relative_to(REPO_ROOT).as_posix()
    except ValueError:
        return str(path)


def validate_task(task: dict[str, Any], entry: dict[str, Any]) -> dict[str, Any]:
    task_id = entry.get("id")
    if task.get("schema") != "seloryn.agent-token-economy.task" or task.get("schema_version") != 1:
        raise EvaluationError(f"{task_id}: unsupported task schema")
    required = ("id", "ordinal", "frozen", "hypothesis", "title", "prompt", "deliverable", "acceptance", "tool_parity", "limits", "provider_telemetry")
    missing = [field for field in required if field not in task]
    if missing:
        raise EvaluationError(f"{task_id}: missing task fields {missing}")
    if task.get("id") != task_id or task.get("ordinal") != entry.get("ordinal"):
        raise EvaluationError(f"{task_id}: manifest and task identity disagree")
    if task.get("frozen") is not True or task.get("hypothesis") not in {"A", "B"}:
        raise EvaluationError(f"{task_id}: task must be frozen and assigned to A or B")
    acceptance = task["acceptance"]
    if not isinstance(acceptance, dict) or acceptance.get("independent") is not True:
        raise EvaluationError(f"{task_id}: acceptance must be independent")
    command = acceptance.get("command")
    if not isinstance(command, list) or len(command) < 3 or not all(isinstance(value, str) and value for value in command):
        raise EvaluationError(f"{task_id}: acceptance command must be a non-empty argv list")
    if "{workspace}" not in command:
        raise EvaluationError(f"{task_id}: acceptance command must contain {{workspace}}")
    fixture = acceptance.get("fixture")
    if not isinstance(fixture, str) or not fixture:
        raise EvaluationError(f"{task_id}: acceptance fixture is missing")
    fixture_path = (SUITE_ROOT / fixture).resolve()
    acceptance_scripts = list(fixture_path.glob("accept-*.py"))
    contracts = list(fixture_path.glob("contract-*.json"))
    if not fixture_path.is_dir() or len(acceptance_scripts) != 1 or len(contracts) != 1:
        raise EvaluationError(f"{task_id}: acceptance fixture is incomplete: {fixture}")
    contract = read_json(contracts[0])
    if contract.get("task_id") != task_id or not isinstance(contract.get("required_observations"), list) or not contract["required_observations"]:
        raise EvaluationError(f"{task_id}: invalid acceptance contract")
    if entry.get("fixture") != fixture:
        raise EvaluationError(f"{task_id}: manifest fixture and task fixture disagree")
    command_script = (SUITE_ROOT / command[1]).resolve()
    if not command_script.is_file() or command_script.parent != fixture_path:
        raise EvaluationError(f"{task_id}: acceptance command does not point to its fixture script")
    for command_part in command[1:]:
        if command_part.startswith("http://") or command_part.startswith("https://"):
            raise EvaluationError(f"{task_id}: acceptance command may not use a URL")
    if "VERSION.toml" not in task.get("limits", []) and "VERSION.toml" not in task.get("deliverable", {}).get("must_not_change_paths", []):
        # The frozen suite has a global protected-path rule; task prose must
        # repeat it so a spec cannot silently widen its scope.
        if "VERSION.toml" not in " ".join(str(item) for item in task["limits"]):
            raise EvaluationError(f"{task_id}: task must prohibit VERSION.toml changes")
    return task


def load_manifest() -> tuple[dict[str, Any], dict[str, dict[str, Any]]]:
    for schema_name in ("manifest.schema.json", "provider-record.schema.json", "report.schema.json"):
        schema = read_json(SCHEMA_DIR / schema_name)
        if not isinstance(schema, dict) or schema.get("$schema") != "https://json-schema.org/draft/2020-12/schema":
            raise EvaluationError(f"invalid schema document: {schema_name}")
    manifest = read_json(MANIFEST_PATH)
    if not isinstance(manifest, dict):
        raise EvaluationError("manifest root must be an object")
    expected = {
        "schema": "seloryn.agent-token-economy.manifest",
        "schema_version": 1,
        "suite_id": "agent-token-economy",
        "suite_version": "1.0.0",
        "frozen_revision": "2026-10-24",
        "frozen": True,
        "task_count": 10,
    }
    for field, value in expected.items():
        if manifest.get(field) != value:
            raise EvaluationError(f"manifest {field} must equal {value!r}")
    if set(manifest.get("hypotheses", {})) != {"A", "B"}:
        raise EvaluationError("manifest must define exactly hypotheses A and B")
    parity = manifest.get("tool_parity", {})
    if parity.get("profile") != "parity-v1" or parity.get("network") != "disabled":
        raise EvaluationError("tool parity must use parity-v1 with network disabled")
    contract = manifest.get("measurement_contract", {})
    if contract.get("missing_value") is not None or "tokens.total" not in contract.get("provider_metrics", []):
        raise EvaluationError("measurement contract must preserve missing values as null and include total tokens")
    if "VERSION.toml" not in manifest.get("protected_paths", []):
        raise EvaluationError("VERSION.toml must be protected")
    entries = manifest.get("tasks")
    if not isinstance(entries, list) or len(entries) != 10:
        raise EvaluationError("manifest must contain exactly ten tasks")
    tasks: dict[str, dict[str, Any]] = {}
    ordinals = []
    for entry in entries:
        if not isinstance(entry, dict):
            raise EvaluationError("manifest task entries must be objects")
        task_id = entry.get("id")
        if task_id in tasks or entry.get("hypothesis") not in {"A", "B"}:
            raise EvaluationError(f"invalid or duplicate task id: {task_id!r}")
        spec_path = SUITE_ROOT / entry.get("spec", "")
        if not spec_path.is_file():
            raise EvaluationError(f"{task_id}: task spec does not exist: {entry.get('spec')}")
        task = read_json(spec_path)
        if not isinstance(task, dict):
            raise EvaluationError(f"{task_id}: task spec must be an object")
        tasks[task_id] = validate_task(task, entry)
        ordinals.append(entry.get("ordinal"))
    if sorted(ordinals) != list(range(1, 11)):
        raise EvaluationError("task ordinals must be exactly 1 through 10")
    if sum(task["hypothesis"] == "A" for task in tasks.values()) != 5 or sum(task["hypothesis"] == "B" for task in tasks.values()) != 5:
        raise EvaluationError("the frozen suite must contain five A tasks and five B tasks")
    return manifest, dict(sorted(tasks.items(), key=lambda item: item[1]["ordinal"]))


def empty_metrics() -> dict[str, Any]:
    return {
        "tokens": {field: None for field in TOKEN_FIELDS},
        "calls": None,
        "attempts": None,
        "duration_seconds": None,
        "cost_usd": None,
        "success": None,
        "human_intervention": None,
        "files_changed": None,
        "lines_added": None,
        "lines_deleted": None,
        "regressions": None,
    }


def normalize_metrics(raw: Any) -> dict[str, Any]:
    if raw is None:
        return empty_metrics()
    if not isinstance(raw, dict):
        raise EvaluationError("provider metrics must be an object")
    result = dict(raw)
    tokens = result.get("tokens")
    if tokens is None:
        result["tokens"] = {field: None for field in TOKEN_FIELDS}
    elif isinstance(tokens, dict):
        result["tokens"] = dict(tokens)
        for field in TOKEN_FIELDS:
            result["tokens"].setdefault(field, None)
    else:
        raise EvaluationError("provider metrics.tokens must be an object or null")
    for field in METRIC_FIELDS:
        result.setdefault(field, None)
    return result


def load_records(path: Path, tasks: dict[str, dict[str, Any]]) -> list[dict[str, Any]]:
    records: list[dict[str, Any]] = []
    run_ids: set[str] = set()
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except OSError as error:
        raise EvaluationError(f"cannot read provider records {path}: {error}") from error
    for line_number, line in enumerate(lines, start=1):
        if not line.strip():
            continue
        try:
            record = json.loads(line)
        except json.JSONDecodeError as error:
            raise EvaluationError(f"provider record line {line_number} is invalid JSON: {error}") from error
        if not isinstance(record, dict):
            raise EvaluationError(f"provider record line {line_number} must be an object")
        if record.get("schema") != "seloryn.agent-token-economy.provider-record" or record.get("schema_version") != 1:
            raise EvaluationError(f"provider record line {line_number} has an unsupported schema")
        run_id = record.get("run_id")
        task_id = record.get("task_id")
        if not isinstance(run_id, str) or not run_id or run_id in run_ids:
            raise EvaluationError(f"provider record line {line_number} has a missing or duplicate run_id")
        if task_id not in tasks:
            raise EvaluationError(f"provider record line {line_number} references unknown task {task_id!r}")
        if record.get("evidence_class") not in EVIDENCE_CLASSES:
            raise EvaluationError(f"provider record {run_id} has an unsupported evidence_class")
        provider = record.get("provider")
        if not isinstance(provider, dict) or not isinstance(provider.get("name"), str) or not provider["name"]:
            raise EvaluationError(f"provider record {run_id} needs provider.name")
        normalize_metrics(record.get("metrics"))
        run_ids.add(run_id)
        records.append(record)
    return records


def resolved_command(command: list[str], workspace: Path) -> list[str]:
    resolved: list[str] = []
    for index, part in enumerate(command):
        value = str(workspace) if part == "{workspace}" else part
        if index == 0 and value in {"python", "python3"}:
            value = sys.executable
        elif index > 0 and not value.startswith("{"):
            candidate = (SUITE_ROOT / value).resolve()
            try:
                candidate.relative_to(SUITE_ROOT.resolve())
            except ValueError:
                pass
            else:
                value = str(candidate)
        resolved.append(value)
    return resolved


def run_acceptance(task: dict[str, Any], workspace: Path | None) -> dict[str, Any]:
    if workspace is None:
        return {"status": "not-run", "returncode": None, "runner_duration_seconds": None}
    if not workspace.is_dir():
        return {"status": "error", "returncode": None, "runner_duration_seconds": 0.0, "error": "workspace is not a directory"}
    command = resolved_command(task["acceptance"]["command"], workspace)
    started = time.monotonic()
    try:
        completed = subprocess.run(
            command,
            cwd=REPO_ROOT,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            encoding="utf-8",
            errors="replace",
            check=False,
            timeout=120,
        )
    except (OSError, subprocess.SubprocessError) as error:
        return {"status": "error", "returncode": None, "runner_duration_seconds": round(time.monotonic() - started, 6), "error": str(error)}
    return {
        "status": "passed" if completed.returncode == 0 else "failed",
        "returncode": completed.returncode,
        "runner_duration_seconds": round(time.monotonic() - started, 6),
    }


def workspace_for_task(workspace_root: Path | None, task_id: str) -> Path | None:
    if workspace_root is None:
        return None
    candidate = workspace_root / task_id
    return candidate if candidate.is_dir() else workspace_root


def build_result(task_id: str, task: dict[str, Any], record: dict[str, Any] | None, workspace_root: Path | None, *, run_acceptance_checks: bool) -> dict[str, Any]:
    metrics = normalize_metrics(record.get("metrics") if record else None)
    acceptance = run_acceptance(task, workspace_for_task(workspace_root, task_id)) if run_acceptance_checks else {
        "status": "not-run",
        "returncode": None,
        "runner_duration_seconds": None,
    }
    provider = record.get("provider") if record else None
    result = {
        "task_id": task_id,
        "hypothesis": task["hypothesis"],
        "condition": record.get("condition") if record else None,
        "provider_record_id": record.get("run_id") if record else None,
        "provider": provider,
        "provider_evidence_class": record.get("evidence_class") if record else None,
        "provider_metrics": metrics,
        "acceptance": acceptance,
        "evaluation_status": "inconclusive",
    }
    if record and metrics.get("success") is not None and acceptance["status"] == "passed":
        result["evaluation_status"] = "observed-provider-success" if metrics["success"] is True else "observed-provider-failure"
    return result


def build_report(manifest: dict[str, Any], tasks: dict[str, dict[str, Any]], *, mode: str, records: list[dict[str, Any]] | None = None, workspace_root: Path | None = None, run_acceptance_checks: bool = False) -> dict[str, Any]:
    records = records or []
    evidence_classes = {record.get("evidence_class") for record in records}
    if mode == "dry-run":
        evidence_class = "planned"
        not_an_evaluation = True
    elif "synthetic-replay" in evidence_classes:
        evidence_class = "synthetic-replay"
        not_an_evaluation = True
    else:
        evidence_class = "provider-replay"
        not_an_evaluation = False
    records_by_task: dict[str, list[dict[str, Any]]] = {task_id: [] for task_id in tasks}
    for record in records:
        records_by_task[record["task_id"]].append(record)
    results: list[dict[str, Any]] = []
    for task_id, task in tasks.items():
        task_records = records_by_task[task_id] or [None]
        results.extend(build_result(task_id, task, record, workspace_root, run_acceptance_checks=run_acceptance_checks) for record in task_records)
    hypotheses: dict[str, Any] = {}
    for hypothesis in ("A", "B"):
        selected = [result for result in results if result["hypothesis"] == hypothesis]
        missing = []
        if any(result["provider_record_id"] is None for result in selected):
            missing.append("one or more tasks have no provider record")
        if any(result["provider_metrics"].get("success") is None for result in selected):
            missing.append("provider success is missing for at least one task")
        if any(result["acceptance"]["status"] != "passed" for result in selected):
            missing.append("independent acceptance is not passed for every task")
        missing.append("no paired baseline/candidate comparison was supplied")
        hypotheses[hypothesis] = {
            "name": manifest["hypotheses"][hypothesis]["name"],
            "conclusion": "inconclusive",
            "reason": missing,
            "task_ids": [result["task_id"] for result in selected],
        }
    limitations = [
        "This runner does not control or invoke an external model.",
        "Provider metrics are copied only when supplied; absent values remain null and are not zero.",
        "Acceptance pass is independent from provider success, cost, tokens, calls, attempts, files, lines and regressions.",
        "A hypothesis requires paired baseline/candidate observations; one replay or a synthetic fixture cannot establish an advantage.",
        "The committed replay is synthetic and must not be cited as benchmark evidence.",
    ]
    if mode == "dry-run":
        limitations.append("Dry-run validates the frozen plan but executes no acceptance adapter.")
    return {
        "schema": "seloryn.agent-token-economy.report",
        "schema_version": 1,
        "suite": {
            "suite_id": manifest["suite_id"],
            "suite_version": manifest["suite_version"],
            "frozen_revision": manifest["frozen_revision"],
            "task_count": manifest["task_count"],
        },
        "mode": mode,
        "evidence_class": evidence_class,
        "not_an_evaluation": not_an_evaluation,
        "provider_record_count": len(records),
        "results": results,
        "hypotheses": hypotheses,
        "limitations": limitations,
    }


def write_json(value: dict[str, Any], output: Path | None) -> None:
    rendered = json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
    if output is None:
        print(rendered, end="")
    else:
        output.write_text(rendered, encoding="utf-8")
        print(f"wrote {relative(output)}")


def render_markdown(report: dict[str, Any]) -> str:
    lines = [
        "# Agent token economy report",
        "",
        f"- Mode: `{report['mode']}`",
        f"- Evidence class: `{report['evidence_class']}`",
        f"- Not an evaluation: `{str(report['not_an_evaluation']).lower()}`",
        f"- Frozen suite: `{report['suite']['suite_id']} {report['suite']['suite_version']}` ({report['suite']['frozen_revision']})",
        "",
        "## Hypothesis conclusions",
        "",
        "| Hypothesis | Conclusion | Reason |",
        "| --- | --- | --- |",
    ]
    for key in ("A", "B"):
        item = report["hypotheses"][key]
        lines.append(f"| {key} — {item['name']} | `{item['conclusion']}` | {'; '.join(item['reason'])} |")
    lines.extend([
        "",
        "## Task observations",
        "",
        "| Task | Acceptance | Provider success | Input | Output | Total | Calls | Attempts | Cost USD | Files | +Lines | -Lines |",
        "| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |",
    ])
    for result in report["results"]:
        metrics = result["provider_metrics"]
        tokens = metrics["tokens"]
        values = [
            result["task_id"],
            result["acceptance"]["status"],
            str(metrics["success"]).lower() if metrics["success"] is not None else "unknown",
            tokens["input"], tokens["output"], tokens["total"], metrics["calls"], metrics["attempts"], metrics["cost_usd"], metrics["files_changed"], metrics["lines_added"], metrics["lines_deleted"],
        ]
        lines.append("| " + " | ".join("unknown" if value is None else str(value) for value in values) + " |")
    lines.extend(["", "## Limitations", ""])
    lines.extend(f"- {item}" for item in report["limitations"])
    lines.extend(["", "Token totals are never derived from input/output fields. `unknown` means the provider did not supply the value.", ""])
    return "\n".join(lines)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("validate", help="validate the frozen manifest, specs and fixtures")
    dry_run = subparsers.add_parser("dry-run", help="emit a deterministic plan without invoking acceptance or a provider")
    dry_run.add_argument("--output", type=Path)
    replay = subparsers.add_parser("replay", help="replay provider JSONL records and local acceptance adapters")
    replay.add_argument("--records", required=True, type=Path)
    replay.add_argument("--workspace", type=Path, help="candidate workspace or task-id workspace directory")
    replay.add_argument("--output", type=Path)
    report = subparsers.add_parser("report", help="render a saved JSON report")
    report.add_argument("--input", required=True, type=Path)
    report.add_argument("--format", choices=("json", "markdown"), default="markdown")
    report.add_argument("--output", type=Path)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        manifest, tasks = load_manifest()
        if args.command == "validate":
            print(f"validated {len(tasks)} frozen tasks for {manifest['suite_id']} {manifest['suite_version']}")
            return 0
        if args.command == "dry-run":
            report = build_report(manifest, tasks, mode="dry-run")
            write_json(report, args.output)
            return 0
        if args.command == "replay":
            records = load_records(args.records.resolve(), tasks)
            workspace = args.workspace.resolve() if args.workspace else None
            report = build_report(manifest, tasks, mode="replay", records=records, workspace_root=workspace, run_acceptance_checks=workspace is not None)
            write_json(report, args.output)
            return 0
        if args.command == "report":
            report = read_json(args.input.resolve())
            if args.format == "json":
                write_json(report, args.output)
            else:
                rendered = render_markdown(report)
                if args.output is None:
                    print(rendered)
                else:
                    args.output.write_text(rendered, encoding="utf-8")
                    print(f"wrote {relative(args.output)}")
            return 0
        raise EvaluationError(f"unsupported command {args.command}")
    except EvaluationError as error:
        print(f"agent-token-economy error: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
