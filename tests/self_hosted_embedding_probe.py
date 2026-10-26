"""Black-box proof for the embedding/ReLU MLP language-model architecture."""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tests"))
from reference.embedding_language_model import train as reference_train  # noqa: E402

BINARY = ROOT / "target" / "release" / ("seloryn.exe" if sys.platform == "win32" else "seloryn")
PROJECT = ROOT / "examples" / "projects" / "self-hosted-language-model"
VOCAB = json.loads((PROJECT / "data" / "vocab.json").read_text(encoding="utf-8"))["tokens"]
TOKEN_ID = {token: index for index, token in enumerate(VOCAB)}
CONTEXT_SIZE = 4
EMBEDDING_DIM = 8
HIDDEN_SIZE = 32
STEPS = 160
SEED = 17


def run(binary: Path, project: Path, payload: dict) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [str(binary), "run", str(project), "--format=json", "--log=off"],
        cwd=ROOT,
        input=json.dumps(payload),
        text=True,
        encoding="utf-8",
        capture_output=True,
        check=False,
        timeout=120,
    )


def record(process: subprocess.CompletedProcess[str]) -> dict:
    assert process.returncode == 0, (process.stdout, process.stderr)
    envelope = json.loads(process.stdout)
    assert envelope["value"]["ok"] is True, envelope
    enum_value = envelope["value"]["value"]
    fields = enum_value["fields"][0]
    return fields["fields"]


def windows(path: Path) -> list[tuple[list[int], int]]:
    tokens = json.loads(path.read_text(encoding="utf-8"))["tokens"]
    ids = [TOKEN_ID[token] for token in tokens]
    return [(ids[index : index + CONTEXT_SIZE], ids[index + CONTEXT_SIZE]) for index in range(len(ids) - CONTEXT_SIZE)]


