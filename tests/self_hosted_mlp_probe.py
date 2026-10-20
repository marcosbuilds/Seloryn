"""Black-box proof for a custom neural network authored entirely in `.sely`."""

from __future__ import annotations

import importlib.util
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target" / "release" / ("seloryn.exe" if sys.platform == "win32" else "seloryn")
PROJECT = ROOT / "examples" / "projects" / "self-hosted-mlp"
VERIFIER = ROOT / "scripts" / "streaming" / "verify-event-chain.py"


def verify_chain(lines: list[str]) -> tuple[bool, str]:
    spec = importlib.util.spec_from_file_location("seloryn_event_chain", VERIFIER)
    assert spec is not None and spec.loader is not None
    verifier = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(verifier)
    valid, message, _ = verifier.verify(lines)
    return valid, message


def main() -> int:
    assert BINARY.is_file(), BINARY
    with tempfile.TemporaryDirectory(prefix="seloryn-self-hosted-mlp-") as directory:
        project = Path(directory) / "custom-mlp"
        shutil.copytree(PROJECT, project)
        source = (project / "src" / "self-hosted-mlp-entry.sely").read_text(encoding="utf-8")
        assert "train_mlp" not in source
        assert "from training import progress" in source

        checked = subprocess.run(
            [str(BINARY), "check", str(project), "--format=json"],
            cwd=ROOT,
            text=True,
            encoding="utf-8",
            capture_output=True,
            check=False,
        )
        assert checked.returncode == 0, checked.stderr or checked.stdout

        process = subprocess.run(
            [str(BINARY), "run", str(project), "--format=json", "--log=json2"],
            cwd=ROOT,
            text=True,
            encoding="utf-8",
            capture_output=True,
            check=False,
            timeout=40,
        )
        assert process.returncode == 0, process.stderr
        envelope = json.loads(process.stdout)
        report = envelope["value"]["value"]["fields"]
        assert report["architecture"] == "custom_relu_mlp", report
        assert report["initial_loss"] > report["final_loss"], report
        assert report["evaluation_loss"] < 0.5, report
        assert abs(report["evaluation_loss"] - report["reloaded_loss"]) < 1.0e-6, report

        lines = [line for line in process.stderr.splitlines() if line.strip()]
        valid, message = verify_chain(lines)
        assert valid, message
        receipt = [json.loads(line) for line in lines]
        progress = [event for event in receipt if event.get("kind") == "training.progress"]
        assert receipt[0]["kind"] == "run.start"
        assert receipt[-1]["kind"] == "run.end"
        assert len(progress) == 160, len(progress)
        assert progress[0]["data"]["step"] == 1
        assert progress[-1]["data"]["step"] == 160
        assert receipt[-1]["data"]["status"] == "ok"

        checkpoint = json.loads((project / "checkpoints" / "custom-model.json").read_text(encoding="utf-8"))
        assert checkpoint["schema"] == "seloryn.sely.model.v1", checkpoint
        assert checkpoint["architecture"] == "custom_relu_mlp", checkpoint
        assert len(checkpoint["weights_sha256"]) == 64, checkpoint
        assert len(checkpoint["weights"]) == 13, checkpoint
    print("Seloryn custom .sely MLP: dataset, forward/backward, SGD, evaluation, checkpoint reload, inference and streaming passed")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (AssertionError, OSError, json.JSONDecodeError) as error:
        print(f"Seloryn custom .sely MLP FAILED: {error!r}", file=sys.stderr)
        raise SystemExit(1) from error
