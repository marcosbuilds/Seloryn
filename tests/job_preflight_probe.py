"""Black-box proof of the read-only local job preflight gate."""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
import tempfile
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target" / "release" / ("seloryn.exe" if sys.platform == "win32" else "seloryn")
VALID_SOURCE = (ROOT / "examples" / "tensors" / "linear-forward.sely").read_bytes()


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


def successful(result: subprocess.CompletedProcess[str]) -> dict:
    assert result.returncode == 0, (result.returncode, result.stdout, result.stderr)
    assert result.stderr == "", result.stderr
    return json.loads(result.stdout)


def failed(result: subprocess.CompletedProcess[str], code: str = "SEL-J021") -> dict:
    assert result.returncode == 65, (result.returncode, result.stdout, result.stderr)
    assert result.stdout == "", result.stdout
    payload = json.loads(result.stderr)
    assert payload["schema"] == "seloryn.error.v1", payload
    assert payload["code"] == code, payload
    return payload


def source_hash(source: bytes) -> str:
    return hashlib.sha256(source).hexdigest()


def write_spec(root: Path, name: str, spec: dict) -> Path:
    path = root / f"{name}-input.json"
    path.write_text(json.dumps(spec, ensure_ascii=False) + "\n", encoding="utf-8")
    return path


def create_job(root: Path, name: str, spec: dict) -> None:
    successful(run(root, "create", str(write_spec(root, name, spec)), f"--id={name}"))


def valid_spec(source: str = "program.sely", source_bytes: bytes = VALID_SOURCE) -> dict:
    return {
        "kind": "seloryn-run.v1",
        "source": source,
        "source_sha256": source_hash(source_bytes),
        "timeout_ms": 30_000,
    }


def event_and_state(root: Path, job_id: str) -> tuple[bytes, bytes]:
    job = root / ".seloryn" / "jobs" / job_id
    return (
        (job / "events.jsonl").read_bytes(),
        (job / "state.json").read_bytes(),
    )


def filesystem_entries(root: Path) -> tuple[str, ...]:
    return tuple(sorted(path.relative_to(root).as_posix() for path in root.rglob("*")))


def assert_read_only(root: Path, job_id: str, expected_status: str) -> None:
    before = event_and_state(root, job_id)
    files_before = filesystem_entries(root)
    result = run(root, "preflight", job_id)
    failed(result)
    after = event_and_state(root, job_id)
    files_after = filesystem_entries(root)
    assert before == after, (job_id, expected_status)
    assert files_before == files_after, (job_id, files_before, files_after)


