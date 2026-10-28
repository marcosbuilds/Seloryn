"""Run the Seloryn Benchmark Suite with independent stdlib references.

The runner measures each implementation separately, checks correctness before
reporting timings, and writes a machine-readable report plus a compact Markdown
view. It intentionally does not compute an aggregate score or a general
superiority claim.
"""

from __future__ import annotations

import argparse
import ast
import hashlib
import importlib.util
import json
import math
import platform
import re
import statistics
import subprocess
import sys
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path
from types import ModuleType
from typing import Any, Callable

from schema_validator import load_and_validate, validate


ROOT = Path(__file__).resolve().parents[1]
BENCHMARKS = ROOT / "benchmarks"
MANIFEST_PATH = BENCHMARKS / "manifest.json"
MANIFEST_SCHEMA = BENCHMARKS / "schema" / "manifest.schema.json"
REPORT_SCHEMA = BENCHMARKS / "schema" / "report.schema.json"
DEFAULT_BINARY = ROOT / "target" / "release" / ("seloryn.exe" if sys.platform == "win32" else "seloryn")
FORBIDDEN_REFERENCE_IMPORTS = ("numpy", "torch", "tensorflow", "pandas", "scipy", "sklearn", "jax")
TENSOR_PATTERN = re.compile(r"Tensor\[F32, shape=(?P<shape>\[[^]]*\]), values=\[(?P<values>.*)\]\]", re.DOTALL)
DIAGNOSTIC_PATTERN = re.compile(r"SEL-[A-Z]\d{3}")


class BlockedCase(RuntimeError):
    """The environment cannot exercise a candidate case."""


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def canonical_hash(value: Any) -> str:
    encoded = json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return sha256_bytes(encoded)


def relative_path(path: Path) -> str:
    try:
        return path.resolve().relative_to(ROOT.resolve()).as_posix()
    except ValueError:
        return path.as_posix()


def command_display(command: list[str]) -> list[str]:
    return [relative_path(Path(part)) if index == 0 else part for index, part in enumerate(command)]


def run_process(command: list[str], *, stdin: str | None, timeout: float) -> dict[str, Any]:
    started = time.perf_counter_ns()
    try:
        process = subprocess.run(
            command,
            cwd=ROOT,
            input=stdin,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=timeout,
            check=False,
        )
    except subprocess.TimeoutExpired as error:
        elapsed_ms = (time.perf_counter_ns() - started) / 1_000_000.0
        stdout = error.stdout if isinstance(error.stdout, str) else ""
        stderr = error.stderr if isinstance(error.stderr, str) else ""
        return {
            "exit_code": None,
            "stdout": stdout,
            "stderr": stderr,
            "elapsed_ms": elapsed_ms,
            "timed_out": True,
        }
    elapsed_ms = (time.perf_counter_ns() - started) / 1_000_000.0
    return {
        "exit_code": process.returncode,
        "stdout": process.stdout,
        "stderr": process.stderr,
        "elapsed_ms": elapsed_ms,
        "timed_out": False,
    }


def result_value(stdout: str) -> Any:
    envelope = json.loads(stdout)
    value = envelope["value"]
    if isinstance(value, dict) and "ok" in value:
        if value["ok"] is not True:
            raise ValueError(f"Seloryn returned an error result: {value!r}")
        value = value["value"]
    return value


def record_fields(value: Any) -> dict[str, Any]:
    if isinstance(value, dict) and value.get("type") == "record":
        fields = value.get("fields")
        if isinstance(fields, dict):
            return fields
    if isinstance(value, dict) and value.get("type") == "enum":
        fields = value.get("fields", [])
        if fields and isinstance(fields[0], dict):
            return record_fields(fields[0])
    raise ValueError(f"expected a Seloryn record result, got {value!r}")


def parse_tensor(stdout: str) -> dict[str, Any]:
    match = TENSOR_PATTERN.fullmatch(stdout.strip())
    if match is None:
        raise ValueError(f"not a Tensor[F32] rendering: {stdout!r}")
    shape = ast.literal_eval(match.group("shape"))
    values_text = match.group("values").strip()
    values = [] if not values_text else [float(item) for item in values_text.split(", ")]
    return {"status": "ok", "shape": shape, "values": values}


def close_enough(left: float, right: float, tolerance: float) -> bool:
    return math.isclose(left, right, rel_tol=tolerance, abs_tol=tolerance)


