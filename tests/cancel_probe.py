"""Black-box proof of cooperative cancellation and an explicit JSON2 receipt end."""

from __future__ import annotations

import json
import os
import signal
import subprocess
import sys
import tempfile
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target" / "release" / ("seloryn.exe" if sys.platform == "win32" else "seloryn")
VERIFY = ROOT / "scripts" / "streaming" / "verify-event-chain.py"


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="seloryn-cancel-") as temporary:
        project = Path(temporary)
        (project / "src").mkdir()
        (project / "data").mkdir()
        (project / "checkpoints").mkdir()
        (project / "seloryn.toml").write_text(
            """[package]
name = "cancellation-probe"
language = "0.02"
source_root = "src"
entry = "main.sely"

[capabilities]
read = ["data"]
write = ["checkpoints"]
""",
            encoding="utf-8",
        )
        (project / "data" / "train.json").write_text(
            '{"inputs":[[-2,-2],[-2,0],[-2,2],[0,-2],[0,0],[0,2],[2,-2],[2,0],[2,2]],'
            '"targets":[[-5.5],[-1.5],[2.5],[-3.5],[0.5],[4.5],[-1.5],[2.5],[6.5]]}\n',
            encoding="utf-8",
        )
        (project / "data" / "test.json").write_text(
            '{"inputs":[[-1,-1],[-1,1],[1,-1],[1,1],[3,0]],'
            '"targets":[[-2.5],[1.5],[-0.5],[3.5],[3.5]]}\n',
            encoding="utf-8",
        )
        (project / "src" / "main.sely").write_text(
            "from training import train_mlp\n\n"
            "fn main() -> Json:\n"
            '    return train_mlp("data/train.json", "data/test.json", '
            '"checkpoints/model.json", learning_rate=0.02, hidden_size=8, '
            "batch_size=3, steps=1000000, seed=17)\n",
            encoding="utf-8",
        )
        receipt = project / "receipt.ndjson"
        stdout = project / "stdout.txt"
        with receipt.open("wb") as receipt_handle, stdout.open("wb") as stdout_handle:
            creationflags = getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0)
            process = subprocess.Popen(
                [str(BINARY), "run", str(project), "--format=json", "--log=json2"],
                cwd=project,
                stdin=subprocess.DEVNULL,
                stdout=stdout_handle,
                stderr=receipt_handle,
                creationflags=creationflags,
            )

        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            if process.poll() is not None:
                raise AssertionError("process finished before cancellation")
            if '"kind":"training.step"' in receipt.read_text(encoding="utf-8"):
                break
            time.sleep(0.01)
        else:
            process.kill()
            process.wait(timeout=10)
            raise AssertionError("did not observe a live training step")

        if sys.platform == "win32":
            process.send_signal(signal.CTRL_BREAK_EVENT)
        else:
            process.send_signal(signal.SIGINT)
        returncode = process.wait(timeout=15)

        lines = [json.loads(line) for line in receipt.read_text(encoding="utf-8").splitlines() if line]
        assert returncode == 130, returncode
        assert not stdout.read_text(encoding="utf-8"), "cancelled run emitted a result"
        assert lines[-1]["kind"] == "run.end", lines[-1]
        assert lines[-1]["data"]["status"] == "error", lines[-1]
        assert lines[-1]["data"]["error_code"] == "SEL-CANCELLED", lines[-1]
        verified = subprocess.run(
            [sys.executable, str(VERIFY), str(receipt)],
            check=False,
            capture_output=True,
            text=True,
        )
        assert verified.returncode == 0, verified.stderr or verified.stdout
        print("cooperative cancellation: run.end=SEL-CANCELLED receipt=valid exit=130")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
