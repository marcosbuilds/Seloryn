"""Black-box proof that concurrent clients cannot lose a job transition."""

from __future__ import annotations

import concurrent.futures
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


def main() -> int:
    assert BINARY.is_file(), f"missing release binary: {BINARY}"
    with tempfile.TemporaryDirectory(prefix="seloryn-job-concurrency-") as temporary:
        root = Path(temporary)
        spec = root / "spec.json"
        spec.write_text('{"kind":"train","seed":13}\n', encoding="utf-8")

        created = run(root, "create", str(spec), "--id=shared")
        assert created.returncode == 0, (created.returncode, created.stdout, created.stderr)

        def create_again(_: int) -> subprocess.CompletedProcess[str]:
            return run(root, "create", str(spec), "--id=shared")

        with concurrent.futures.ThreadPoolExecutor(max_workers=32) as pool:
            creations = list(pool.map(create_again, range(1000)))
        assert all(result.returncode == 0 for result in creations), [
            (result.returncode, result.stdout, result.stderr)
            for result in creations
            if result.returncode != 0
        ][:5]
        assert all(json.loads(result.stdout)["reused"] for result in creations)

        queued = run(root, "transition", "shared", "queued")
        assert queued.returncode == 0, (queued.returncode, queued.stdout, queued.stderr)

        def claim_again(index: int) -> subprocess.CompletedProcess[str]:
            return run(root, "claim", "shared", f"--owner=worker-{index}")

        with concurrent.futures.ThreadPoolExecutor(max_workers=32) as pool:
            transitions = list(pool.map(claim_again, range(1000)))
        successful_claims = [result for result in transitions if result.returncode == 0]
        assert len(successful_claims) == 1, [
            (result.returncode, result.stdout, result.stderr)
            for result in transitions
        ][:5]
        assert all(
            result.returncode == 65 and json.loads(result.stderr)["code"] in ("SEL-J014", "SEL-J011")
            for result in transitions
            if result.returncode != 0
        )
        assert json.loads(successful_claims[0].stdout)["status"] == "starting"

        events = (root / ".seloryn" / "jobs" / "shared" / "events.jsonl").read_text(
            encoding="utf-8"
        ).splitlines()
        assert len(events) == 3, events
        parsed = [json.loads(line) for line in events]
        assert [event["sequence"] for event in parsed] == [0, 1, 2], parsed
        assert [event["to"] for event in parsed] == ["accepted", "queued", "starting"], parsed

    print("Seloryn job concurrency verification: 1000 idempotent creates and transitions kept one ordered history")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
