"""Reproducible local benchmark for the source-owned training workflow."""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PROJECT = ROOT / "examples" / "projects" / "auditable-local-inference"
DEFAULT_BINARY = ROOT / "target" / "release" / (
    "seloryn.exe" if sys.platform == "win32" else "seloryn"
)


def response_record(stdout: str) -> dict:
    envelope = json.loads(stdout)
    if envelope["value"]["ok"] is not True:
        raise AssertionError(envelope)
    enum_value = envelope["value"]["value"]
    if enum_value["type"] != "enum" or enum_value["fields"][0]["type"] != "record":
        raise AssertionError(enum_value)
    return enum_value["fields"][0]["fields"]


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def percentile95(samples: list[float]) -> float:
    if len(samples) == 1:
        return samples[0]
    return statistics.quantiles(samples, n=20, method="inclusive")[18]


def train_once(binary: Path, root: Path, run_number: int) -> tuple[float, dict]:
    project = root / f"seloryn-{run_number}"
    shutil.copytree(PROJECT, project)
    started = time.perf_counter()
    process = subprocess.run(
        [str(binary), "run", str(project), "--format=json", "--log=off"],
        cwd=ROOT,
        input='{"mode":"train"}',
        text=True,
        capture_output=True,
        check=False,
    )
    elapsed = time.perf_counter() - started
    if process.returncode != 0:
        raise AssertionError(
            f"{binary} failed on run {run_number} with {process.returncode}: "
            f"{process.stdout}\n{process.stderr}"
        )
    report = response_record(process.stdout)
    if not report["initial_loss"] > report["final_loss"] > 0.0:
        raise AssertionError(report)
    if report["test_accuracy"] != 1.0 or not (project / "checkpoints" / "equipment-health.json").is_file():
        raise AssertionError(report)
    return elapsed, report


def benchmark_binary(binary: Path, runs: int) -> dict:
    if not binary.is_file():
        raise FileNotFoundError(binary)
    with tempfile.TemporaryDirectory(prefix="seloryn-performance-") as temporary:
        samples = [train_once(binary, Path(temporary), index) for index in range(runs)]
    elapsed = [sample[0] for sample in samples]
    report = samples[-1][1]
    return {
        "binary": str(binary),
        "binary_sha256": file_sha256(binary),
        "runs": runs,
        "seconds": elapsed,
        "median_seconds": statistics.median(elapsed),
        "p95_seconds": percentile95(elapsed),
        "report": report,
    }


def python_reference(runs: int) -> dict:
    path = PROJECT / "baseline" / "reference.py"
    spec = importlib.util.spec_from_file_location("seloryn_reference", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load reference: {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    samples: list[float] = []
    report = None
    for _ in range(runs):
        started = time.perf_counter()
        report = module.train_reference()
        samples.append(time.perf_counter() - started)
    assert report is not None
    if report["accuracy"] != 1.0 or not report["initial_loss"] > report["final_loss"] > 0.0:
        raise AssertionError(report)
    return {
        "binary": "python-pure-reference",
        "runs": runs,
        "seconds": samples,
        "median_seconds": statistics.median(samples),
        "p95_seconds": percentile95(samples),
        "report": report,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=DEFAULT_BINARY)
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument(
        "--require-reduction",
        action="store_true",
        help="fail unless the current median is lower than the baseline median",
    )
    args = parser.parse_args()
    if args.runs < 1:
        raise ValueError("--runs must be positive")

    current = benchmark_binary(args.binary, args.runs)
    reference = python_reference(args.runs)
    payload = {"current": current, "python_reference": reference}
    if args.baseline is not None:
        payload["baseline"] = benchmark_binary(args.baseline, args.runs)
        payload["reduction"] = {
            "median_seconds": payload["baseline"]["median_seconds"] - current["median_seconds"],
            "median_ratio": payload["baseline"]["median_seconds"] / current["median_seconds"],
        }
        if args.require_reduction and current["median_seconds"] >= payload["baseline"]["median_seconds"]:
            raise AssertionError(
                "current median is not lower than baseline: "
                f"{current['median_seconds']:.6f} >= {payload['baseline']['median_seconds']:.6f}"
            )
    print(json.dumps(payload, indent=2, sort_keys=True))
    print()
    print("| Backend | Median seconds | P95 seconds | Final loss | Test accuracy |")
    print("|---|---:|---:|---:|---:|")
    for label, result in (("Seloryn current", current), ("Python pure reference", reference)):
        report = result["report"]
        accuracy = report.get("test_accuracy", report.get("accuracy"))
        print(
            f"| {label} | {result['median_seconds']:.6f} | {result['p95_seconds']:.6f} | "
            f"{report['final_loss']:.8f} | {accuracy:.6f} |"
        )
    if args.baseline is not None:
        report = payload["baseline"]["report"]
        print(
            f"| Seloryn baseline | {payload['baseline']['median_seconds']:.6f} | "
            f"{payload['baseline']['p95_seconds']:.6f} | {report['final_loss']:.8f} | "
            f"{report['test_accuracy']:.6f} |"
        )
        reduction = payload["reduction"]
        print()
        print(
            f"Reduction gate | median={reduction['median_seconds']:.6f}s | "
            f"baseline/current={reduction['median_ratio']:.3f}x"
        )


if __name__ == "__main__":
    try:
        main()
    except (AssertionError, FileNotFoundError, OSError, subprocess.SubprocessError, ValueError, json.JSONDecodeError) as error:
        print(f"performance probe FAILED: {error!r}", file=sys.stderr)
        raise SystemExit(1) from error
