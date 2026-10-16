"""Independent proof for bounded MLP checkpoint batch inference."""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target" / "release" / ("seloryn.exe" if sys.platform == "win32" else "seloryn")
VERIFIER = ROOT / "scripts" / "streaming" / "verify-event-chain.py"

TRAIN = {
    "inputs": [[-2, -2], [-2, 0], [-2, 2], [0, -2], [0, 0], [0, 2], [2, -2], [2, 0], [2, 2]],
    "targets": [[-5.5], [-1.5], [2.5], [-3.5], [0.5], [4.5], [-1.5], [2.5], [6.5]],
}
TEST = {
    "inputs": [[-1, -1], [-1, 1], [1, -1], [1, 1], [3, 0]],
    "targets": [[-2.5], [1.5], [-0.5], [3.5], [3.5]],
}
INPUTS = {"inputs": [[-1.0, -1.0], [0.0, 0.0], [1.0, 1.0], [2.0, -1.0]]}


def run(project: Path, *extra: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [str(BINARY), "run", str(project), "--format=json", *extra],
        cwd=ROOT,
        text=True,
        encoding="utf-8",
        errors="replace",
        capture_output=True,
        check=False,
    )


def write_training_project(project: Path) -> None:
    (project / "src").mkdir(parents=True, exist_ok=True)
    (project / "data").mkdir(exist_ok=True)
    (project / "checkpoints").mkdir(exist_ok=True)
    (project / "seloryn.toml").write_text(
        """[package]
name = "inference-training"
language = "0.09"
source_root = "src"
entry = "main.sely"

[capabilities]
read = ["data"]
write = ["checkpoints"]

[limits]
json_max_bytes = 1048576
json_max_depth = 32
""",
        encoding="utf-8",
    )
    (project / "data" / "train.json").write_text(json.dumps(TRAIN) + "\n", encoding="utf-8")
    (project / "data" / "test.json").write_text(json.dumps(TEST) + "\n", encoding="utf-8")
    (project / "src" / "main.sely").write_text(
        'from training import train_mlp\n\nfn main() -> Json:\n'
        '    return train_mlp("data/train.json", "data/test.json", '
        '"checkpoints/model.json", seed=17, steps=640, batch_size=3, hidden_size=8, learning_rate=0.02)\n',
        encoding="utf-8",
    )


def write_inference_project(project: Path, checkpoint: Path, inputs: dict) -> None:
    (project / "src").mkdir(parents=True, exist_ok=True)
    (project / "data").mkdir(exist_ok=True)
    (project / "checkpoints").mkdir(exist_ok=True)
    shutil.copy2(checkpoint, project / "checkpoints" / "model.json")
    (project / "data" / "inputs.json").write_text(json.dumps(inputs) + "\n", encoding="utf-8")
    (project / "seloryn.toml").write_text(
        """[package]
name = "inference-consumer"
language = "0.09"
source_root = "src"
entry = "main.sely"

[capabilities]
read = ["data", "checkpoints"]

[limits]
json_max_bytes = 1048576
json_max_depth = 32
""",
        encoding="utf-8",
    )
    (project / "src" / "main.sely").write_text(
        'from training import predict_mlp\n\nfn main() -> Json:\n'
        '    return predict_mlp("checkpoints/model.json", "data/inputs.json")\n',
        encoding="utf-8",
    )


def reference_predictions(checkpoint: dict, inputs: list[list[float]]) -> list[list[float]]:
    weights = checkpoint["weights"]
    input_size = checkpoint["input_size"]
    hidden_size = checkpoint["hidden_size"]
    output_size = checkpoint["output_size"]
    predictions = []
    for row in inputs:
        hidden = []
        for hidden_index in range(hidden_size):
            value = weights["first_bias"][hidden_index]
            value += sum(row[i] * weights["first_weights"][i * hidden_size + hidden_index] for i in range(input_size))
            hidden.append(max(value, 0.0))
        predictions.append([
            weights["second_bias"][output_index]
            + sum(hidden[h] * weights["second_weights"][h * output_size + output_index] for h in range(hidden_size))
            for output_index in range(output_size)
        ])
    return predictions


def main() -> int:
    if not BINARY.is_file():
        raise AssertionError(f"release binary is missing: {BINARY}")
    with tempfile.TemporaryDirectory(prefix="seloryn-mlp-inference-") as directory:
        root = Path(directory)
        training = root / "training"
        write_training_project(training)
        trained = run(training)
        assert trained.returncode == 0, trained.stderr
        checkpoint_path = training / "checkpoints" / "model.json"
        checkpoint = json.loads(checkpoint_path.read_text(encoding="utf-8"))

        inference = root / "inference"
        write_inference_project(inference, checkpoint_path, INPUTS)
        first = run(inference)
        second = run(inference)
        assert first.returncode == 0, first.stderr
        assert second.returncode == 0, second.stderr
        assert first.stdout == second.stdout, "inference output is not deterministic"
        result = json.loads(first.stdout)["value"]
        assert result["operation"] == "predict_mlp"
        assert result["checkpoint_step"] == 640
        assert result["samples"] == len(INPUTS["inputs"])
        expected = reference_predictions(checkpoint, INPUTS["inputs"])
        for actual_row, expected_row in zip(result["predictions"], expected):
            for actual, expected_value in zip(actual_row, expected_row):
                assert abs(actual - expected_value) < 1.0e-5, (actual, expected_value)

        receipt = root / "receipt.ndjson"
        logged = run(inference, "--log=json2")
        assert logged.returncode == 0, logged.stderr
        receipt.write_text(logged.stderr, encoding="utf-8")
        verified = subprocess.run(
            [sys.executable, str(VERIFIER), str(receipt)],
            cwd=ROOT,
            text=True,
            encoding="utf-8",
            capture_output=True,
            check=False,
        )
        assert verified.returncode == 0, verified.stdout + verified.stderr
        assert "inference.start" in logged.stderr
        assert "inference.complete" in logged.stderr

        corrupted = json.loads(checkpoint_path.read_text(encoding="utf-8"))
        corrupted["weights_sha256"] = "0" * 64
        (inference / "checkpoints" / "model.json").write_text(json.dumps(corrupted) + "\n", encoding="utf-8")
        rejected = run(inference)
        assert rejected.returncode == 65
        assert "weights hash does not match" in rejected.stderr

        write_inference_project(inference, checkpoint_path, {"inputs": [[1.0, 2.0, 3.0]]})
        dimension_error = run(inference)
        assert dimension_error.returncode == 65
        assert "dimension" in dimension_error.stderr
    print("Seloryn MLP inference verification: deterministic batch predictions, receipt and rejection passed")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (AssertionError, OSError, json.JSONDecodeError) as error:
        print(f"Seloryn MLP inference FAILED: {error!r}", file=sys.stderr)
        raise SystemExit(1) from error
