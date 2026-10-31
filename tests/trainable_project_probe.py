"""Black-box proof for the generated, source-owned Trainable Project v1."""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target" / "release" / ("seloryn.exe" if sys.platform == "win32" else "seloryn")


def run(project: Path, request: dict, *, log: str = "off") -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [str(BINARY), "run", str(project), "--format=json", f"--log={log}"],
        cwd=ROOT,
        input=json.dumps(request),
        text=True,
        capture_output=True,
        check=False,
    )


def fields(result: subprocess.CompletedProcess[str]) -> dict:
    assert result.returncode == 0, (result.stdout, result.stderr)
    envelope = json.loads(result.stdout)
    value = envelope["value"]
    assert value["ok"] is True, envelope
    enum = value["value"]
    assert enum["type"] == "enum", enum
    return enum["fields"][0]["fields"]


def assert_failure(result: subprocess.CompletedProcess[str], message: str) -> None:
    assert result.returncode == 65, (result.stdout, result.stderr)
    assert message in result.stdout + result.stderr, (message, result.stdout, result.stderr)


def verify_receipt(receipt: str) -> None:
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


def main() -> None:
    assert BINARY.is_file(), BINARY
    with tempfile.TemporaryDirectory(prefix="seloryn-trainable-project-") as temporary:
        root = Path(temporary)
        project = root / "starter"
        initialized = subprocess.run(
            [str(BINARY), "init", str(project), "--template=trainable-mlp", "--format=json"],
            cwd=ROOT,
            text=True,
            capture_output=True,
            check=False,
        )
        assert initialized.returncode == 0, (initialized.stdout, initialized.stderr)
        assert len(json.loads(initialized.stdout)["files"]) >= 18
        assert (project / "src" / "app" / "training.sely").is_file()
        assert (project / "contracts" / "run.json").is_file()

        source = "\n".join(path.read_text(encoding="utf-8") for path in project.rglob("*.sely"))
        for forbidden in ("train_mlp", "predict_mlp", "train_linear_gpu", "load_checkpoint"):
            assert forbidden not in source, forbidden
        assert "from autograd import gradients" in source
        assert "from app.parameters import Parameter" in source

        checked = subprocess.run(
            [str(BINARY), "check", str(project), "--format=json"],
            cwd=ROOT,
            text=True,
            capture_output=True,
            check=False,
        )
        assert checked.returncode == 0, (checked.stdout, checked.stderr)

        trained = run(project, {"mode": "train"}, log="json2")
        train_report = fields(trained)
        assert train_report["schema"] == "seloryn.trainable-training.v1", train_report
        assert train_report["initial_loss"] > train_report["final_loss"] > 0.0, train_report
        assert train_report["test_accuracy"] == 1.0, train_report
        assert train_report["reloaded_loss"] == train_report["test_loss"], train_report
        assert len(train_report["checkpoint_sha256"]) == 64, train_report
        assert "audit.record" in trained.stderr, trained.stderr
        assert "run.start" not in trained.stdout, trained.stdout
        verify_receipt(trained.stderr)

        repeated = run(project, {"mode": "train"})
        repeated_report = fields(repeated)
        assert repeated_report["checkpoint_sha256"] == train_report["checkpoint_sha256"], repeated_report
        assert repeated_report["final_loss"] == train_report["final_loss"], repeated_report

        source_owned = root / "source-owned-change"
        shutil.copytree(project, source_owned)
        model_source = source_owned / "src" / "app" / "model.sely"
        model_source.write_text(
            model_source.read_text(encoding="utf-8").replace("0.2, -0.1", "0.9, -0.1", 1),
            encoding="utf-8",
        )
        source_report = fields(run(source_owned, {"mode": "train"}))
        assert source_report["checkpoint_sha256"] != train_report["checkpoint_sha256"], source_report

        evaluated = run(project, {"mode": "eval"}, log="json2")
        eval_report = fields(evaluated)
        assert eval_report["schema"] == "seloryn.evaluation.v1", eval_report
        assert eval_report["checkpoint_sha256"] == train_report["checkpoint_sha256"], eval_report
        assert eval_report["test_accuracy"] == train_report["test_accuracy"], eval_report
        assert '"audit_kind":"training.contract"' not in evaluated.stderr
        assert '"audit_kind":"evaluation.complete"' in evaluated.stderr
        verify_receipt(evaluated.stderr)

        inferred = run(
            project,
            {
                "mode": "infer",
                "feature_names": ["signal_a", "signal_b", "signal_c", "signal_d"],
                "features": [0.90, 0.92, 0.88, 0.91],
            },
            log="json2",
        )
        inference_report = fields(inferred)
        assert inference_report["schema"] == "seloryn.inference.v1", inference_report
        assert inference_report["checkpoint_sha256"] == train_report["checkpoint_sha256"], inference_report
        assert '"audit_kind":"inference.complete"' in inferred.stderr
        verify_receipt(inferred.stderr)

        free_layout = root / "free-layout"
        shutil.copytree(project, free_layout)
        source_root = free_layout / "src"
        free_source_root = free_layout / "code"
        source_root.rename(free_source_root)
        (free_source_root / "app").rename(free_source_root / "domain")
        entry = free_source_root / "main.sely"
        entry.rename(free_source_root / "boot.sely")
        manifest = free_layout / "seloryn.toml"
        manifest.write_text(
            manifest.read_text(encoding="utf-8")
            .replace('source_root = "src"', 'source_root = "code"')
            .replace('entry = "main.sely"', 'entry = "boot.sely"'),
            encoding="utf-8",
        )
        for source_file in free_source_root.rglob("*.sely"):
            source_file.write_text(
                source_file.read_text(encoding="utf-8").replace("app.", "domain."),
                encoding="utf-8",
            )
        (free_source_root / "ignored.sely").write_text(
            "this source is intentionally outside the import graph\n",
            encoding="utf-8",
        )
        free_check = subprocess.run(
            [str(BINARY), "check", str(free_layout), "--format=json"],
            cwd=ROOT,
            text=True,
            capture_output=True,
            check=False,
        )
        assert free_check.returncode == 0, (free_check.stdout, free_check.stderr)
        free_train = run(free_layout, {"mode": "train"})
        free_train_report = fields(free_train)
        assert free_train_report["final_loss"] < free_train_report["initial_loss"], free_train_report
        assert fields(run(free_layout, {"mode": "eval"}))["test_accuracy"] == 1.0
        assert fields(
            run(
                free_layout,
                {
                    "mode": "infer",
                    "feature_names": ["signal_a", "signal_b", "signal_c", "signal_d"],
                    "features": [0.90, 0.92, 0.88, 0.91],
                },
            )
        )["checkpoint_sha256"] == free_train_report["checkpoint_sha256"]

        outside = root / "outside.json"
        outside.write_text("{}", encoding="utf-8")
        escaped_read = root / "escaped-read"
        shutil.copytree(project, escaped_read)
        data_source = escaped_read / "src" / "app" / "training.sely"
        data_source.write_text(
            data_source.read_text(encoding="utf-8").replace('"data/train.json"', '"../outside.json"'),
            encoding="utf-8",
        )
        assert_failure(run(escaped_read, {"mode": "train"}), "granted read root")

        escaped_write = root / "escaped-write"
        shutil.copytree(project, escaped_write)
        checkpoint_source = escaped_write / "src" / "app" / "training.sely"
        checkpoint_source.write_text(
            checkpoint_source.read_text(encoding="utf-8").replace(
                '"checkpoints/model.json"', '"data/model.json"'
            ),
            encoding="utf-8",
        )
        assert_failure(run(escaped_write, {"mode": "train"}), "declared write capability")

        wrong_order = run(
            project,
            {
                "mode": "infer",
                "feature_names": ["signal_b", "signal_a", "signal_c", "signal_d"],
                "features": [0.90, 0.92, 0.88, 0.91],
            },
        )
        assert_failure(wrong_order, "feature order mismatch")

        changed_contract = root / "changed-contract"
        shutil.copytree(project, changed_contract)
        model_contract = changed_contract / "contracts" / "model.json"
        model_document = json.loads(model_contract.read_text(encoding="utf-8"))
        model_document["steps"] = 319
        model_contract.write_text(json.dumps(model_document), encoding="utf-8")
        assert_failure(run(changed_contract, {"mode": "eval"}), "model contract hash does not match checkpoint")

        changed_checkpoint = root / "changed-checkpoint"
        shutil.copytree(project, changed_checkpoint)
        checkpoint = changed_checkpoint / "checkpoints" / "model.json"
        checkpoint_document = json.loads(checkpoint.read_text(encoding="utf-8"))
        checkpoint_document["weights"][0] += 0.25
        checkpoint.write_text(json.dumps(checkpoint_document), encoding="utf-8")
        assert_failure(
            run(
                changed_checkpoint,
                {
                    "mode": "infer",
                    "feature_names": ["signal_a", "signal_b", "signal_c", "signal_d"],
                    "features": [0.90, 0.92, 0.88, 0.91],
                },
            ),
            "checkpoint weights hash does not match payload",
        )

        for name, relative_path, mutation, expected in [
            (
                "changed-dataset-contract",
                Path("contracts/dataset.json"),
                lambda document: document.__setitem__("revision", "tampered"),
                "dataset contract hash does not match checkpoint",
            ),
            (
                "changed-run-contract",
                Path("contracts/run.json"),
                lambda document: document.__setitem__("revision", "tampered"),
                "run contract hash does not match checkpoint",
            ),
            (
                "changed-model-dtype",
                Path("contracts/model.json"),
                lambda document: document.__setitem__("dtype", "f64"),
                "starter supports only f32",
            ),
            (
                "changed-model-threshold",
                Path("contracts/model.json"),
                lambda document: document.__setitem__("threshold", 1.5),
                "threshold must be between zero and one",
            ),
            (
                "changed-train-data",
                Path("data/train.json"),
                lambda document: document["inputs"].__setitem__(0, document["inputs"][0] + 0.01),
                "training dataset changed since checkpoint",
            ),
            (
                "changed-test-data",
                Path("data/test.json"),
                lambda document: document["inputs"].__setitem__(0, document["inputs"][0] + 0.01),
                "test dataset changed since checkpoint",
            ),
        ]:
            changed = root / name
            shutil.copytree(project, changed)
            target = changed / relative_path
            document = json.loads(target.read_text(encoding="utf-8"))
            mutation(document)
            target.write_text(json.dumps(document), encoding="utf-8")
            assert_failure(run(changed, {"mode": "eval"}), expected)

    print("Seloryn Trainable Project v1 proof: PASS")
    print(
        "gates | init=0 check=0 train=0 eval=0 infer=0 "
        "repeat_train=0 "
        "free_layout=0 receipts=0 capability_read=65 capability_write=65 "
        "negative_order=65 negative_contract=65 "
        "negative_checkpoint=65 negative_dataset_contract=65 negative_run_contract=65 "
        "negative_model_dtype=65 negative_threshold=65 negative_train_data=65 negative_test_data=65 "
        "source_owned=0 source_change_observed=0 no_builtin_training=0"
    )


if __name__ == "__main__":
    try:
        main()
    except (AssertionError, OSError, subprocess.SubprocessError, UnicodeError, json.JSONDecodeError) as error:
        print(f"trainable project probe FAILED: {error!r}", file=sys.stderr)
        raise SystemExit(1) from error
