"""Reproducible stdlib-Python versus Seloryn contextual-model comparison."""

from __future__ import annotations

import json
import math
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target" / "release" / ("seloryn.exe" if sys.platform == "win32" else "seloryn")
PROJECT = ROOT / "examples" / "projects" / "self-hosted-language-model"
BENCHMARK_RUNS = 3
TOKENS = json.loads((PROJECT / "data" / "vocab.json").read_text(encoding="utf-8"))["tokens"]
TOKEN_ID = {token: index for index, token in enumerate(TOKENS)}
EOS = TOKEN_ID["<eos>"]
BOS = TOKEN_ID["<bos>"]
CONTEXT_SIZE = 4
STEPS = 160
LEARNING_RATE = 0.5


def windows(path: Path) -> list[tuple[list[int], int]]:
    ids = [TOKEN_ID[token] for token in json.loads(path.read_text(encoding="utf-8"))["tokens"]]
    return [(ids[offset : offset + CONTEXT_SIZE], ids[offset + CONTEXT_SIZE]) for offset in range(len(ids) - CONTEXT_SIZE)]


def logits(weights: list[list[float]], context: list[int]) -> list[float]:
    result = [0.0 for _ in TOKENS]
    for position, source in enumerate(context):
        row = weights[position * len(TOKENS) + source]
        for target, value in enumerate(row):
            result[target] += value
    return result


def loss(weights: list[list[float]], data: list[tuple[list[int], int]]) -> float:
    total = 0.0
    for context, target in data:
        row = logits(weights, context)
        maximum = max(row)
        total += maximum + math.log(sum(math.exp(value - maximum) for value in row)) - row[target]
    return total / len(data)


def accuracy(weights: list[list[float]], data: list[tuple[list[int], int]]) -> float:
    return sum(max(range(len(TOKENS)), key=logits(weights, context).__getitem__) == target for context, target in data) / len(data)


def generate(weights: list[list[float]], steps: int) -> str:
    context = [BOS for _ in range(CONTEXT_SIZE)]
    output: list[str] = []
    for _ in range(steps):
        current = max(range(len(TOKENS)), key=logits(weights, context).__getitem__)
        output.append(TOKENS[current])
        if current == EOS:
            break
        context = context[1:] + [current]
    return " ".join(output)


def python_model(train: list[tuple[list[int], int]], test: list[tuple[list[int], int]]) -> dict[str, object]:
    weights = [[0.0 for _ in TOKENS] for _ in range(CONTEXT_SIZE * len(TOKENS))]
    initial = loss(weights, train)
    for _ in range(STEPS):
        gradient = [[0.0 for _ in TOKENS] for _ in range(CONTEXT_SIZE * len(TOKENS))]
        for context, target in train:
            row = logits(weights, context)
            maximum = max(row)
            exponentials = [math.exp(value - maximum) for value in row]
            normalizer = sum(exponentials)
            for position, source in enumerate(context):
                for index, exponential in enumerate(exponentials):
                    gradient[position * len(TOKENS) + source][index] += exponential / normalizer
                gradient[position * len(TOKENS) + source][target] -= 1.0
        scale = LEARNING_RATE / len(train)
        for source in range(CONTEXT_SIZE * len(TOKENS)):
            for target in range(len(TOKENS)):
                weights[source][target] -= scale * gradient[source][target]
    test_loss = loss(weights, test)
    return {
        "language": "Python stdlib",
        "context_size": CONTEXT_SIZE,
        "initial_loss": initial,
        "final_loss": loss(weights, train),
        "test_loss": test_loss,
        "test_perplexity": math.exp(test_loss),
        "test_accuracy": accuracy(weights, test),
        "generated": generate(weights, 8),
    }


def python_worker() -> int:
    result = python_model(windows(PROJECT / "data" / "train.json"), windows(PROJECT / "data" / "test.json"))
    print(json.dumps(result, sort_keys=True))
    return 0


def main() -> int:
    train = windows(PROJECT / "data" / "train.json")
    test = windows(PROJECT / "data" / "test.json")
    if sys.argv[1:] == ["--python-worker"]:
        return python_worker()

    python_seconds_samples: list[float] = []
    python_result: dict[str, object] | None = None
    for _ in range(BENCHMARK_RUNS):
        started = time.perf_counter()
        process = subprocess.run(
            [sys.executable, __file__, "--python-worker"],
            cwd=ROOT,
            text=True,
            encoding="utf-8",
            capture_output=True,
            check=False,
            timeout=60,
        )
        python_seconds_samples.append(time.perf_counter() - started)
        assert process.returncode == 0, process.stderr
        current = json.loads(process.stdout)
        if python_result is None:
            python_result = current
        else:
            assert current == python_result
    assert python_result is not None
    python_result["wall_seconds"] = statistics.median(python_seconds_samples)
    python_result["wall_seconds_samples"] = python_seconds_samples

    seloryn_seconds_samples: list[float] = []
    report: dict[str, object] | None = None
    for _ in range(BENCHMARK_RUNS):
        with tempfile.TemporaryDirectory(prefix="seloryn-language-comparison-") as directory:
            project = Path(directory) / "language-model"
            shutil.copytree(PROJECT, project)
            started = time.perf_counter()
            process = subprocess.run([str(BINARY), "run", str(project), "--format=json", "--log=off"], cwd=ROOT, input=json.dumps({"mode": "train", "context_size": CONTEXT_SIZE, "steps": STEPS}), text=True, encoding="utf-8", capture_output=True, check=False, timeout=60)
            seloryn_seconds_samples.append(time.perf_counter() - started)
            assert process.returncode == 0, process.stderr
            current = json.loads(process.stdout)["value"]["value"]["fields"][0]["fields"]
            if report is None:
                report = current
            else:
                assert current == report
    assert report is not None
    seloryn_seconds = statistics.median(seloryn_seconds_samples)
    seloryn_result = {"language": "Seloryn .sely", "context_size": report["context_size"], "initial_loss": report["initial_loss"], "final_loss": report["final_loss"], "test_loss": report["test_loss"], "test_perplexity": report["test_perplexity"], "test_accuracy": report["test_accuracy"], "generated": report["generated"], "wall_seconds": seloryn_seconds}
    seloryn_result["wall_seconds_samples"] = seloryn_seconds_samples
    print(json.dumps({"python": python_result, "seloryn": seloryn_result}, indent=2, sort_keys=True))
    print("| implementation | context | initial loss | final loss | test loss | perplexity | test accuracy | generation | wall seconds |")
    print("|---|---:|---:|---:|---:|---:|---:|---|---:|")
    for result in (python_result, seloryn_result):
        print(f"| {result['language']} | {result['context_size']} | {result['initial_loss']:.9f} | {result['final_loss']:.9f} | {result['test_loss']:.9f} | {result['test_perplexity']:.8f} | {result['test_accuracy']:.6f} | `{result['generated']}` | {result['wall_seconds']:.6f} |")
    assert abs(python_result["final_loss"] - seloryn_result["final_loss"]) < 1.0e-5
    assert abs(python_result["test_loss"] - seloryn_result["test_loss"]) < 1.0e-5
    assert abs(python_result["test_perplexity"] - seloryn_result["test_perplexity"]) < 1.0e-5
    assert python_result["test_accuracy"] == seloryn_result["test_accuracy"]
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