def main() -> int:
    assert BINARY.is_file(), f"missing release binary: {BINARY}"
    with tempfile.TemporaryDirectory(prefix="seloryn-job-preflight-") as temporary:
        root = Path(temporary)
        (root / "program.sely").write_bytes(VALID_SOURCE)

        create_job(root, "ready", valid_spec())
        before = event_and_state(root, "ready")
        files_before = filesystem_entries(root)
        result = successful(run(root, "preflight", "ready"))
        expected_spec = {
            "kind": "seloryn-run.v1",
            "source": "program.sely",
            "source_sha256": source_hash(VALID_SOURCE),
            "timeout_ms": 30_000,
        }
        expected_spec_hash = hashlib.sha256(
            (json.dumps(expected_spec, sort_keys=True, separators=(",", ":")) + "\n").encode()
        ).hexdigest()
        assert result == {
            "attempt": 1,
            "job_id": "ready",
            "operation": "preflight",
            "schema": "seloryn.job.v1",
            "source": "program.sely",
            "source_bytes": len(VALID_SOURCE),
            "source_sha256": source_hash(VALID_SOURCE),
            "spec_sha256": expected_spec_hash,
            "status": "ready",
            "timeout_ms": 30_000,
        }, result
        assert event_and_state(root, "ready") == before
        assert filesystem_entries(root) == files_before

        queued = successful(run(root, "transition", "ready", "queued"))
        assert queued["status"] == "queued", queued
        before = event_and_state(root, "ready")
        assert successful(run(root, "preflight", "ready"))["status"] == "ready"
        assert event_and_state(root, "ready") == before

        create_job(root, "digest", valid_spec())
        digest_spec = root / ".seloryn" / "jobs" / "digest" / "spec.json"
        changed_spec = valid_spec()
        changed_spec["timeout_ms"] = 31_000
        digest_spec.write_text(json.dumps(changed_spec) + "\n", encoding="utf-8")
        assert_read_only(root, "digest", "accepted")

        extra = valid_spec()
        extra["unexpected"] = True
        create_job(root, "extra", extra)
        assert_read_only(root, "extra", "accepted")

        invalid_specs = {
            "kind-value": {**valid_spec(), "kind": "other.v1"},
            "kind-type": {**valid_spec(), "kind": 7},
            "source-type": {**valid_spec(), "source": 7},
            "timeout-type": {**valid_spec(), "timeout_ms": "30000"},
            "timeout-zero": {**valid_spec(), "timeout_ms": 0},
            "timeout-large": {**valid_spec(), "timeout_ms": 3_600_001},
        }
        for name, invalid_spec in invalid_specs.items():
            create_job(root, name, invalid_spec)
            assert_read_only(root, name, "accepted")

        mismatch = valid_spec()
        mismatch["source_sha256"] = "0" * 64
        create_job(root, "hash", mismatch)
        assert_read_only(root, "hash", "accepted")

        escape = valid_spec("../outside.sely")
        create_job(root, "escape", escape)
        assert_read_only(root, "escape", "accepted")

        absolute = valid_spec(str((root / "program.sely").resolve()))
        create_job(root, "absolute", absolute)
        assert_read_only(root, "absolute", "accepted")

        invalid_paths = {
            "too-long": "a" * 1021 + ".sely",
            "component-long": "a" * 256 + ".sely",
            "forbidden": "bad?.sely",
            "trailing-dot": "bad./program.sely",
            "reserved": "CON.sely",
        }
        for name, invalid_path in invalid_paths.items():
            create_job(root, name, valid_spec(invalid_path))
            assert_read_only(root, name, "accepted")

        for name, source in (
            ("syntax", b"fn main(\n"),
            ("semantic", b"fn main() -> I64:\n    return missing_name\n"),
        ):
            (root / f"{name}.sely").write_bytes(source)
            create_job(root, name, valid_spec(f"{name}.sely", source))
            assert_read_only(root, name, "accepted")

        invalid_utf8 = b"fn main() -> I64:\n    return 1\n\xff"
        (root / "invalid.sely").write_bytes(invalid_utf8)
        create_job(root, "utf8", valid_spec("invalid.sely", invalid_utf8))
        assert_read_only(root, "utf8", "accepted")

        oversized = b"a" * (16 * 1024 * 1024 + 1)
        (root / "oversized.sely").write_bytes(oversized)
        create_job(root, "oversized", valid_spec("oversized.sely", oversized))
        assert_read_only(root, "oversized", "accepted")

        symlink = root / "link.sely"
        try:
            symlink.symlink_to(root / "program.sely")
        except (OSError, NotImplementedError):
            symlink = None
        if symlink is not None:
            create_job(root, "symlink", valid_spec("link.sely"))
            assert_read_only(root, "symlink", "accepted")

        nested = root / "nested"
        nested.mkdir()
        (nested / "program.sely").write_bytes(VALID_SOURCE)
        intermediate = root / "nested-link"
        try:
            intermediate.symlink_to(nested, target_is_directory=True)
        except (OSError, NotImplementedError):
            intermediate = None
        if intermediate is not None:
            create_job(root, "intermediate", valid_spec("nested-link/program.sely"))
            assert_read_only(root, "intermediate", "accepted")

        create_job(root, "starting", valid_spec())
        successful(run(root, "claim", "starting", "--owner=probe", "--ttl-seconds=5"))
        assert_read_only(root, "starting", "starting")

        create_job(root, "lost", valid_spec())
        successful(run(root, "claim", "lost", "--owner=probe", "--ttl-seconds=1"))
        time.sleep(1.2)
        successful(run(root, "recover", "lost"))
        assert_read_only(root, "lost", "lost")

    print("Seloryn job preflight verification: strict spec, bounded source validation, diagnostics, path safety and read-only state passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
