"""Run the registered P0 workloads as a balanced baseline/candidate campaign.

The runner intentionally treats the executable as a black box.  It validates
both binaries against the independent oracle before collecting timings, keeps
the paired order in the report, and records all raw observations so a result
can be audited rather than reduced to a headline speedup.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import platform
import statistics
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Callable


ROOT = Path(__file__).resolve().parents[2]
P0 = Path(__file__).resolve().parent
REFERENCE = P0 / "reference.py"
WORKLOADS = {
    "array-aggregation": P0 / "array-aggregation.sely",
    "tensor-loop": P0 / "tensor-loop.sely",
    "numeric-json-pipeline": P0 / "numeric-json-pipeline",
}


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def sha256_tree(path: Path) -> str:
    digest = hashlib.sha256()
    files = sorted(item for item in path.rglob("*") if item.is_file())
    for item in files:
        relative = item.relative_to(path).as_posix().encode("utf-8")
        digest.update(relative)
        digest.update(b"\0")
        digest.update(item.read_bytes())
        digest.update(b"\0")
    return digest.hexdigest()


def git_value(*args: str) -> str | None:
    process = subprocess.run(
        ["git", *args], cwd=ROOT, capture_output=True, text=True, encoding="utf-8", check=False
    )
    return process.stdout.strip() if process.returncode == 0 else None


def load_oracles() -> dict[str, Any]:
    namespace: dict[str, Any] = {"__file__": str(REFERENCE), "__name__": "p0_reference"}
    exec(REFERENCE.read_text(encoding="utf-8"), namespace)
    return {
        "array-aggregation": namespace["array_aggregation"](),
        "tensor-loop": namespace["tensor_loop"](),
        "numeric-json-pipeline": namespace["numeric_json_pipeline"](),
    }


def command_for(workload: str, binary: Path, mode: str) -> list[str]:
    path = WORKLOADS[workload]
    if workload == "numeric-json-pipeline":
        return [str(binary), mode, str(path), "--format=json", "--log=off"]
    return [str(binary), mode, str(path)]


def run_process(command: list[str], timeout: float) -> dict[str, Any]:
    started = time.perf_counter_ns()
    try:
        process = subprocess.run(
            command,
            cwd=ROOT,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=timeout,
            check=False,
        )
        timed_out = False
        exit_code = process.returncode
        stdout = process.stdout
        stderr = process.stderr
    except subprocess.TimeoutExpired as error:
        timed_out = True
        exit_code = None
        stdout = error.stdout if isinstance(error.stdout, str) else ""
        stderr = error.stderr if isinstance(error.stderr, str) else ""
    elapsed_ms = (time.perf_counter_ns() - started) / 1_000_000.0
    return {
        "elapsed_ms": round(elapsed_ms, 6),
        "exit_code": exit_code,
        "stdout": stdout,
        "stderr": stderr,
        "timed_out": timed_out,
        "output_sha256": sha256_bytes((stdout + "\0" + stderr).encode("utf-8")),
    }


def result_value(workload: str, stdout: str) -> float | int:
    if workload == "array-aggregation":
        return int(stdout.strip())
    if workload == "tensor-loop":
        return float(stdout.strip())
    envelope = json.loads(stdout)
    value = envelope["value"]
    if isinstance(value, dict) and value.get("ok") is True:
        value = value["value"]
    if not isinstance(value, (int, float)):
        raise ValueError(f"unexpected project result: {value!r}")
    return value


def correct(workload: str, process: dict[str, Any], expected: Any) -> tuple[bool, str | None, Any]:
    if process["timed_out"] or process["exit_code"] != 0:
        return False, f"exit={process['exit_code']} timeout={process['timed_out']}", None
    try:
        actual = result_value(workload, process["stdout"])
    except (ValueError, TypeError, KeyError, json.JSONDecodeError) as error:
        return False, str(error), None
    tolerance = 1e-9 if workload == "tensor-loop" else 1e-7
    is_correct = math.isclose(float(actual), float(expected), rel_tol=tolerance, abs_tol=tolerance)
    return is_correct, None if is_correct else f"expected={expected!r} actual={actual!r}", actual


def percentile(values: list[float], fraction: float) -> float | None:
    if not values:
        return None
    ordered = sorted(values)
    if len(ordered) == 1:
        return ordered[0]
    position = (len(ordered) - 1) * fraction
    lower = math.floor(position)
    upper = math.ceil(position)
    if lower == upper:
        return ordered[lower]
    return ordered[lower] + (ordered[upper] - ordered[lower]) * (position - lower)


def timing_stats(samples: list[dict[str, Any]]) -> dict[str, Any]:
    values = [float(sample["elapsed_ms"]) for sample in samples]
    return {
        "count": len(values),
        "median_ms": round(statistics.median(values), 6) if values else None,
        "p95_ms": round(percentile(values, 0.95), 6) if values else None,
        "min_ms": round(min(values), 6) if values else None,
        "max_ms": round(max(values), 6) if values else None,
        "samples_ms": values,
    }


def binary_metadata(path: Path) -> dict[str, Any]:
    return {"path": str(path), "bytes": path.stat().st_size, "sha256": sha256_file(path)}


def measure_binary(
    label: str,
    workload: str,
    binary: Path,
    expected: Any,
    samples: int,
    warmup: int,
    timeout: float,
) -> dict[str, Any]:
    run_command = command_for(workload, binary, "run")
    check_command = command_for(workload, binary, "check")
    preflight = run_process(run_command, timeout)
    preflight_ok, preflight_error, actual = correct(workload, preflight, expected)
    if not preflight_ok:
        raise RuntimeError(f"{label}/{workload} preflight failed: {preflight_error}; stderr={preflight['stderr']!r}")

    warmups: list[dict[str, Any]] = []
    for index in range(warmup):
        observation = run_process(run_command, timeout)
        ok, error, _ = correct(workload, observation, expected)
        warmups.append({"index": index, "elapsed_ms": observation["elapsed_ms"], "correct": ok, "error": error})
        if not ok:
            raise RuntimeError(f"{label}/{workload} warmup {index} failed: {error}")

    run_samples: list[dict[str, Any]] = []
    check_samples: list[dict[str, Any]] = []
    for index in range(samples):
        observation = run_process(run_command, timeout)
        ok, error, _ = correct(workload, observation, expected)
        run_samples.append({
            "index": index,
            "elapsed_ms": observation["elapsed_ms"],
            "exit_code": observation["exit_code"],
            "correct": ok,
            "error": error,
            "output_sha256": observation["output_sha256"],
        })
        if not ok:
            raise RuntimeError(f"{label}/{workload} sample {index} failed: {error}")
        check_observation = run_process(check_command, timeout)
        check_samples.append({
            "index": index,
            "elapsed_ms": check_observation["elapsed_ms"],
            "exit_code": check_observation["exit_code"],
            "passed": check_observation["exit_code"] == 0 and not check_observation["timed_out"],
            "output_sha256": check_observation["output_sha256"],
        })
        if not check_samples[-1]["passed"]:
            raise RuntimeError(f"{label}/{workload} check {index} failed")

    return {
        "label": label,
        "binary": binary_metadata(binary),
        "preflight": {
            "elapsed_ms": preflight["elapsed_ms"],
            "output_sha256": preflight["output_sha256"],
            "actual": actual,
        },
        "warmups": warmups,
        "run": {**timing_stats(run_samples), "samples": run_samples},
        "check": {**timing_stats(check_samples), "samples": check_samples},
    }


def paired_measure(
    workload: str,
    baseline: Path,
    candidate: Path,
    expected: Any,
    samples: int,
    warmup: int,
    timeout: float,
) -> dict[str, Any]:
    commands = {"baseline": command_for(workload, baseline, "run"), "candidate": command_for(workload, candidate, "run")}
    for label, binary in (("baseline", baseline), ("candidate", candidate)):
        preflight = run_process(commands[label], timeout)
        ok, error, actual = correct(workload, preflight, expected)
        if not ok:
            raise RuntimeError(f"{label}/{workload} preflight failed: {error}")
        if actual is None:
            raise RuntimeError(f"{label}/{workload} preflight did not produce a value")

    for binary in (baseline, candidate):
        for index in range(warmup):
            observation = run_process(command_for(workload, binary, "run"), timeout)
            ok, error, _ = correct(workload, observation, expected)
            if not ok:
                raise RuntimeError(f"warmup {binary}/{workload}/{index} failed: {error}")

    observations: dict[str, list[dict[str, Any]]] = {"baseline": [], "candidate": []}
    check_observations: dict[str, list[dict[str, Any]]] = {"baseline": [], "candidate": []}
    orders: list[list[str]] = []
    for index in range(samples):
        order = ["baseline", "candidate"] if index % 2 == 0 else ["candidate", "baseline"]
        orders.append(order)
        for label in order:
            binary = baseline if label == "baseline" else candidate
            process = run_process(command_for(workload, binary, "run"), timeout)
            ok, error, _ = correct(workload, process, expected)
            sample = {
                "pair_index": index,
                "order_position": order.index(label),
                "elapsed_ms": process["elapsed_ms"],
                "exit_code": process["exit_code"],
                "correct": ok,
                "error": error,
                "output_sha256": process["output_sha256"],
            }
            observations[label].append(sample)
            if not ok:
                raise RuntimeError(f"{label}/{workload} sample {index} failed: {error}")
            check_process = run_process(command_for(workload, binary, "check"), timeout)
            check_sample = {
                "pair_index": index,
                "order_position": order.index(label),
                "elapsed_ms": check_process["elapsed_ms"],
                "exit_code": check_process["exit_code"],
                "passed": check_process["exit_code"] == 0 and not check_process["timed_out"],
                "output_sha256": check_process["output_sha256"],
            }
            check_observations[label].append(check_sample)
            if not check_sample["passed"]:
                raise RuntimeError(f"{label}/{workload} check {index} failed")

    baseline_median = statistics.median(item["elapsed_ms"] for item in observations["baseline"])
    candidate_median = statistics.median(item["elapsed_ms"] for item in observations["candidate"])
    return {
        "workload": workload,
        "expected": expected,
        "warmup_count": warmup,
        "pair_count": samples,
        "orders": orders,
        "baseline": {**timing_stats(observations["baseline"]), "samples": observations["baseline"]},
        "candidate": {**timing_stats(observations["candidate"]), "samples": observations["candidate"]},
        "check": {
            "baseline": {**timing_stats(check_observations["baseline"]), "samples": check_observations["baseline"]},
            "candidate": {**timing_stats(check_observations["candidate"]), "samples": check_observations["candidate"]},
        },
        "speedup_median": round(baseline_median / candidate_median, 6),
        "memory_allocation_evidence": {"status": "NOT RUN", "reason": "No approved process-level allocator/RSS sampler was available in this Windows host."},
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--samples", type=int, default=30)
    parser.add_argument("--warmup", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=180.0)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.samples < 30:
        raise SystemExit("--samples must be at least 30 for the registered gate")
    baseline = args.baseline.resolve()
    candidate = args.candidate.resolve()
    oracles = load_oracles()
    result: dict[str, Any] = {
        "schema": "seloryn.p0.benchmark.v1",
        "generated_at_utc": datetime.now(timezone.utc).isoformat(),
        "host": {
            "platform": platform.platform(),
            "machine": platform.machine(),
            "processor": platform.processor(),
            "python": sys.version,
            "pid": os.getpid(),
        },
        "repository": {
            "root": str(ROOT),
            "head": git_value("rev-parse", "HEAD"),
            "branch": git_value("branch", "--show-current"),
            "origin_master": git_value("rev-parse", "origin/master"),
        },
        "source": {
            "reference_sha256": sha256_file(REFERENCE),
            "workloads": {name: sha256_tree(path) if path.is_dir() else sha256_file(path) for name, path in WORKLOADS.items()},
        },
        "baseline_binary": binary_metadata(baseline),
        "candidate_binary": binary_metadata(candidate),
        "configuration": {"samples": args.samples, "warmup": args.warmup, "timeout_seconds": args.timeout, "order": "alternating baseline/candidate per pair"},
        "correctness": {"oracles": oracles},
        "workloads": [],
    }
    for workload in WORKLOADS:
        print(f"measuring {workload}", flush=True)
        result["workloads"].append(paired_measure(workload, baseline, candidate, oracles[workload], args.samples, args.warmup, args.timeout))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps({"output": str(args.output), "workloads": len(result["workloads"])}, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
