"""Capture sanitized, machine-readable summaries for public CI evidence.

The wrapped command remains a blocking correctness gate: its exit status is
returned unchanged.  Only structured summaries are written to the evidence
directory; raw stdout/stderr is never persisted.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import platform
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any, Sequence


ROOT = Path(__file__).resolve().parents[2]
SCHEMA = "seloryn.ci-evidence.v1"
MAX_SUMMARY_CHARS = 2_000
SAFE_NAME = re.compile(r"^[a-z0-9][a-z0-9-]{0,63}$")
HEX_SHA = re.compile(r"^[0-9a-f]{40}$")
WINDOWS_PATH = re.compile(r"(?i)(?<![a-z0-9])(?:[a-z]:[\\/]|\\\\)[^\s\"'<>]+")
POSIX_PATH = re.compile(
    r"(?<![a-z0-9])/(?:home|Users|private|tmp|runner|workspaces|opt|var)/[^\s\"'<>]+"
)
SECRET_VALUE = re.compile(
    r"(?i)(\b(?:token|secret|password|authorization|api[_-]?key)\b\s*[:=]\s*)[^\s,;]+"
)

DEFAULT_EXPECTED = (
    "ci-evidence-helper",
    "n1-cli",
    "n2-cli",
    "n3a-cli",
    "n3b-tensor",
    "public-projects",
    "project-manifest",
    "local-import",
    "cpu-mlp-training",
    "training-stream",
    "self-hosted-mlp",
    "self-hosted-language-model",
    "auditable-local-inference",
    "performance",
    "operational-reliability",
    "job-lifecycle",
    "job-lease",
    "job-recovery",
    "job-concurrency",
    "job-preflight",
    "language-model-comparison",
    "gpu-compute",
    "gpu-training",
)


def _known_roots() -> tuple[str, ...]:
    values = {
        str(ROOT),
        str(ROOT.parent),
        str(Path.cwd()),
        tempfile.gettempdir(),
    }
    for key in ("GITHUB_WORKSPACE", "RUNNER_TEMP", "RUNNER_WORKSPACE", "TEMP", "TMP"):
        value = os.environ.get(key)
        if value:
            values.add(value)
    return tuple(sorted(values, key=len, reverse=True))


def _basename(value: str) -> str:
    name = re.split(r"[\\/]", value.rstrip("\\/"))[-1]
    return name or "<path>"


def sanitize_text(value: str) -> str:
    """Redact paths, workspace roots and obvious credential-shaped values."""

    result = value
    for root in _known_roots():
        if root:
            result = result.replace(root, "<workspace>")
            result = result.replace(root.replace("\\", "/"), "<workspace>")
    result = SECRET_VALUE.sub(r"\1<redacted>", result)
    result = WINDOWS_PATH.sub("<absolute-path>", result)
    result = POSIX_PATH.sub("<absolute-path>", result)
    return result


def _looks_absolute(value: str) -> bool:
    return bool(
        re.match(r"(?i)^[a-z]:[\\/]", value)
        or value.startswith(("/", "\\\\"))
        or any(root and value.startswith(root) for root in _known_roots())
    )


def sanitize_value(value: Any, key: str = "") -> Any:
    """Return JSON-compatible data with path-bearing fields made portable."""

    if isinstance(value, dict):
        return {str(item_key): sanitize_value(item, str(item_key)) for item_key, item in value.items()}
    if isinstance(value, list):
        return [sanitize_value(item, key) for item in value]
    if isinstance(value, str):
        lowered = key.lower()
        if lowered in {
            "token",
            "secret",
            "password",
            "authorization",
            "api_key",
            "api-key",
        }:
            return "<redacted>"
        if lowered in {"binary", "baseline", "executable"} or lowered.endswith("_path"):
            if _looks_absolute(value):
                return _basename(value)
        return sanitize_text(value)
    return value


def sanitize_command(command: Sequence[str]) -> list[str]:
    result: list[str] = []
    redact_next = False
    for token in command:
        if redact_next:
            result.append("<baseline-binary>")
            redact_next = False
            continue
        if token == "--baseline":
            result.append(token)
            redact_next = True
        elif _looks_absolute(token):
            result.append(f"<{_basename(token)}>")
        else:
            result.append(sanitize_text(token))
    if redact_next:
        result.append("<missing-value>")
    return result


def _first_json(value: str) -> Any | None:
    decoder = json.JSONDecoder()
    start = len(value) - len(value.lstrip())
    if start == len(value):
        return None
    try:
        parsed, _ = decoder.raw_decode(value, start)
    except json.JSONDecodeError:
        return None
    return parsed


def summarize_output(stdout: str, stderr: str) -> dict[str, Any]:
    """Extract a bounded summary without retaining raw process output."""

    parsed = _first_json(stdout)
    if parsed is not None:
        return {"format": "json", "value": sanitize_value(parsed)}

    stdout_lines = [sanitize_text(line) for line in stdout.splitlines() if line.strip()]
    stderr_lines = [sanitize_text(line) for line in stderr.splitlines() if line.strip()]
    summary: dict[str, Any] = {"format": "text"}
    if stdout_lines:
        summary["stdout_tail"] = "\n".join(stdout_lines[-10:])[-MAX_SUMMARY_CHARS:]
    if stderr_lines:
        summary["stderr_tail"] = "\n".join(stderr_lines[-10:])[-MAX_SUMMARY_CHARS:]
    if not stdout_lines and not stderr_lines:
        summary["message"] = "command produced no textual output"
    return summary


def _commit() -> str:
    candidate = os.environ.get("GITHUB_SHA", "").lower()
    if HEX_SHA.fullmatch(candidate):
        return candidate
    try:
        result = subprocess.run(
            ["git", "-C", str(ROOT), "rev-parse", "HEAD"],
            capture_output=True,
            text=True,
            check=True,
        )
    except (OSError, subprocess.SubprocessError):
        return "unknown"
    candidate = result.stdout.strip().lower()
    return candidate if HEX_SHA.fullmatch(candidate) else "unknown"


def _metadata() -> dict[str, Any]:
    metadata: dict[str, Any] = {
        "source_commit": _commit(),
        "runner_os": os.environ.get("RUNNER_OS") or platform.system(),
        "python": platform.python_version(),
        "ci": os.environ.get("GITHUB_ACTIONS", "").lower() == "true",
    }
    for output_key, env_key in (("run_id", "GITHUB_RUN_ID"), ("run_attempt", "GITHUB_RUN_ATTEMPT")):
        value = os.environ.get(env_key)
        if value and value.isdigit():
            metadata[output_key] = int(value)
    return metadata


def _write_json(path: Path, payload: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.tmp-{os.getpid()}")
    try:
        temporary.write_text(
            json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
            newline="\n",
        )
        os.replace(temporary, path)
    finally:
        try:
            temporary.unlink()
        except FileNotFoundError:
            pass


def run_probe(output_dir: Path, name: str, kind: str, command: Sequence[str]) -> int:
    if not SAFE_NAME.fullmatch(name):
        raise ValueError(f"invalid evidence name: {name!r}")
    if not command:
        raise ValueError("the wrapped command is required after --")

    started = time.perf_counter()
    completed = subprocess.run(
        list(command),
        cwd=ROOT,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        check=False,
    )
    duration = time.perf_counter() - started
    status = "passed" if completed.returncode == 0 else "failed"
    record = {
        "schema": SCHEMA,
        "name": name,
        "kind": kind,
        "status": status,
        "exit_code": completed.returncode,
        "duration_seconds": round(duration, 6),
        "command": sanitize_command(command),
        "summary": summarize_output(completed.stdout, completed.stderr),
    }
    _write_json(output_dir / f"{name}.json", record)

    if completed.stdout:
        sys.stdout.write(sanitize_text(completed.stdout))
    if completed.stderr:
        sys.stderr.write(sanitize_text(completed.stderr))
    return completed.returncode


def finalize(output_dir: Path, expected: Sequence[str]) -> int:
    output_dir.mkdir(parents=True, exist_ok=True)
    records: dict[str, dict[str, Any]] = {}
    for path in sorted(output_dir.glob("*.json")):
        if path.name == "manifest.json":
            continue
        try:
            record = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            continue
        if isinstance(record, dict) and record.get("schema") == SCHEMA and isinstance(record.get("name"), str):
            records[record["name"]] = sanitize_value(record)

    ordered: list[dict[str, Any]] = []
    missing: list[str] = []
    for name in expected:
        record = records.get(name)
        if record is None:
            missing.append(name)
            ordered.append({"name": name, "status": "not-run"})
        else:
            ordered.append(record)

    failed = [record["name"] for record in ordered if record.get("status") == "failed"]
    status = "passed" if not missing and not failed else "incomplete"
    manifest = {
        "schema": SCHEMA,
        "manifest": "ci-summary",
        "status": status,
        "generated_at_utc": dt.datetime.now(dt.timezone.utc).isoformat().replace("+00:00", "Z"),
        "expected": list(expected),
        "missing": missing,
        "failed": failed,
        "retention": {
            "raw_stdout_stderr_persisted": False,
            "absolute_workspace_paths_persisted": False,
            "credential_shaped_values_persisted": False,
        },
        "environment": _metadata(),
        "records": ordered,
    }
    _write_json(output_dir / "manifest.json", manifest)
    return 0 if status == "passed" else 1


def parse_args(argv: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="action", required=True)

    run_parser = subparsers.add_parser("run", help="run one blocking probe and save its sanitized summary")
    run_parser.add_argument("--output-dir", type=Path, required=True)
    run_parser.add_argument("--name", required=True)
    run_parser.add_argument("--kind", choices=("benchmark", "probe", "test"), required=True)
    run_parser.add_argument("command", nargs=argparse.REMAINDER)

    finalize_parser = subparsers.add_parser("finalize", help="write the complete evidence manifest")
    finalize_parser.add_argument("--output-dir", type=Path, required=True)
    finalize_parser.add_argument("--expected", nargs="+", default=DEFAULT_EXPECTED)
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(argv or sys.argv[1:])
    if args.action == "run":
        command = list(args.command)
        if command and command[0] == "--":
            command = command[1:]
        return run_probe(args.output_dir, args.name, args.kind, command)
    return finalize(args.output_dir, args.expected)


if __name__ == "__main__":
    raise SystemExit(main())
