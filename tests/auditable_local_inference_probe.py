"""Black-box proof for the source-owned auditable local inference project."""

from __future__ import annotations

import importlib
import json
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target" / "release" / ("seloryn.exe" if sys.platform == "win32" else "seloryn")
PROJECT = ROOT / "examples" / "projects" / "auditable-local-inference"
INFERENCE = {
    "mode": "infer",
    "feature_names": ["temperature", "vibration", "load", "age_days"],
    "features": [0.90, 0.92, 0.88, 0.91],
}


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
    record = enum_value["fields"][0]
    assert enum_value["type"] == "enum", enum_value
    assert record["type"] == "record", record
    return record["fields"]


def assert_failure(result: subprocess.CompletedProcess[str], expected: str) -> None:
    assert result.returncode != 0, result.stdout
    combined = result.stdout + result.stderr
    assert expected in combined, (expected, combined)


def verify_receipt(receipt: str) -> str:
    with tempfile.NamedTemporaryFile("w", suffix=".json2", encoding="utf-8", delete=False) as handle:
        handle.write(receipt)
        receipt_path = Path(handle.name)
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
    assert verified.returncode == 0, (verified.stdout, verified.stderr)
    return verified.stdout.strip()


def replay_receipt(receipt: str, project: Path) -> subprocess.CompletedProcess[str]:
    receipt_path = project.parent / "training.json2"
    receipt_path.write_text(receipt, encoding="utf-8")
    try:
        return subprocess.run(
            [
                sys.executable,
                str(ROOT / "scripts" / "streaming" / "replay-receipt.py"),
                "--receipt",
                str(receipt_path),
                "--project",
                str(project),
                "--binary",
                str(BINARY),
            ],
            cwd=ROOT,
            text=True,
            capture_output=True,
            check=False,
        )
    finally:
        receipt_path.unlink(missing_ok=True)


def copy_fixture(root: Path, name: str) -> Path:
    destination = root / name
    shutil.copytree(PROJECT, destination)
    return destination


