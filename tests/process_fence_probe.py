"""Black-box proof that worker timeout fences a descendant process tree."""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time


ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target" / "release" / ("seloryn.exe" if sys.platform == "win32" else "seloryn")
HELPER_SOURCE = ROOT / "tests" / "fixtures" / "process_fence_helper.rs"


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


def process_exists(pid: int) -> bool:
    if sys.platform == "win32":
        result = subprocess.run(
            ["tasklist", "/FI", f"PID eq {pid}", "/FO", "CSV", "/NH"],
            capture_output=True,
            text=True,
            check=False,
        )
        return f'"{pid}"' in result.stdout
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


def main() -> int:
    assert BINARY.is_file(), BINARY
    assert HELPER_SOURCE.is_file(), HELPER_SOURCE
    with tempfile.TemporaryDirectory(prefix="seloryn-process-fence-") as temporary:
        root = Path(temporary)
        helper = root / ("seloryn.exe" if sys.platform == "win32" else "seloryn")
        compiled = subprocess.run(
            ["rustc", "--edition=2024", str(HELPER_SOURCE), "-O", "-o", str(helper)],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
        assert compiled.returncode == 0, compiled.stderr

        source = b"fn main() -> I64:\n    return 0\n"
        (root / "main.sely").write_bytes(source)
        spec = root / "spec.json"
        spec.write_text(
            json.dumps(
                {
                    "kind": "seloryn-run.v1",
                    "source": "main.sely",
                    "source_sha256": hashlib.sha256(source).hexdigest(),
                    "timeout_ms": 300,
                },
                sort_keys=True,
            )
            + "\n",
            encoding="utf-8",
        )
        created = run(root, "create", str(spec), "--id=fence")
        assert created.returncode == 0, (created.stdout, created.stderr)
        queued = run(root, "transition", "fence", "queued")
        assert queued.returncode == 0, (queued.stdout, queued.stderr)

        worker = subprocess.Popen(
            [
                str(BINARY),
                "job",
                "run",
                "fence",
                "--owner=probe",
                f"--binary={helper}",
                f"--root={root}",
                "--format=json",
            ],
            cwd=ROOT,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            encoding="utf-8",
        )
        ready = root / "fence-ready"
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline and not ready.exists():
            assert worker.poll() is None, worker.communicate()
            time.sleep(0.01)
        assert ready.exists(), "helper did not reach the fence barrier"
        (root / "fence-go").write_text("go\n", encoding="utf-8")
        stdout, stderr = worker.communicate(timeout=5)
        assert worker.returncode == 0, (worker.returncode, stdout, stderr)
        result = json.loads(stdout)
        assert stderr == "", stderr
        assert result["status"] == "cancelled", result
        assert result["timed_out"] is True, result
        assert result["fence_kind"] in {"windows-job-object", "posix-process-group"}, result

        pid_path = root / "fence-descendant.pid"
        assert pid_path.exists(), "helper did not create a descendant"
        descendant = int(pid_path.read_text(encoding="utf-8"))
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline and process_exists(descendant):
            time.sleep(0.05)
        assert not process_exists(descendant), f"descendant {descendant} survived the fence"

    print("Seloryn process-fence verification: timeout terminated the descendant tree")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