def main() -> None:
    assert BINARY.is_file(), BINARY
    checked = subprocess.run([str(BINARY), "check", str(PROJECT), "--format=json"], cwd=ROOT, text=True, capture_output=True, check=False)
    assert checked.returncode == 0, (checked.stdout, checked.stderr)

    payload = {
        "mode": "train_embedding",
        "context_size": CONTEXT_SIZE,
        "embedding_dim": EMBEDDING_DIM,
        "hidden_size": HIDDEN_SIZE,
        "steps": STEPS,
        "seed": SEED,
        "resume": False,
    }
    trained = run(BINARY, PROJECT, payload)
    report = record(trained)
    assert report["architecture"] == "embedding_relu_mlp_softmax", report
    assert report["parameter_count"] == 1630, report
    assert report["initial_loss"] > report["final_loss"] > 0.0, report
    assert report["final_loss"] < 0.25, report
    assert report["test_loss"] < 1.0, report
    assert report["test_perplexity"] < 3.0, report
    assert abs(report["test_loss"] - report["reloaded_loss"]) < 1e-6, report
    assert report["test_accuracy"] >= 0.5, report
    assert report["generated"], report

    checkpoint = json.loads((PROJECT / "checkpoints" / "language-model-embedding.json").read_text(encoding="utf-8"))
    assert checkpoint["schema"] == "seloryn.sely.language-model.v3", checkpoint
    assert checkpoint["architecture"] == "embedding_relu_mlp_softmax", checkpoint
    assert checkpoint["device"] == "cpu", checkpoint
    assert checkpoint["dtype"] == "F32", checkpoint
    assert checkpoint["optimizer"] == "sgd", checkpoint
    assert checkpoint["learning_rate"] == 0.5, checkpoint
    assert len(checkpoint["embedding"]) == 14 * EMBEDDING_DIM, checkpoint
    assert len(checkpoint["first_weights"]) == CONTEXT_SIZE * EMBEDDING_DIM * HIDDEN_SIZE, checkpoint
    assert len(checkpoint["second_weights"]) == HIDDEN_SIZE * len(VOCAB), checkpoint
    for key in ("embedding_sha256", "first_weights_sha256", "first_bias_sha256", "second_weights_sha256", "second_bias_sha256", "vocabulary_sha256", "state_sha256"):
        assert len(checkpoint[key]) == 64, (key, checkpoint)

    inferred = run(BINARY, PROJECT, {"mode": "infer_embedding_text", "text": "<bos> the cat sat", "generation_steps": 4})
    inference_report = record(inferred)
    assert inference_report["mode"] == "infer_embedding_text", inference_report
    assert inference_report["status"] == "ok", inference_report
    assert inference_report["prompt_tokens"] == CONTEXT_SIZE, inference_report
    assert inference_report["checkpoint_sha256"] == report["checkpoint_hash"], inference_report

    reference = reference_train(
        windows(PROJECT / "data" / "train.json"),
        windows(PROJECT / "data" / "test.json"),
        len(VOCAB),
        CONTEXT_SIZE,
        EMBEDDING_DIM,
        HIDDEN_SIZE,
        STEPS,
        SEED,
        VOCAB,
        TOKEN_ID["<bos>"],
        TOKEN_ID["<eos>"],
    )
    for field in ("initial_loss", "final_loss", "test_loss", "test_perplexity"):
        assert abs(report[field] - reference[field]) < 2e-4, (field, report, reference)
    assert report["test_accuracy"] == reference["test_accuracy"], (report, reference)
    assert report["generated"] == reference["generated"], (report, reference)

    with tempfile.TemporaryDirectory(prefix="seloryn-embedding-resume-") as directory:
        split_project = Path(directory) / "language-model"
        shutil.copytree(PROJECT, split_project)
        first_half = run(BINARY, split_project, {**payload, "steps": 80})
        assert first_half.returncode == 0, (first_half.stdout, first_half.stderr)
        resumed = record(run(BINARY, split_project, {**payload, "steps": STEPS, "resume": True}))
        for field in ("final_loss", "test_loss", "test_perplexity", "test_accuracy", "checkpoint_hash"):
            assert resumed[field] == report[field], (field, resumed, report)

    with tempfile.TemporaryDirectory(prefix="seloryn-embedding-determinism-") as directory:
        deterministic_project = Path(directory) / "language-model"
        shutil.copytree(PROJECT, deterministic_project)
        first = record(run(BINARY, deterministic_project, payload))
        second = record(run(BINARY, deterministic_project, payload))
        assert first == second, (first, second)

    with tempfile.TemporaryDirectory(prefix="seloryn-embedding-bad-vocab-") as directory:
        bad_project = Path(directory) / "language-model"
        shutil.copytree(PROJECT, bad_project)
        (bad_project / "data" / "vocab.json").write_text(json.dumps({"tokens": list(reversed(VOCAB))}), encoding="utf-8")
        invalid = run(BINARY, bad_project, {"mode": "infer_embedding_text", "text": "<bos> the cat sat", "generation_steps": 2})
        assert invalid.returncode != 0, invalid.stdout
        assert "checkpoint vocabulary hash does not match tokenizer" in invalid.stdout or "checkpoint vocabulary hash does not match tokenizer" in invalid.stderr

    print("Seloryn embedding language-model proof: PASS")
    print(f"architecture=embedding_relu_mlp_softmax parameters={report['parameter_count']} seed={SEED} steps={STEPS}")
    print(f"initial_loss={report['initial_loss']:.8f} final_loss={report['final_loss']:.8f} test_loss={report['test_loss']:.8f} perplexity={report['test_perplexity']:.8f} accuracy={report['test_accuracy']:.6f}")
    print(f"reference_final_loss={reference['final_loss']:.8f} reference_test_loss={reference['test_loss']:.8f} generated={report['generated']!r}")
    print("gates=forward,scatter_add,indexed_ce,autograd,reload,split_resume,determinism,hash_integrity")


if __name__ == "__main__":
    main()