def output_digest(process: dict[str, Any]) -> str:
    return sha256_bytes((process["stdout"] + "\0" + process["stderr"]).encode("utf-8"))


def common_sample(index: int, process: dict[str, Any], correct: bool) -> dict[str, Any]:
    return {
        "index": index,
        "elapsed_ms": round(process["elapsed_ms"], 6),
        "exit_code": process["exit_code"],
        "correct": correct,
        "output_sha256": output_digest(process),
        "timed_out": process["timed_out"],
    }


def reference_sample(index: int, started_ns: int, value: Any, correct: bool = True, error: str | None = None) -> dict[str, Any]:
    sample = {
        "index": index,
        "elapsed_ms": round((time.perf_counter_ns() - started_ns) / 1_000_000.0, 6),
        "exit_code": 0 if error is None else 1,
        "correct": correct,
        "output_sha256": canonical_hash(value) if error is None else sha256_bytes(error.encode("utf-8")),
        "timed_out": False,
    }
    if error is not None:
        sample["error"] = error
    return sample


def stats(samples: list[dict[str, Any]]) -> dict[str, Any]:
    values = [sample["elapsed_ms"] for sample in samples]
    if not values:
        return {"samples_ms": [], "median_ms": None, "p95_ms": None}
    ordered = sorted(values)
    p95 = ordered[-1] if len(ordered) == 1 else statistics.quantiles(values, n=20, method="inclusive")[18]
    return {
        "samples_ms": values,
        "median_ms": round(statistics.median(values), 6),
        "p95_ms": round(p95, 6),
    }


