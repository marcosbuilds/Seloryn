"""Black-box proof for Seloryn's bounded, non-fallback GPU linear trainer."""

from __future__ import annotations

import json
import importlib.util
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target" / "release" / ("seloryn.exe" if sys.platform == "win32" else "seloryn")
PROJECT = ROOT / "examples" / "projects" / "gpu-linear-training"
VERIFIER = ROOT / "scripts" / "streaming" / "verify-event-chain.py"


def main() -> int:
    if not BINARY.is_file():
        raise AssertionError(BINARY)
    with tempfile.TemporaryDirectory(prefix="seloryn-gpu-training-") as directory:
        project = Path(directory) / "gpu"
        shutil.copytree(PROJECT, project)
        process = subprocess.run(
            [str(BINARY), "run", str(project), "--format=json", "--log=json"],
            cwd=ROOT,
            text=True,
            encoding="utf-8",
            errors="replace",
            capture_output=True,
            check=False,
        )
        devices = subprocess.run(
            [str(BINARY), "devices", "--format=json"],
            cwd=ROOT,
            text=True,
            encoding="utf-8",
            errors="replace",
            capture_output=True,
            check=False,
        )
        device_payload = json.loads(devices.stdout) if devices.returncode == 0 else {}
        adapters = device_payload.get("devices", [])
        physical_gpu = any(
            adapter.get("device_type") in {"discretegpu", "integratedgpu"}
            for adapter in adapters
        )
        if not physical_gpu:
            assert process.returncode != 0, process.stdout
            print("Seloryn GPU training: capability-aware skip (no physical GPU)")
            return 0
        assert process.returncode == 0, process.stderr
        result = json.loads(process.stdout)
        value = result["value"]
        assert value["operation"] == "train_linear_gpu", value
        assert value["device"] == "gpu", value
        assert value["fallback"] is False, value
        assert value["dispatches"] == value["steps"] == 300, value
        assert value["readbacks"] == 1, value
        assert value["loss_decreased"] is True, value
        assert abs(value["weight"] - 2.0) < 0.05, value
        assert abs(value["bias"] - 1.0) < 0.05, value
        assert '"device":"gpu"' in process.stderr, process.stderr
        assert '"operation":"train_linear_gpu"' in process.stderr, process.stderr
        stream_project = Path(directory) / "gpu-stream"
        shutil.copytree(PROJECT, stream_project)
        stream = subprocess.run(
            [str(BINARY), "run", str(stream_project), "--log=json2"],
            cwd=ROOT,
            text=True,
            encoding="utf-8",
            errors="replace",
            capture_output=True,
            check=False,
        )
        assert stream.returncode == 0, stream.stderr
        spec = importlib.util.spec_from_file_location("seloryn_event_chain", VERIFIER)
        assert spec is not None and spec.loader is not None
        verifier = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(verifier)
        stream_lines = [line for line in stream.stderr.splitlines() if line.strip()]
        valid, message, _ = verifier.verify(stream_lines)
        assert valid, message
        stream_events = [json.loads(line) for line in stream_lines]
        assert stream_events[1]["data"]["operation"] == "train_linear_gpu"
        assert stream_events[-1]["data"]["result_sha256"]
    print("Seloryn GPU training: device-resident linear update and readback passed")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (AssertionError, OSError, json.JSONDecodeError) as error:
        print(f"Seloryn GPU training FAILED: {error!r}", file=sys.stderr)
        raise SystemExit(1) from error
