"""Black-box proof for the self-hosted Seloryn language-model project."""

from __future__ import annotations

import json
import math
import subprocess
import sys
import tempfile
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target" / "release" / "seloryn.exe"
PROJECT = ROOT / "examples" / "projects" / "self-hosted-language-model"
TRAIN = PROJECT / "data" / "train.json"
TEST = PROJECT / "data" / "test.json"
CHECKPOINT = PROJECT / "checkpoints" / "language-model.json"
VOCAB = [
    "<bos>",
    "the",
    "cat",
    "sat",
    "on",
    "mat",
    "dog",
    "ran",
    "in",
    "park",
    "<eos>",
    "a",
    "fox",
    "jumped",
]
TOKEN_ID = {token: index for index, token in enumerate(VOCAB)}


def run_seloryn(*arguments: str, stdin: str = "") -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [str(BINARY), *arguments],
        cwd=ROOT,
        input=stdin,
        text=True,
        capture_output=True,
        check=False,
    )


def response_record(stdout: str) -> dict:
    envelope = json.loads(stdout)
    assert envelope["value"]["ok"] is True, envelope
    enum_value = envelope["value"]["value"]
    assert enum_value["type"] == "enum", enum_value
    record = enum_value["fields"][0]
    assert record["type"] == "record", record
    return record["fields"]


def load_tokens(path: Path) -> list[str]:
    return json.loads(path.read_text(encoding="utf-8"))["tokens"]


def pairs(tokens: list[str]) -> list[tuple[int, int]]:
    return [(TOKEN_ID[tokens[index]], TOKEN_ID[tokens[index + 1]]) for index in range(len(tokens) - 1)]


def softmax(row: list[float]) -> list[float]:
    maximum = max(row)
    exponents = [math.exp(value - maximum) for value in row]
    total = sum(exponents)
    return [value / total for value in exponents]


def reference_train(train_pairs: list[tuple[int, int]], test_pairs: list[tuple[int, int]], steps: int = 160) -> dict:
    vocabulary_size = len(VOCAB)
    weights = [[0.0 for _ in range(vocabulary_size)] for _ in range(vocabulary_size)]
    scale = float(len(train_pairs))

    def loss(data: list[tuple[int, int]]) -> float:
        return sum(-math.log(softmax(weights[input_id])[target_id]) for input_id, target_id in data) / len(data)

    initial_loss = loss(train_pairs)
    for _ in range(steps):
        gradients = [[0.0 for _ in range(vocabulary_size)] for _ in range(vocabulary_size)]
        for input_id, target_id in train_pairs:
            probabilities = softmax(weights[input_id])
            for output_id, probability in enumerate(probabilities):
                gradients[input_id][output_id] += probability / scale
            gradients[input_id][target_id] -= 1.0 / scale
        for input_id in range(vocabulary_size):
            for output_id in range(vocabulary_size):
                weights[input_id][output_id] -= 0.5 * gradients[input_id][output_id]

    return {
        "initial_loss": initial_loss,
        "final_loss": loss(train_pairs),
        "test_loss": loss(test_pairs),
        "accuracy": sum(
            max(range(vocabulary_size), key=lambda output_id: weights[input_id][output_id]) == target_id
            for input_id, target_id in test_pairs
        )
        / len(test_pairs),
    }


