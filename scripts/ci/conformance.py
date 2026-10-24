#!/usr/bin/env python3
"""Validate and run Seloryn's source-owned public conformance matrix.

The matrix deliberately points at the existing independent probes. This file
only validates their coverage and provides a deterministic selector/orchestrator;
it does not replace the probes or manufacture results for unsupported features.
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import re
import subprocess
import sys
import time
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
MATRIX_PATH = ROOT / "tests" / "conformance_matrix.json"
STATUS_LABELS = {
    "design-only",
    "reserved",
    "experimental-implemented",
    "verified-preview",
    "supported-stable",
    "deprecated",
    "unsupported",
}
PLATFORM_LABELS = {"linux", "macos", "windows", "not-applicable"}
LOCAL_PLATFORMS = {"Windows": "windows", "Darwin": "macos", "Linux": "linux"}
HEADING_RE = re.compile(r"^#{1,6}\s+(.+?)\s*#*\s*$")


class MatrixError(ValueError):
    """A source-owned matrix contract is invalid."""


def load_matrix() -> dict[str, Any]:
    try:
        with MATRIX_PATH.open("r", encoding="utf-8") as handle:
            matrix = json.load(handle)
    except (OSError, json.JSONDecodeError) as error:
        raise MatrixError(f"cannot load {MATRIX_PATH}: {error}") from error
    if not isinstance(matrix, dict):
        raise MatrixError("matrix root must be an object")
    return matrix


def slugify_heading(value: str) -> str:
    value = re.sub(r"<[^>]+>", "", value).strip().lower()
    value = re.sub(r"[^\w\s-]", "", value, flags=re.UNICODE)
    return re.sub(r"[-\s]+", "-", value).strip("-")


def check_repo_path(raw: str, *, label: str) -> None:
    if not isinstance(raw, str) or not raw or Path(raw).is_absolute():
        raise MatrixError(f"{label} must be a non-empty repository-relative path")
    candidate = (ROOT / raw).resolve()
    try:
        candidate.relative_to(ROOT.resolve())
    except ValueError as error:
        raise MatrixError(f"{label} escapes the repository: {raw}") from error
    if not candidate.exists():
        raise MatrixError(f"{label} does not exist: {raw}")


def check_doc_link(raw: str, *, label: str) -> None:
    if not isinstance(raw, str) or raw.startswith(("http://", "https://", "#")):
        raise MatrixError(f"{label} must be a local repository link: {raw!r}")
    path_text, _, anchor = raw.partition("#")
    check_repo_path(path_text, label=label)
    if not anchor:
        return
    path = ROOT / path_text
    headings = set()
    for line in path.read_text(encoding="utf-8").splitlines():
        match = HEADING_RE.match(line)
        if match:
            headings.add(slugify_heading(match.group(1)))
    if anchor not in headings:
        raise MatrixError(f"{label} anchor does not exist: {raw}")


def validate_probe(probe: dict[str, Any], probe_ids: set[str]) -> None:
    probe_id = probe.get("id")
    if not isinstance(probe_id, str) or not probe_id:
        raise MatrixError("every probe needs a non-empty id")
    if probe_id in probe_ids:
        raise MatrixError(f"duplicate probe id: {probe_id}")
    probe_ids.add(probe_id)
    check_repo_path(probe.get("path", ""), label=f"probe {probe_id} path")
    probe_type = probe.get("type", "command")
    if probe_type not in {"command", "boundary"}:
        raise MatrixError(f"probe {probe_id} has unsupported type {probe_type!r}")
    suites = probe.get("suites")
    if not isinstance(suites, list) or not suites or not all(isinstance(item, str) and item for item in suites):
        raise MatrixError(f"probe {probe_id} must list at least one suite")
    platforms = probe.get("platforms")
    if not isinstance(platforms, list) or not platforms or not set(platforms) <= PLATFORM_LABELS:
        raise MatrixError(f"probe {probe_id} has invalid platforms")
    requirements = probe.get("requirements")
    if not isinstance(requirements, list) or not requirements or not all(isinstance(item, str) and item for item in requirements):
        raise MatrixError(f"probe {probe_id} must list requirements")
    if probe_type == "boundary":
        if "command" in probe:
            raise MatrixError(f"boundary probe {probe_id} must not define a command")
        must_contain = probe.get("must_contain")
        if not isinstance(must_contain, str) or not must_contain:
            raise MatrixError(f"boundary probe {probe_id} must state the documented boundary text")
        content = (ROOT / probe["path"]).read_text(encoding="utf-8")
        if must_contain not in content:
            raise MatrixError(f"boundary probe {probe_id} text is missing from {probe['path']}: {must_contain!r}")
        return
    command = probe.get("command")
    if not isinstance(command, list) or not command or not all(isinstance(item, str) and item for item in command):
        raise MatrixError(f"command probe {probe_id} must define a non-empty argv list")


def validate_capability(capability: dict[str, Any], capability_ids: set[str], probe_ids: set[str], statuses: set[str]) -> None:
    capability_id = capability.get("id")
    if not isinstance(capability_id, str) or not capability_id:
        raise MatrixError("every capability needs a non-empty id")
    if capability_id in capability_ids:
        raise MatrixError(f"duplicate capability id: {capability_id}")
    capability_ids.add(capability_id)
    if capability.get("status") not in statuses:
        raise MatrixError(f"capability {capability_id} has invalid status {capability.get('status')!r}")
    if not isinstance(capability.get("area"), str) or not capability["area"]:
        raise MatrixError(f"capability {capability_id} needs an area")
    if not isinstance(capability.get("claim"), str) or not capability["claim"]:
        raise MatrixError(f"capability {capability_id} needs a claim")
    implementation = capability.get("implementation")
    if not isinstance(implementation, list) or not all(isinstance(item, str) for item in implementation):
        raise MatrixError(f"capability {capability_id} has invalid implementation paths")
    for path in implementation:
        check_repo_path(path, label=f"capability {capability_id} implementation")
    for field in ("positive_probes", "negative_probes"):
        references = capability.get(field)
        if not isinstance(references, list):
            raise MatrixError(f"capability {capability_id} must define {field}")
        if capability["status"] in {"unsupported", "design-only", "reserved"} and field == "positive_probes" and references:
            raise MatrixError(f"unimplemented capability {capability_id} cannot have positive probes")
        for reference in references:
            if not isinstance(reference, str) or reference not in probe_ids:
                raise MatrixError(f"capability {capability_id} references unknown probe {reference!r}")
    if capability["status"] not in {"unsupported", "design-only", "reserved"} and not capability["positive_probes"]:
        raise MatrixError(f"implemented capability {capability_id} needs a positive probe")
    if not capability["negative_probes"]:
        raise MatrixError(f"capability {capability_id} needs a negative probe or explicit boundary")
    platforms = capability.get("platforms")
    if not isinstance(platforms, list) or not platforms or not set(platforms) <= PLATFORM_LABELS:
        raise MatrixError(f"capability {capability_id} has invalid platforms")
    requirements = capability.get("requirements")
    if not isinstance(requirements, list) or not requirements or not all(isinstance(item, str) and item for item in requirements):
        raise MatrixError(f"capability {capability_id} must list requirements")
    documentation = capability.get("documentation")
    if not isinstance(documentation, list) or not documentation:
        raise MatrixError(f"capability {capability_id} needs documentation links")
    for link in documentation:
        check_doc_link(link, label=f"capability {capability_id} documentation")
    limits = capability.get("limits")
    if not isinstance(limits, list) or not limits or not all(isinstance(item, str) and item for item in limits):
        raise MatrixError(f"capability {capability_id} must state limits")


def validate_matrix(matrix: dict[str, Any]) -> tuple[dict[str, dict[str, Any]], dict[str, dict[str, Any]]]:
    if matrix.get("schema") != "seloryn.conformance.v1":
        raise MatrixError("unsupported matrix schema")
    if matrix.get("schema_version") != 1:
        raise MatrixError("unsupported matrix schema version")
    statuses = set(matrix.get("status_labels", []))
    if statuses != STATUS_LABELS:
        raise MatrixError("status_labels must exactly match the public contract vocabulary")
    if set(matrix.get("platform_labels", [])) != PLATFORM_LABELS:
        raise MatrixError("platform_labels must exactly match the runner vocabulary")
    probes_raw = matrix.get("probes")
    capabilities_raw = matrix.get("capabilities")
    if not isinstance(probes_raw, list) or not probes_raw:
        raise MatrixError("matrix needs probes")
    if not isinstance(capabilities_raw, list) or not capabilities_raw:
        raise MatrixError("matrix needs capabilities")
    probes: dict[str, dict[str, Any]] = {}
    probe_ids: set[str] = set()
    for probe in probes_raw:
        if not isinstance(probe, dict):
            raise MatrixError("probe entries must be objects")
        validate_probe(probe, probe_ids)
        probes[probe["id"]] = probe
    capabilities: dict[str, dict[str, Any]] = {}
    capability_ids: set[str] = set()
    for capability in capabilities_raw:
        if not isinstance(capability, dict):
            raise MatrixError("capability entries must be objects")
        validate_capability(capability, capability_ids, probe_ids, statuses)
        capabilities[capability["id"]] = capability
    return probes, capabilities


def current_platform() -> str:
    return LOCAL_PLATFORMS.get(platform.system(), platform.system().lower())


def selected_probe_ids(
    probes: dict[str, dict[str, Any]],
    capabilities: dict[str, dict[str, Any]],
    suite: str | None,
    capability_ids: list[str],
) -> list[str]:
    if capability_ids:
        selected: set[str] = set()
        for capability_id in capability_ids:
            if capability_id not in capabilities:
                raise MatrixError(f"unknown capability: {capability_id}")
            capability = capabilities[capability_id]
            selected.update(capability["positive_probes"])
            selected.update(capability["negative_probes"])
    else:
        selected = {
            probe_id
            for probe_id, probe in probes.items()
            if suite is None or suite in probe["suites"]
        }
    return [probe_id for probe_id in probes if probe_id in selected]


def display_list(probes: dict[str, dict[str, Any]], capabilities: dict[str, dict[str, Any]]) -> None:
    print(f"Capabilities: {len(capabilities)}")
    for capability in capabilities.values():
        positive = ", ".join(capability["positive_probes"]) or "none"
        negative = ", ".join(capability["negative_probes"]) or "none"
        print(f"{capability['id']}: {capability['status']} | + {positive} | - {negative}")
    print(f"\nProbes: {len(probes)}")
    for probe in probes.values():
        probe_type = probe.get("type", "command")
        print(f"{probe['id']}: {probe_type} | {', '.join(probe['suites'])} | {probe['path']}")


def resolved_command(probe: dict[str, Any]) -> list[str]:
    command = list(probe["command"])
    if command[0] in {"python", "python3"}:
        return [sys.executable, *command[1:]]
    if sys.platform == "win32" and command[0] == "npm":
        command[0] = "npm.cmd"
    return command


def run_probe(probe: dict[str, Any], host_platform: str) -> dict[str, Any]:
    started = time.monotonic()
    result: dict[str, Any] = {"id": probe["id"], "path": probe["path"]}
    if probe.get("type", "command") == "boundary":
        result.update({"status": "documented-boundary", "duration_seconds": 0.0})
        return result
    if host_platform not in probe["platforms"]:
        result.update({"status": "skipped-platform", "duration_seconds": 0.0})
        return result
    command = resolved_command(probe)
    environment = None
    if sys.platform == "win32" and command[0].lower() in {"powershell", "powershell.exe"}:
        # Some host shells can prepend a PowerShell 7 utility module
        # directory whose shadowed Microsoft.PowerShell.Utility lacks the
        # Windows PowerShell Get-FileHash cmdlet used by the installer proof.
        # CI's normal environment does not need this, but an explicit module
        # path keeps the declared Windows probe reproducible locally too.
        environment = os.environ.copy()
        environment["PSModulePath"] = ";".join(
            path
            for path in (
                r"C:\Windows\System32\WindowsPowerShell\v1.0\Modules",
                r"C:\Program Files\WindowsPowerShell\Modules",
            )
            if Path(path).is_dir()
        )
    try:
        completed = subprocess.run(
            command,
            cwd=ROOT,
            text=True,
            encoding="utf-8",
            errors="replace",
            capture_output=True,
            check=False,
            timeout=600,
            env=environment,
        )
    except (OSError, subprocess.SubprocessError) as error:
        result.update({"status": "error", "error": str(error)})
    else:
        result.update({
            "status": "passed" if completed.returncode == 0 else "failed",
            "returncode": completed.returncode,
            "stdout_tail": completed.stdout[-2000:],
            "stderr_tail": completed.stderr[-2000:],
        })
    result["duration_seconds"] = round(time.monotonic() - started, 3)
    return result


def print_result(result: dict[str, Any]) -> None:
    status = result["status"]
    duration = result.get("duration_seconds", 0.0)
    print(f"[{status.upper()}] {result['id']} ({duration:.3f}s)")
    if status in {"failed", "error"}:
        if result.get("stdout_tail"):
            print("stdout tail:")
            print(result["stdout_tail"].rstrip())
        if result.get("stderr_tail"):
            print("stderr tail:")
            print(result["stderr_tail"].rstrip())
        if result.get("error"):
            print(f"error: {result['error']}")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--validate", action="store_true", help="validate the matrix and all referenced local paths")
    parser.add_argument("--list", action="store_true", help="list capabilities and probes")
    parser.add_argument("--run", action="store_true", help="run selected executable probes")
    parser.add_argument("--suite", help="select a probe suite, such as core, editor, hardware, release or benchmark")
    parser.add_argument("--capability", action="append", default=[], help="run probes referenced by this capability; repeatable")
    parser.add_argument("--json", action="store_true", help="emit run results as JSON after human-readable output")
    return parser


def main() -> int:
    args = build_parser().parse_args()
    if not (args.validate or args.list or args.run):
        args.validate = True
    try:
        matrix = load_matrix()
        probes, capabilities = validate_matrix(matrix)
        print(f"Validated {len(capabilities)} capabilities and {len(probes)} probes from {MATRIX_PATH.relative_to(ROOT)}")
        if args.list:
            display_list(probes, capabilities)
        if not args.run:
            return 0
        if args.suite and args.capability:
            raise MatrixError("--suite and --capability cannot be combined")
        selected = selected_probe_ids(probes, capabilities, args.suite, args.capability)
        if not selected:
            raise MatrixError("selection matched no probes")
        host_platform = current_platform()
        print(f"Running {len(selected)} probes on {host_platform}")
        results = []
        for probe_id in selected:
            result = run_probe(probes[probe_id], host_platform)
            results.append(result)
            print_result(result)
        if args.json:
            print(json.dumps({"platform": host_platform, "results": results}, indent=2, sort_keys=True))
        failures = [item for item in results if item["status"] in {"failed", "error"}]
        return 1 if failures else 0
    except MatrixError as error:
        print(f"conformance matrix error: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