def main() -> None:
    assert BINARY.is_file(), BINARY
    source_files = sorted(PROJECT.joinpath("src").rglob("*.sely"))
    source_text = "\n".join(path.read_text(encoding="utf-8") for path in source_files)
    for forbidden in ("train_mlp", "predict_mlp", "train_linear_gpu", "load_checkpoint"):
        assert forbidden not in source_text, forbidden
    assert "from audit import emit" in source_text

    checked = run_seloryn("check", str(PROJECT), "--format=json")
    assert checked.returncode == 0, (checked.stdout, checked.stderr)

    with tempfile.TemporaryDirectory(prefix="seloryn-auditable-local-inference-") as temporary:
        root = Path(temporary)
        project = copy_fixture(root, "positive")
        train_started = time.perf_counter()
        trained = run_seloryn(
            "run", str(project), "--format=json", "--log=json2", stdin='{"mode":"train"}'
        )
        seloryn_seconds = time.perf_counter() - train_started
        assert trained.returncode == 0, (trained.stdout, trained.stderr)
        report = response_record(trained.stdout)
        assert report["schema"] == "seloryn.auditable-training.v1", report
        assert report["steps"] == 320, report
        assert report["initial_loss"] > report["final_loss"] > 0.0, report
        assert report["test_loss"] < 0.05, report
        assert abs(report["test_loss"] - report["reloaded_loss"]) < 1e-6, report
        assert report["test_accuracy"] == 1.0, report

        receipt_lines = [json.loads(line) for line in trained.stderr.splitlines() if line.strip()]
        assert len(receipt_lines) == 25, len(receipt_lines)
        assert receipt_lines[0]["kind"] == "run.start", receipt_lines[0]
        assert receipt_lines[-1]["kind"] == "run.end", receipt_lines[-1]
        identity = receipt_lines[0]["data"]["identity"]
        assert len(identity["train_sha256"]) == 64, identity
        assert len(identity["test_sha256"]) == 64, identity
        progress = [event for event in receipt_lines if event["kind"] == "training.progress"]
        assert len(progress) == 21, len(progress)
        assert progress[0]["data"]["step"] == 1, progress[0]
        assert progress[-1]["data"]["step"] == 320, progress[-1]
        assert progress[0]["data"]["metric"] > progress[-1]["data"]["metric"], progress
        receipt_verification = verify_receipt(trained.stderr)
        contract_events = [
            event
            for event in receipt_lines
            if event["kind"] == "audit.record"
            and event["data"]["audit_kind"] == "training.contract"
        ]
        assert len(contract_events) == 1, receipt_lines
        assert contract_events[0]["data"]["payload"]["request"] == {"mode": "train"}
        completion_events = [
            event
            for event in receipt_lines
            if event["kind"] == "audit.record"
            and event["data"]["audit_kind"] == "training.complete"
        ]
        assert len(completion_events) == 1, receipt_lines

        replayed = replay_receipt(trained.stderr, project)
        assert replayed.returncode == 0, (replayed.stdout, replayed.stderr)
        assert "program, datasets, result and checkpoint identity match" in replayed.stdout

        tampered_replay_project = copy_fixture(root, "tampered-replay")
        tampered_replay_train = tampered_replay_project / "data" / "train.json"
        tampered_replay_data = json.loads(tampered_replay_train.read_text(encoding="utf-8"))
        tampered_replay_data["inputs"][0] = tampered_replay_data["inputs"][0] + 0.01
        tampered_replay_train.write_text(json.dumps(tampered_replay_data), encoding="utf-8")
        rejected_replay = replay_receipt(trained.stderr, tampered_replay_project)
        assert rejected_replay.returncode != 0, rejected_replay.stdout
        assert "replay identity differs" in (rejected_replay.stdout + rejected_replay.stderr)

        checkpoint = project / "checkpoints" / "equipment-health.json"
        assert checkpoint.is_file(), checkpoint
        checkpoint_document = json.loads(checkpoint.read_text(encoding="utf-8"))
        assert checkpoint_document["schema"] == "seloryn.checkpoint.v1", checkpoint_document
        assert len(checkpoint_document["weights"]) == 25, checkpoint_document
        assert len(report["checkpoint_sha256"]) == 64, report

        inferred = run_seloryn(
            "run", str(project), "--format=json", "--log=json2", stdin=json.dumps(INFERENCE)
        )
        assert inferred.returncode == 0, (inferred.stdout, inferred.stderr)
        inference_report = response_record(inferred.stdout)
        assert inference_report["decision"] == "review", inference_report
        assert inference_report["checkpoint_sha256"] == report["checkpoint_sha256"], inference_report
        inference_events = [json.loads(line) for line in inferred.stderr.splitlines() if line.strip()]
        audit_events = [event for event in inference_events if event["kind"] == "audit.record"]
        assert len(audit_events) == 1, inference_events
        assert audit_events[0]["data"]["audit_kind"] == "inference.complete", audit_events[0]
        assert audit_events[0]["data"]["payload"]["checkpoint_sha256"] == report["checkpoint_sha256"]
        inference_receipt_verification = verify_receipt(inferred.stderr)

        wrong_order = dict(INFERENCE)
        wrong_order["feature_names"] = ["vibration", "temperature", "load", "age_days"]
        assert_failure(
            run_seloryn("run", str(project), "--format=json", stdin=json.dumps(wrong_order)),
            "feature order mismatch",
        )
        wrong_count = dict(INFERENCE)
        wrong_count["features"] = [0.90, 0.92, 0.88]
        assert_failure(
            run_seloryn("run", str(project), "--format=json", stdin=json.dumps(wrong_count)),
            "feature values do not match contract",
        )

        checkpoint_document["weights"][0] = checkpoint_document["weights"][0] + 0.25
        checkpoint.write_text(json.dumps(checkpoint_document), encoding="utf-8")
        assert_failure(
            run_seloryn("run", str(project), "--format=json", stdin=json.dumps(INFERENCE)),
            "checkpoint weights hash does not match payload",
        )

        malformed_dataset_project = copy_fixture(root, "malformed-dataset")
        malformed_train = malformed_dataset_project / "data" / "train.json"
        malformed = json.loads(malformed_train.read_text(encoding="utf-8"))
        malformed["inputs"] = malformed["inputs"][:-1]
        malformed_train.write_text(json.dumps(malformed), encoding="utf-8")
        assert_failure(
            run_seloryn("run", str(malformed_dataset_project), "--format=json", stdin='{"mode":"train"}'),
            "dataset input shape does not match contract",
        )

        invalid_target_project = copy_fixture(root, "invalid-target")
        invalid_test = invalid_target_project / "data" / "test.json"
        invalid = json.loads(invalid_test.read_text(encoding="utf-8"))
        invalid["targets"][0] = 0.5
        invalid_test.write_text(json.dumps(invalid), encoding="utf-8")
        assert_failure(
            run_seloryn("run", str(invalid_target_project), "--format=json", stdin='{"mode":"train"}'),
            "dataset target must be zero or one",
        )

        contract_project = copy_fixture(root, "invalid-contract")
        model_contract = contract_project / "contracts" / "model.json"
        model = json.loads(model_contract.read_text(encoding="utf-8"))
        model["hidden_size"] = 3
        model_contract.write_text(json.dumps(model), encoding="utf-8")
        assert_failure(
            run_seloryn("run", str(contract_project), "--format=json", stdin='{"mode":"train"}'),
            "fixture requires hidden size four",
        )

    sys.path.insert(0, str(PROJECT / "baseline"))
    reference = importlib.import_module("reference")
    reference_started = time.perf_counter()
    python_report = reference.train_reference()
    python_seconds = time.perf_counter() - reference_started
    for key in ("final_loss", "test_loss"):
        assert abs(python_report[key] - report[key]) < 1e-4, (key, python_report, report)
    assert python_report["accuracy"] == report["test_accuracy"] == 1.0

    points = [progress[index]["data"]["metric"] for index in (0, 5, 10, 15, 20)]
    curve = " -> ".join(
        f"{step}:{value:.6f}"
        for step, value in zip((1, 80, 160, 240, 320), points)
    )
    print("Auditable local inference proof: PASS")
    print(
        "gates | check=0 train=0 infer=0 receipt_train=0 receipt_infer=0 "
        "negative_order=65 negative_count=65 negative_dataset=65 negative_target=65 "
        "negative_contract=65 negative_checkpoint=65"
    )
    print(f"receipt | train_events={len(receipt_lines)} train_progress={len(progress)} verify={receipt_verification}")
    print(f"receipt | inference_events={len(inference_events)} audit_events={len(audit_events)} verify={inference_receipt_verification}")
    print("comparison | backend | initial_loss | final_loss | test_loss | accuracy | seconds")
    print(
        f"comparison | Seloryn/.sely | {report['initial_loss']:.8f} | {report['final_loss']:.8f} | "
        f"{report['test_loss']:.8f} | {report['test_accuracy']:.6f} | {seloryn_seconds:.4f}"
    )
    print(
        f"comparison | Python/pure  | {python_report['initial_loss']:.8f} | {python_report['final_loss']:.8f} | "
        f"{python_report['test_loss']:.8f} | {python_report['accuracy']:.6f} | {python_seconds:.4f}"
    )
    print(f"loss_curve checkpoints: {curve}")
    print(f"checkpoint_sha256={report['checkpoint_sha256']}")


if __name__ == "__main__":
    try:
        main()
    except (AssertionError, OSError, subprocess.SubprocessError, UnicodeError, json.JSONDecodeError) as error:
        print(f"auditable local inference probe FAILED: {error!r}", file=sys.stderr)
        raise SystemExit(1) from error
