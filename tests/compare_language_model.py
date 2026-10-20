"""Reproducible stdlib-Python versus Seloryn bigram-language-model comparison."""

from __future__ import annotations

import json
import math
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target" / "release" / ("seloryn.exe" if sys.platform == "win32" else "seloryn")
PROJECT = ROOT / "examples" / "projects" / "self-hosted-language-model"
TOKENS = ["<bos>", "the", "cat", "sat", "on", "mat", "dog", "ran", "in", "park", "<eos>", "a", "fox", "jumped"]
TOKEN_ID = {token: index for index, token in enumerate(TOKENS)}


def pairs(path: Path) -> list[tuple[int, int]]:
    ids = [TOKEN_ID[token] for token in json.loads(path.read_text(encoding="utf-8"))["tokens"]]
    return list(zip(ids, ids[1:]))


def loss(weights: list[list[float]], data: list[tuple[int, int]]) -> float:
    total = 0.0
    for source, target in data:
        row = weights[source]
        maximum = max(row)
        total += maximum + math.log(sum(math.exp(value - maximum) for value in row)) - row[target]
    return total / len(data)


def accuracy(weights: list[list[float]], data: list[tuple[int, int]]) -> float:
    return sum(max(range(len(weights[source])), key=weights[source].__getitem__) == target for source, target in data) / len(data)


def generate(weights: list[list[float]], steps: int) -> str:
    current = 0
    output: list[str] = []
    for _ in range(steps):
        current = max(range(len(weights[current])), key=weights[current].__getitem__)
        output.append(TOKENS[current] if current == 10 else TOKENS[current] + " ")
        if current == 10:
            break
    return "".join(output)


def python_model(train: list[tuple[int, int]], test: list[tuple[int, int]]) -> dict[str, object]:
    weights = [[0.0 for _ in TOKENS] for _ in TOKENS]
    initial = loss(weights, train)
    for _ in range(160):
        gradient = [[0.0 for _ in TOKENS] for _ in TOKENS]
        for source, target in train:
            row = weights[source]
            maximum = max(row)
            exponentials = [math.exp(value - maximum) for value in row]
            normalizer = sum(exponentials)
            for index, exponential in enumerate(exponentials):
                gradient[source][index] += exponential / normalizer
            gradient[source][target] -= 1.0
        scale = 0.5 / len(train)
        for source in range(len(TOKENS)):
            for target in range(len(TOKENS)):
                weights[source][target] -= scale * gradient[source][target]
    return {"language": "Python stdlib", "initial_loss": initial, "final_loss": loss(weights, train), "test_loss": loss(weights, test), "test_accuracy": accuracy(weights, test), "generated": generate(weights, 8)}


def main() -> int:
    train = pairs(PROJECT / "data" / "train.json")
    test = pairs(PROJECT / "data" / "test.json")
    started = time.perf_counter()
    python_result = python_model(train, test)
    python_result["wall_seconds"] = time.perf_counter() - started
    with tempfile.TemporaryDirectory(prefix="seloryn-language-comparison-") as directory:
        project = Path(directory) / "language-model"
        shutil.copytree(PROJECT, project)
        started = time.perf_counter()
        process = subprocess.run([str(BINARY), "run", str(project), "--format=json", "--log=off"], cwd=ROOT, input=json.dumps({"mode": "train"}), text=True, encoding="utf-8", capture_output=True, check=False, timeout=60)
        seloryn_seconds = time.perf_counter() - started
        assert process.returncode == 0, process.stderr
        report = json.loads(process.stdout)["value"]["value"]["fields"][0]["fields"]
    seloryn_result = {"language": "Seloryn .sely", "initial_loss": report["initial_loss"], "final_loss": report["final_loss"], "test_loss": report["test_loss"], "test_accuracy": report["test_accuracy"], "generated": report["generated"], "wall_seconds": seloryn_seconds}
    print(json.dumps({"python": python_result, "seloryn": seloryn_result}, indent=2, sort_keys=True))
    print("| implementation | initial loss | final loss | test loss | test accuracy | generation | wall seconds |")
    print("|---|---:|---:|---:|---:|---|---:|")
    for result in (python_result, seloryn_result):
        print(f"| {result['language']} | {result['initial_loss']:.9f} | {result['final_loss']:.9f} | {result['test_loss']:.9f} | {result['test_accuracy']:.6f} | `{result['generated']}` | {result['wall_seconds']:.6f} |")
    assert abs(python_result["final_loss"] - seloryn_result["final_loss"]) < 1.0e-5
    assert abs(python_result["test_loss"] - seloryn_result["test_loss"]) < 1.0e-5
    assert python_result["test_accuracy"] == seloryn_result["test_accuracy"]
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
