"""Black-box proof of recovery and refusal on interrupted local job writes."""

from __future__ import annotations

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


def expect_ok(result: subprocess.CompletedProcess[str]) -> dict:
    assert result.returncode == 0, (result.returncode, result.stdout, result.stderr)
    return json.loads(result.stdout)


def expect_error(
    result: subprocess.CompletedProcess[str],
    *codes: str,
    returncode: int = 65,
) -> None:
    assert result.returncode == returncode, (result.returncode, result.stdout, result.stderr)
    assert result.stdout == "", result.stdout
    assert json.loads(result.stderr)["code"] in codes, result.stderr


def lock_state_for_replace(path: Path):
    if sys.platform != "win32":
        return None
    import ctypes

    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel32.CreateFileW.argtypes = [
        ctypes.c_wchar_p,
        ctypes.c_uint32,
        ctypes.c_uint32,
        ctypes.c_void_p,
        ctypes.c_uint32,
        ctypes.c_uint32,
        ctypes.c_void_p,
    ]
    kernel32.CreateFileW.restype = ctypes.c_void_p
    kernel32.CloseHandle.argtypes = [ctypes.c_void_p]
    handle = kernel32.CreateFileW(
        str(path),
        0x80000000,
        0x00000001 | 0x00000002,
        None,
        3,
        0x00000080,
        None,
    )
    invalid = ctypes.c_void_p(-1).value
    assert handle not in (None, invalid), ctypes.get_last_error()
    return kernel32, handle


def main() -> int:
    assert BINARY.is_file(), f"missing release binary: {BINARY}"
    with tempfile.TemporaryDirectory(prefix="seloryn-job-recovery-") as temporary:
        root = Path(temporary)
        spec = root / "spec.json"
        spec.write_text('{"kind":"train","seed":11}\n', encoding="utf-8")

        expect_ok(run(root, "create", str(spec), "--id=repairable"))
        job_dir = root / ".seloryn" / "jobs" / "repairable"
        state_path = job_dir / "state.json"
        old_state = state_path.read_bytes()
        expect_ok(run(root, "transition", "repairable", "queued"))
        queued_state = state_path.read_bytes()
        assert json.loads(queued_state)["status"] == "queued"

        state_path.write_bytes(old_state)
        repaired = expect_ok(run(root, "status", "repairable"))
        assert repaired["status"] == "queued", repaired
        assert json.loads(state_path.read_text(encoding="utf-8"))["status"] == "queued"

        events_path = job_dir / "events.jsonl"
        complete_events = events_path.read_bytes()
        events_path.write_bytes(complete_events + b'{"schema":"broken"')
        expect_error(run(root, "status", "repairable"), "SEL-J008", "SEL-J007")
        assert state_path.read_bytes() == queued_state

        events_path.write_bytes(complete_events)
        state_path.write_bytes(b'{"schema":')
        expect_error(run(root, "status", "repairable"), "SEL-J007")
        assert not list(job_dir.glob("*.tmp-*")), list(job_dir.iterdir())

        expect_ok(run(root, "create", str(spec), "--id=write-failure"))
        write_failure_dir = root / ".seloryn" / "jobs" / "write-failure"
        write_failure_state = write_failure_dir / "state.json"
        locked = lock_state_for_replace(write_failure_state)
        if locked is not None:
            kernel32, handle = locked
            try:
                failed = run(root, "transition", "write-failure", "queued")
                expect_error(failed, "SEL-J010", returncode=74)
                assert len(
                    (write_failure_dir / "events.jsonl")
                    .read_text(encoding="utf-8")
                    .splitlines()
                ) == 2
                assert json.loads(write_failure_state.read_text(encoding="utf-8"))[
                    "status"
                ] == "accepted"
            finally:
                assert kernel32.CloseHandle(handle)
            recovered = expect_ok(run(root, "status", "write-failure"))
            assert recovered["status"] == "queued", recovered
            assert json.loads(write_failure_state.read_text(encoding="utf-8"))[
                "status"
            ] == "queued"
            assert not list(write_failure_dir.glob("*.tmp-*")), list(write_failure_dir.iterdir())

        incomplete = root / ".seloryn" / "jobs" / "interrupted"
        incomplete.mkdir(parents=True)
        (incomplete / "spec.json").write_text('{"kind":"partial"}\n', encoding="utf-8")
        expect_error(run(root, "status", "interrupted"), "SEL-J007")
        assert not (incomplete / "state.json").exists()

        with tempfile.TemporaryDirectory(prefix="seloryn-job-external-") as external_dir:
            outside = Path(external_dir) / "escape"
            outside.mkdir()
            link = root / ".seloryn" / "jobs" / "escape"
            try:
                link.symlink_to(outside, target_is_directory=True)
            except (NotImplementedError, OSError):
                symlink_checked = False
            else:
                symlink_checked = True
                expect_error(run(root, "status", "escape"), "SEL-J012")
                evil_root = root / "evil-root"
                evil_root.mkdir()
                metadata_link = evil_root / ".seloryn"
                metadata_link.symlink_to(Path(external_dir) / "store", target_is_directory=True)
                rejected = run(evil_root, "create", str(spec), "--id=escape")
                expect_error(rejected, "SEL-J012")
                assert not (Path(external_dir) / "store" / "jobs").exists()
            if sys.platform != "win32":
                assert symlink_checked, "POSIX symlink containment test could not run"

    print("Seloryn job recovery verification: event-authoritative snapshot repair and interrupted-write refusal passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
