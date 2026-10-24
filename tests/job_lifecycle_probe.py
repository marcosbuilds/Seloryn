"""Black-box proof of the local job contract and validated lifecycle."""

from __future__ import annotations

import json
import re
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


def successful_json(result: subprocess.CompletedProcess[str]) -> dict:
    assert result.returncode == 0, (result.returncode, result.stdout, result.stderr)
    assert result.stderr == "", result.stderr
    return json.loads(result.stdout)


def failed_code(
    result: subprocess.CompletedProcess[str],
    code: str,
    expected_returncode: int = 65,
) -> None:
    assert result.returncode == expected_returncode, (
        result.returncode,
        result.stdout,
        result.stderr,
    )
    assert result.stdout == "", result.stdout
    payload = json.loads(result.stderr)
    assert payload["schema"] == "seloryn.error.v1", payload
    assert payload["code"] == code, payload


def main() -> int:
    assert BINARY.is_file(), f"missing release binary: {BINARY}"
    with tempfile.TemporaryDirectory(prefix="seloryn-job-lifecycle-") as temporary:
        root = Path(temporary)
        spec = root / "spec.json"
        spec.write_text(
            '{ "seed": 7, "kind": "train", "inputs": ["data/train.json"] }\n',
            encoding="utf-8",
        )

        created = successful_json(run(root, "create", str(spec), "--id=alpha"))
        assert created["schema"] == "seloryn.job.v1", created
        assert created["operation"] == "create", created
        assert created["job_id"] == "alpha", created
        assert created["status"] == "accepted", created
        assert created["sequence"] == 0, created
        assert created["reused"] is False, created
        assert str(root) not in json.dumps(created), created

        missing_spec = root / "private" / "secret.json"
        missing_result = run(root, "create", str(missing_spec), "--id=missing")
        failed_code(missing_result, "SEL-J001", 74)
        assert str(missing_spec) not in missing_result.stderr, missing_result.stderr

        job_dir = root / ".seloryn" / "jobs" / "alpha"
        assert sorted(path.name for path in job_dir.iterdir()) == [
            "events.jsonl",
            "job.lock",
            "spec.json",
            "state.json",
        ]
        assert json.loads((job_dir / "spec.json").read_text(encoding="utf-8")) == {
            "inputs": ["data/train.json"],
            "kind": "train",
            "seed": 7,
        }
        assert len((job_dir / "events.jsonl").read_text(encoding="utf-8").splitlines()) == 1

        # The lock itself is part of the trusted storage boundary. If the
        # final component is replaced with a symlink/reparse point, status
        # must reject it before opening or locking the target.
        lock_path = job_dir / "job.lock"
        lock_target = job_dir / "job-lock-target"
        lock_path.rename(lock_target)
        try:
            lock_path.symlink_to(lock_target.name)
        except (OSError, NotImplementedError):
            lock_target.rename(lock_path)
        else:
            try:
                failed_code(run(root, "status", "alpha"), "SEL-J012")
            finally:
                if lock_path.is_symlink():
                    lock_path.unlink()
                if lock_target.exists():
                    lock_target.rename(lock_path)

        reused = successful_json(run(root, "create", str(spec), "--id=alpha"))
        assert reused["reused"] is True, reused
        assert reused["sequence"] == 0, reused

        different_spec = root / "different.json"
        different_spec.write_text('{"kind":"evaluate"}\n', encoding="utf-8")
        failed_code(run(root, "create", str(different_spec), "--id=alpha"), "SEL-J004")

        transitioned = successful_json(run(root, "transition", "alpha", "queued"))
        assert transitioned["status"] == "queued", transitioned
        current = successful_json(run(root, "status", "alpha"))
        assert current["status"] == "queued", current
        assert current["sequence"] == 1, current
        assert current["attempt"] == 1, current

        failed_code(run(root, "transition", "alpha", "starting"), "SEL-J018")
        failed_code(run(root, "transition", "alpha", "succeeded"), "SEL-J018")
        failed_code(run(root, "transition", "alpha", "not-a-state"), "SEL-J006")
        failed_code(run(root, "status", "../escape"), "SEL-J003")
        assert len((job_dir / "events.jsonl").read_text(encoding="utf-8").splitlines()) == 2

        listed = successful_json(run(root, "list"))
        assert listed["schema"] == "seloryn.job-list.v1", listed
        assert [item["job_id"] for item in listed["jobs"]] == ["alpha"], listed
        assert listed["jobs"][0]["status"] == "queued", listed

        generated = successful_json(run(root, "create", str(spec)))
        assert re.fullmatch(r"job-[a-f0-9]{24}", generated["job_id"]), generated
        assert generated["status"] == "accepted", generated
        assert successful_json(run(root, "create", str(spec), "--id=alpha"))["reused"] is True

        for path in root.rglob("*"):
            assert ".tmp-" not in path.name, path
    print("Seloryn job lifecycle verification: atomic files, idempotent re-execution, validated transitions and safe IDs passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