def main() -> None:
    assert BINARY.is_file(), BINARY
    source_files = sorted(PROJECT.joinpath("src").rglob("*.sely"))
    source_text = "\n".join(path.read_text(encoding="utf-8") for path in source_files)
    for forbidden in ("train_mlp", "train_linear", "predict_mlp", "load_checkpoint"):
        assert forbidden not in source_text, forbidden
    expected_modules = {"checkpoint.sely", "data.sely", "model.sely", "production.sely", "serving.sely", "tokenizer.sely", "training.sely"}
    assert {path.name for path in source_files} >= expected_modules, source_files

    checked = run_seloryn("check", str(PROJECT), "--format=json")
    assert checked.returncode == 0, (checked.returncode, checked.stdout, checked.stderr)

    train_start = time.perf_counter()
    trained = run_seloryn(
        "run",
        str(PROJECT),
        "--format=json",
        "--log=json2",
        stdin='{"mode":"train"}',
    )
    seloryn_seconds = time.perf_counter() - train_start
    assert trained.returncode == 0, (trained.returncode, trained.stdout, trained.stderr)
    report = response_record(trained.stdout)
    assert report["architecture"] == "bigram_softmax", report
    assert report["steps"] == 160, report
    assert report["initial_loss"] > report["final_loss"] > 0.0, report
    assert report["test_loss"] < 1.60, report
    assert abs(report["test_loss"] - report["reloaded_loss"]) < 1e-6, report
    assert report["test_accuracy"] >= 0.5, report
    assert report["generated"], report
    assert CHECKPOINT.is_file(), CHECKPOINT

    receipt_lines = [json.loads(line) for line in trained.stderr.splitlines() if line.strip()]
    assert len(receipt_lines) == 162, len(receipt_lines)
    assert receipt_lines[0]["kind"] == "run.start", receipt_lines[0]
    assert receipt_lines[-1]["kind"] == "run.end", receipt_lines[-1]
    progress = [event for event in receipt_lines if event["kind"] == "training.progress"]
    assert len(progress) == 160, len(progress)
    assert progress[0]["data"]["step"] == 1, progress[0]
    assert progress[-1]["data"]["step"] == 160, progress[-1]
    assert progress[0]["data"]["metric"] > progress[-1]["data"]["metric"], (progress[0], progress[-1])

    with tempfile.NamedTemporaryFile("w", suffix=".json2", encoding="utf-8", delete=False) as receipt:
        receipt.write(trained.stderr)
        receipt_path = Path(receipt.name)
    try:
        verified = subprocess.run(
            [sys.executable, str(ROOT / "scripts" / "streaming" / "verify-event-chain.py"), str(receipt_path)],
            cwd=ROOT,
            text=True,
            capture_output=True,
            check=False,
        )
    finally:
        receipt_path.unlink(missing_ok=True)
    assert verified.returncode == 0, (verified.returncode, verified.stdout, verified.stderr)

    checkpoint = json.loads(CHECKPOINT.read_text(encoding="utf-8"))
    assert checkpoint["schema"] == "seloryn.sely.language-model.v1", checkpoint
    assert checkpoint["architecture"] == "bigram_softmax", checkpoint
    assert checkpoint["vocabulary_size"] == 14, checkpoint
    assert len(checkpoint["weights"]) == 196, len(checkpoint["weights"])
    assert len(checkpoint["weights_sha256"]) == 64, checkpoint
    assert len(report["checkpoint_hash"]) == 64, report

    inferred = run_seloryn(
        "run",
        str(PROJECT),
        "--format=json",
        "--log=json",
        stdin='{"mode":"infer","tokens":["<bos>","the","cat"],"prompt_count":3,"generation_steps":4}',
    )
    assert inferred.returncode == 0, (inferred.returncode, inferred.stdout, inferred.stderr)
    inference_report = response_record(inferred.stdout)
    assert inference_report["status"] == "ok", inference_report
    assert inference_report["generated"], inference_report
    assert inference_report["checkpoint_sha256"] == report["checkpoint_hash"], inference_report

    invalid = run_seloryn(
        "run",
        str(PROJECT),
        "--format=json",
        stdin='{"mode":"infer","tokens":["unknown"],"prompt_count":1,"generation_steps":2}',
    )
    assert invalid.returncode != 0, invalid.stdout
    assert "unknown token in dataset" in invalid.stdout or "unknown token in dataset" in invalid.stderr, (invalid.stdout, invalid.stderr)

    mismatch = run_seloryn(
        "run",
        str(PROJECT),
        "--format=json",
        stdin='{"mode":"infer","tokens":["<bos>"],"prompt_count":2,"generation_steps":2}',
    )
    assert mismatch.returncode != 0, mismatch.stdout
    assert "prompt_count must match tokens" in mismatch.stdout or "prompt_count must match tokens" in mismatch.stderr, (mismatch.stdout, mismatch.stderr)

    reference_start = time.perf_counter()
    reference = reference_train(pairs(load_tokens(TRAIN)), pairs(load_tokens(TEST)))
    python_seconds = time.perf_counter() - reference_start
    assert abs(reference["final_loss"] - report["final_loss"]) < 1e-4, (reference, report)
    assert abs(reference["test_loss"] - report["test_loss"]) < 1e-4, (reference, report)
    assert reference["accuracy"] >= 0.5, reference

    points = [progress[index]["data"]["metric"] for index in (0, 39, 79, 119, 159)]
    minimum = min(points)
    maximum = max(points)
    bars = " ".join("#" * max(1, round(20 * (value - minimum) / max(maximum - minimum, 1e-12))) for value in points)
    print("Seloryn self-hosted language-model proof: PASS")
    print(f"check_exit=0 train_exit=0 streaming_verify_exit={verified.returncode} infer_exit=0")
    print(f"events=162 progress=160 receipt={verified.stdout.strip()}")
    print("comparison | backend | initial_loss | final_loss | test_loss | accuracy | seconds")
    print(f"comparison | Seloryn/.sely | {report['initial_loss']:.8f} | {report['final_loss']:.8f} | {report['test_loss']:.8f} | {report['test_accuracy']:.6f} | {seloryn_seconds:.4f}")
    print(f"comparison | Python/pure  | {reference['initial_loss']:.8f} | {reference['final_loss']:.8f} | {reference['test_loss']:.8f} | {reference['accuracy']:.6f} | {python_seconds:.4f}")
    print(f"loss_curve checkpoints 1,40,80,120,160: {bars}")
    print(f"generated={inference_report['generated']!r} checkpoint_sha256={report['checkpoint_hash']}")


if __name__ == "__main__":
    main()
