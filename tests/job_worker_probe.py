"""Black-box proof that a validated job executes through the local worker."""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target" / "release" / ("seloryn.exe" if sys.platform == "win32" else "seloryn")


def run(root: Path, *arguments: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [str(BINARY), "job", *arguments, f"--root={root}", "--format=json"],
        cwd=ROOT,
        text=True,
        encoding="utf-8",
        errors="replace",
        capture_output=True,
        check=False,
    )


def ok(result: subprocess.CompletedProcess[str]) -> dict:
    assert result.returncode == 0, (result.returncode, result.stdout, result.stderr)
    assert result.stderr == "", result.stderr
    return json.loads(result.stdout)


def main() -> int:
    assert BINARY.is_file(), f"missing release binary: {BINARY}"
    source = "fn main() -> I64:\n    return 7\n"
    source_bytes = source.encode("utf-8")
    with tempfile.TemporaryDirectory(prefix="seloryn-job-worker-") as temporary:
        root = Path(temporary)
        (root / "main.sely").write_bytes(source_bytes)
        spec = root / "spec.json"
        spec.write_text(
            json.dumps(
                {
                    "kind": "seloryn-run.v1",
                    "source": "main.sely",
                    "source_sha256": hashlib.sha256(source_bytes).hexdigest(),
                    "timeout_ms": 2_000,
                },
                sort_keys=True,
            )
            + "\n",
            encoding="utf-8",
        )

        created = ok(run(root, "create", str(spec), "--id=worker"))
        assert created["status"] == "accepted", created
        queued = ok(run(root, "transition", "worker", "queued"))
        assert queued["status"] == "queued", queued
        executed = ok(run(root, "run", "worker", "--owner=probe"))
        assert executed["schema"] == "seloryn.job-result.v1", executed
        assert executed["status"] == "succeeded", executed
        assert executed["timed_out"] is False, executed
        assert executed["cancelled"] is False, executed
        assert executed["committed"] is True, executed
        assert executed["stdout_bytes"] > 0, executed

        status = ok(run(root, "status", "worker"))
        assert status["status"] == "succeeded", status
        result_path = root / ".seloryn" / "jobs" / "worker" / "result.json"
        receipt = json.loads(result_path.read_text(encoding="utf-8"))
        assert receipt["status"] == "succeeded"
        assert receipt["committed"] is True

    print("Seloryn job worker verification: preflight, lease, bounded execution, receipt and terminal state passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