def import_reference(path: Path) -> tuple[ModuleType, Callable[[Path], dict[str, Any]]]:
    for forbidden in FORBIDDEN_REFERENCE_IMPORTS:
        if re.search(rf"^\s*(?:from|import)\s+{re.escape(forbidden)}(?:\.|\s|$)", path.read_text(encoding="utf-8"), re.MULTILINE):
            raise BlockedCase(f"reference imports unavailable dependency {forbidden}")
    module_name = f"seloryn_benchmark_reference_{path.stem}"
    spec = importlib.util.spec_from_file_location(module_name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load reference module {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    reference = getattr(module, "reference", None)
    if not callable(reference):
        raise RuntimeError(f"reference module {path} does not expose reference(repo_root)")
    return module, reference


def candidate_process(case_id: str, binary: Path, timeout: float) -> tuple[dict[str, Any], list[str]]:
    if not binary.is_file():
        raise BlockedCase(f"Seloryn binary not found: {relative_path(binary)}")
    if case_id == "BENCH-01":
        project = ROOT / "examples" / "projects" / "json-config"
        command = [str(binary), "run", str(project), "--format=json", "--log=off"]
        return run_process(command, stdin=None, timeout=timeout), command
    if case_id == "BENCH-02":
        project = ROOT / "examples" / "projects" / "json-array-pipeline"
        command = [str(binary), "run", str(project), "--format=json", "--log=off"]
        return run_process(command, stdin=None, timeout=timeout), command
    if case_id == "BENCH-03":
        fixture = BENCHMARKS / "fixtures" / "bench-03-numerical-computation.sely"
        command = [str(binary), "run", str(fixture)]
        return run_process(command, stdin=None, timeout=timeout), command
    if case_id == "BENCH-04":
        with tempfile.TemporaryDirectory(prefix="seloryn-bench-mlp-") as directory:
            project = Path(directory) / "self-hosted-mlp"
            import shutil

            shutil.copytree(ROOT / "examples" / "projects" / "self-hosted-mlp", project)
            command = [str(binary), "run", str(project), "--format=json", "--log=off"]
            return run_process(command, stdin=None, timeout=timeout), [str(binary), "run", "<temporary-project>", "--format=json", "--log=off"]
    if case_id == "BENCH-05":
        with tempfile.TemporaryDirectory(prefix="seloryn-bench-language-") as directory:
            project = Path(directory) / "self-hosted-language-model"
            import shutil

            shutil.copytree(ROOT / "examples" / "projects" / "self-hosted-language-model", project)
            command = [str(binary), "run", str(project), "--format=json", "--log=off"]
            return run_process(command, stdin='{"mode":"train","context_size":4,"steps":160}', timeout=timeout), [str(binary), "run", "<temporary-project>", "--format=json", "--log=off"]
    if case_id == "BENCH-06":
        fixture = BENCHMARKS / "fixtures" / "bench-06-error-handling.sely"
        command = [str(binary), "run", str(fixture)]
        return run_process(command, stdin=None, timeout=timeout), command
    raise ValueError(f"unknown benchmark case {case_id}")


def candidate_observation(case_id: str, process: dict[str, Any], reference_value: dict[str, Any]) -> tuple[Any, bool, str | None]:
    if process["timed_out"]:
        return {"status": "timeout"}, False, "candidate timed out"
    if case_id == "BENCH-01":
        try:
            value = result_value(process["stdout"])
            expected = reference_value["value"]
            correct = process["exit_code"] == 0 and value == expected
            return {"status": "ok", "value": value}, correct, None if correct else f"expected {expected!r}, got {value!r}"
        except (KeyError, TypeError, ValueError, json.JSONDecodeError) as error:
            return {"status": "invalid-output"}, False, str(error)
    if case_id == "BENCH-02":
        try:
            value = float(result_value(process["stdout"]))
            expected = float(reference_value["value"])
            correct = process["exit_code"] == 0 and close_enough(value, expected, 1e-6)
            return {"status": "ok", "value": value}, correct, None if correct else f"expected {expected}, got {value}"
        except (KeyError, TypeError, ValueError, json.JSONDecodeError) as error:
            return {"status": "invalid-output"}, False, str(error)
    if case_id == "BENCH-03":
        try:
            value = parse_tensor(process["stdout"])
            expected_values = reference_value["values"]
            correct = (
                process["exit_code"] == 0
                and value["shape"] == reference_value["shape"]
                and len(value["values"]) == len(expected_values)
                and all(close_enough(actual, expected, 2e-4) for actual, expected in zip(value["values"], expected_values))
            )
            return value, correct, None if correct else "tensor shape or values differ from the stdlib oracle"
        except (KeyError, TypeError, ValueError, SyntaxError) as error:
            return {"status": "invalid-output"}, False, str(error)
    if case_id in {"BENCH-04", "BENCH-05"}:
        try:
            fields = record_fields(result_value(process["stdout"]))
            expected = reference_value
            required = ["initial_loss", "final_loss"] if case_id == "BENCH-04" else ["initial_loss", "final_loss", "test_loss", "test_accuracy"]
            observed = {key: fields[key] for key in required}
            if case_id == "BENCH-04":
                observed.update({key: fields[key] for key in ("evaluation_loss", "reloaded_loss", "prediction")})
                mismatches = []
                for key in ("final_loss", "evaluation_loss", "reloaded_loss", "prediction"):
                    if not close_enough(fields[key], expected[key], 2e-3):
                        mismatches.append(f"{key}: candidate={fields[key]!r}, reference={expected[key]!r}")
                correct = (
                    process["exit_code"] == 0
                    and fields["initial_loss"] > fields["final_loss"] > 0.0
                    and not mismatches
                )
                error = None if correct else "; ".join(mismatches) or "Seloryn did not produce a successful MLP result"
            else:
                observed["generated"] = fields["generated"]
                correct = (
                    process["exit_code"] == 0
                    and fields["initial_loss"] > fields["final_loss"] > 0.0
                    and close_enough(fields["final_loss"], expected["final_loss"], 2e-3)
                    and close_enough(fields["test_loss"], expected["test_loss"], 2e-3)
                    and close_enough(fields["test_accuracy"], expected["test_accuracy"], 1e-6)
                    and fields["generated"] == expected["generated"]
                )
                error = None if correct else "Seloryn report is outside the declared numerical/model contract"
            return observed, correct, error
        except (KeyError, TypeError, ValueError, json.JSONDecodeError) as error:
            return {"status": "invalid-output"}, False, str(error)
    if case_id == "BENCH-06":
        codes = DIAGNOSTIC_PATTERN.findall(process["stderr"] + process["stdout"])
        code = codes[0] if codes else None
        correct = process["exit_code"] == 65 and code == "SEL-R210" and process["stdout"] == ""
        observation = {"status": "error" if process["exit_code"] else "ok", "code": code, "returncode": process["exit_code"]}
        return observation, correct, None if correct else f"expected exit 65, empty stdout and SEL-R210, got {observation!r}"
    raise ValueError(case_id)


def measure_candidate(case_id: str, binary: Path, reference_value: dict[str, Any], samples: int, warmup: int, timeout: float) -> dict[str, Any]:
    for _ in range(warmup):
        candidate_process(case_id, binary, timeout)
    measured: list[dict[str, Any]] = []
    observations: list[Any] = []
    errors: list[str] = []
    command: list[str] | None = None
    for index in range(samples):
        process, command = candidate_process(case_id, binary, timeout)
        observation, correct, error = candidate_observation(case_id, process, reference_value)
        sample = common_sample(index, process, correct)
        if error is not None:
            sample["error"] = error
            errors.append(error)
        measured.append(sample)
        observations.append(observation)
    return {
        "implementation": "Seloryn",
        "status": "passed" if all(sample["correct"] for sample in measured) else "failed",
        "command": command_display(command or []),
        "samples": measured,
        **stats(measured),
        "observations": observations[-1] if observations else None,
        "errors": errors,
    }


def measure_reference(reference: Callable[[Path], dict[str, Any]], samples: int, warmup: int) -> tuple[dict[str, Any], dict[str, Any] | None]:
    for _ in range(warmup):
        reference(ROOT)
    measured: list[dict[str, Any]] = []
    last_value: dict[str, Any] | None = None
    for index in range(samples):
        started = time.perf_counter_ns()
        try:
            value = reference(ROOT)
            if not isinstance(value, dict):
                raise TypeError("reference must return a dictionary")
            last_value = value
            measured.append(reference_sample(index, started, value))
        except Exception as error:  # reference failure is evidence, not a runner crash
            measured.append(reference_sample(index, started, {}, correct=False, error=repr(error)))
    return {
        "implementation": "Python stdlib reference",
        "status": "passed" if all(sample["correct"] for sample in measured) else "failed",
        "samples": measured,
        **stats(measured),
        "observations": last_value,
        "errors": [sample["error"] for sample in measured if "error" in sample],
    }, last_value


def package_version() -> str | None:
    match = re.search(r'^version\s*=\s*"([^"]+)"', (ROOT / "Cargo.toml").read_text(encoding="utf-8"), re.MULTILINE)
    return match.group(1) if match else None


def language_version() -> str | None:
    version_path = ROOT / "VERSION.toml"
    if not version_path.is_file():
        return None
    match = re.search(r'^language\s*=\s*"([^"]+)"', version_path.read_text(encoding="utf-8"), re.MULTILINE)
    return match.group(1) if match else None


def command_version(binary: Path) -> str | None:
    if not binary.is_file():
        return None
    try:
        process = subprocess.run([str(binary), "--version"], cwd=ROOT, capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=5, check=False)
    except (OSError, subprocess.SubprocessError):
        return None
    text = (process.stdout or process.stderr).strip()
    return text or None


def environment(binary: Path) -> dict[str, Any]:
    try:
        commit = subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT, capture_output=True, text=True, encoding="utf-8", check=True).stdout.strip()
        clean = not subprocess.run(["git", "status", "--porcelain"], cwd=ROOT, capture_output=True, text=True, encoding="utf-8", check=True).stdout.strip()
    except (OSError, subprocess.SubprocessError):
        commit, clean = None, None
    def tool_version(name: str) -> str | None:
        try:
            return subprocess.run([name, "--version"], capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=5, check=False).stdout.strip() or None
        except (OSError, subprocess.SubprocessError):
            return None
    return {
        "workspace": "Seloryn",
        "git_commit": commit,
        "git_worktree_clean": clean,
        "language_version": language_version(),
        "runtime_package_version": package_version(),
        "binary": relative_path(binary),
        "binary_sha256": sha256_file(binary) if binary.is_file() else None,
        "binary_version": command_version(binary),
        "python_version": platform.python_version(),
        "python_implementation": platform.python_implementation(),
        "python_dependencies": "stdlib-only references; no NumPy or ML framework required",
        "rustc": tool_version("rustc"),
        "cargo": tool_version("cargo"),
        "platform": platform.platform(),
        "system": platform.system(),
        "release": platform.release(),
        "machine": platform.machine(),
        "processor": platform.processor(),
        "cpu_count": __import__("os").cpu_count(),
        "network": False,
        "gpu": False,
    }


def comparison(candidate: dict[str, Any], reference: dict[str, Any]) -> dict[str, Any]:
    candidate_median = candidate.get("median_ms")
    reference_median = reference.get("median_ms")
    if candidate_median is None or reference_median is None or reference_median == 0:
        return {"status": "not-comparable", "reason": "missing timing or zero reference median"}
    delta = candidate_median - reference_median
    return {
        "status": "descriptive-only",
        "candidate_minus_reference_ms": round(delta, 6),
        "candidate_over_reference": round(candidate_median / reference_median, 6),
        "direction": "gain" if delta < 0 else "loss" if delta > 0 else "tie",
        "scope": "this fixture, this process boundary, this host",
    }


def case_limitations(case_id: str, *, status: str | None = None) -> list[str]:
    common = [
        "The timing is wall-clock process time and is not a claim about general Seloryn performance.",
        "The Python reference is an independent correctness oracle, not a competing production runtime.",
    ]
    if case_id in {"BENCH-04", "BENCH-05"}:
        common.append("The model workload is intentionally tiny and CPU-only; it does not represent framework-scale training or transformer serving.")
    if case_id == "BENCH-04" and status == "failed":
        common.append("The current run records a failed independent numerical-equivalence check for the MLP metrics; this remains an evidence gap, not a pass or a general performance claim.")
    if case_id == "BENCH-06":
        common.append("Failure timing is diagnostic evidence; lower error latency is not a quality score.")
    return common


def markdown_report(report: dict[str, Any]) -> str:
    lines = [
        "# Seloryn Benchmark Suite report",
        "",
        f"- Commit: `{report['environment'].get('git_commit')}`",
        f"- Started: `{report['run']['started_at_utc']}`",
        f"- Samples: `{report['run']['samples']}`; warm-up: `{report['run']['warmup']}`",
        f"- Binary: `{report['run']['binary']}`",
        f"- Summary: `{report['summary']}`",
        "",
        "| Case | Status | Seloryn median (ms) | Python median (ms) | Direction | Correctness |",
        "|---|---|---:|---:|---|---|",
    ]
    for case in report["cases"]:
        candidate = case.get("candidate", {})
        reference = case.get("reference", {})
        relative = case.get("comparison", {})
        lines.append(
            f"| {case['id']} {case['name']} | {case['status']} | "
            f"{candidate.get('median_ms', '—')} | {reference.get('median_ms', '—')} | "
            f"{relative.get('direction', '—')} | {case.get('correctness', {}).get('status', '—')} |"
        )
    lines.extend(
        [
            "",
            "## Interpretation",
            "",
            "Results are per-case descriptive evidence. A direction marked `gain` or `loss` is recorded only for that fixture and run configuration; it is not an aggregate or general superiority claim.",
            "",
            "## Limitations",
            "",
            "- Correctness is checked independently from timing and includes expected failure behavior.",
            "- GPU, network, NumPy and external ML frameworks are not required or exercised.",
            "- MLP and language-model cases are bounded CPU previews with tiny repository fixtures.",
        ]
    )
    return "\n".join(lines) + "\n"


def run(args: argparse.Namespace) -> dict[str, Any]:
    manifest = load_and_validate(MANIFEST_PATH, MANIFEST_SCHEMA)
    binary = args.binary.resolve()
    started_at = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
    selected = set(args.case or [item["id"] for item in manifest["cases"]])
    cases: list[dict[str, Any]] = []
    for spec in manifest["cases"]:
        case_id = spec["id"]
        if case_id not in selected:
            cases.append({"id": case_id, "name": spec["name"], "status": "not-run", "correctness": {"status": "not-run"}, "limitations": case_limitations(case_id, status="not-run")})
            continue
        reference_path = ROOT / spec["reference"]
        try:
            _, reference_fn = import_reference(reference_path)
            reference_result, reference_value = measure_reference(reference_fn, args.samples, args.warmup)
            if reference_value is None:
                raise RuntimeError("reference produced no successful value")
            candidate_result = measure_candidate(case_id, binary, reference_value, args.samples, args.warmup, args.timeout)
            if candidate_result["status"] == "passed" and reference_result["status"] == "passed":
                correctness = {"status": "passed", "contract": spec["correctness"], "candidate": candidate_result["observations"], "reference": reference_value}
                status = "passed"
            else:
                correctness = {"status": "failed", "contract": spec["correctness"], "candidate": candidate_result["observations"], "reference": reference_value, "errors": candidate_result["errors"] + reference_result["errors"]}
                status = "failed"
            cases.append({
                "id": case_id,
                "name": spec["name"],
                "kind": spec["kind"],
                "candidate_source": spec["candidate"],
                "reference_source": spec["reference"],
                "status": status,
                "candidate": candidate_result,
                "reference": reference_result,
                "correctness": correctness,
                "comparison": comparison(candidate_result, reference_result),
                "limitations": case_limitations(case_id, status=status),
            })
        except BlockedCase as error:
            cases.append({
                "id": case_id,
                "name": spec["name"],
                "kind": spec["kind"],
                "candidate_source": spec["candidate"],
                "reference_source": spec["reference"],
                "status": "blocked",
                "blocked_reason": str(error),
                "correctness": {"status": "blocked", "contract": spec["correctness"]},
                "limitations": case_limitations(case_id, status="blocked"),
            })
        except (OSError, RuntimeError, TypeError, ValueError, json.JSONDecodeError) as error:
            cases.append({
                "id": case_id,
                "name": spec["name"],
                "kind": spec["kind"],
                "candidate_source": spec["candidate"],
                "reference_source": spec["reference"],
                "status": "failed",
                "error": repr(error),
                "correctness": {"status": "failed", "contract": spec["correctness"]},
                "limitations": case_limitations(case_id, status="failed"),
            })
    summary = {
        "passed": sum(1 for case in cases if case["status"] == "passed"),
        "failed": sum(1 for case in cases if case["status"] == "failed"),
        "blocked": sum(1 for case in cases if case["status"] == "blocked"),
        "not_run": sum(1 for case in cases if case["status"] == "not-run"),
    }
    summary["total"] = len(cases)
    report = {
        "schema": "seloryn.benchmark-report.v1",
        "suite": "Seloryn Benchmark Suite",
        "run": {
            "started_at_utc": started_at,
            "samples": args.samples,
            "warmup": args.warmup,
            "timeout_seconds": args.timeout,
            "binary": relative_path(binary),
        },
        "environment": environment(binary),
        "manifest": {
            "path": relative_path(MANIFEST_PATH),
            "sha256": sha256_file(MANIFEST_PATH),
            "dataset_version": manifest["dataset_version"],
            "reference_policy": "stdlib-only",
        },
        "cases": cases,
        "summary": summary,
        "interpretation": {
            "scope": "six bounded repository cases, paired with independent Python references where applicable",
            "no_general_superiority_claim": True,
            "gains_and_losses_recorded": True,
            "blocked_means_required_capability_or_environment_evidence_is_unavailable": True,
        },
    }
    schema_errors = validate(report, json.loads(REPORT_SCHEMA.read_text(encoding="utf-8")))
    if schema_errors:
        raise ValueError("generated report did not satisfy its schema: " + "; ".join(schema_errors))
    return report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=DEFAULT_BINARY, help="Seloryn executable; default: target/release/seloryn")
    parser.add_argument("--samples", type=int, default=3, help="measured samples per implementation")
    parser.add_argument("--warmup", type=int, default=1, help="discarded warm-up runs per implementation")
    parser.add_argument("--timeout", type=float, default=60.0, help="per-process timeout in seconds")
    parser.add_argument("--case", action="append", choices=[f"BENCH-0{index}" for index in range(1, 7)], help="run one or more cases; omitted means all")
    parser.add_argument("--output", type=Path, default=BENCHMARKS / "reports" / "latest.json")
    parser.add_argument("--markdown", type=Path, default=BENCHMARKS / "reports" / "latest.md")
    args = parser.parse_args()
    if args.samples < 1 or args.warmup < 0 or args.timeout <= 0:
        parser.error("--samples must be >= 1, --warmup must be >= 0 and --timeout must be > 0")
    report = run(args)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.markdown.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    args.markdown.write_text(markdown_report(report), encoding="utf-8")
    print(json.dumps(report["summary"], sort_keys=True))
    print(f"JSON report: {args.output}")
    print(f"Markdown report: {args.markdown}")
    return 0 if report["summary"]["failed"] == 0 and report["summary"]["blocked"] == 0 and report["summary"]["not_run"] == 0 else 2 if report["summary"]["failed"] == 0 else 1


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError, json.JSONDecodeError) as error:
        print(f"benchmark runner failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
