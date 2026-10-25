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

        invalid_source = "fn main() -> I64:\n  return 7\n"
        invalid_bytes = invalid_source.encode("utf-8")
        (root / "invalid.sely").write_bytes(invalid_bytes)
        invalid_spec = root / "invalid-spec.json"
        invalid_spec.write_text(
            json.dumps(
                {
                    "kind": "seloryn-run.v1",
                    "source": "invalid.sely",
                    "source_sha256": hashlib.sha256(invalid_bytes).hexdigest(),
                    "timeout_ms": 2_000,
                },
                sort_keys=True,
            )
            + "\n",
            encoding="utf-8",
        )
        created_invalid = ok(
            run(root, "create", str(invalid_spec), "--id=preflight-failure")
        )
        assert created_invalid["status"] == "accepted", created_invalid
        failed = run(root, "run", "preflight-failure", "--owner=probe")
        assert failed.returncode == 65, (failed.returncode, failed.stdout, failed.stderr)
        assert failed.stdout == "", failed.stdout
        error = json.loads(failed.stderr)
        assert error["code"] == "SEL-J021", error

        failed_status = ok(run(root, "status", "preflight-failure"))
        assert failed_status["status"] == "accepted", failed_status
        assert "lease_id" not in failed_status, failed_status
        failure_receipt_path = (
            root / ".seloryn" / "jobs" / "preflight-failure" / "result.json"
        )
        failure_receipt = json.loads(failure_receipt_path.read_text(encoding="utf-8"))
        assert failure_receipt["schema"] == "seloryn.job-result.v1", failure_receipt
        assert failure_receipt["phase"] == "preflight", failure_receipt
        assert failure_receipt["status"] == "preflight_failed", failure_receipt
        assert failure_receipt["committed"] is True, failure_receipt
        assert failure_receipt["error_code"] == "SEL-J021", failure_receipt
        assert len(failure_receipt["error_sha256"]) == 64, failure_receipt
        assert "message" not in failure_receipt, failure_receipt
        assert str(root) not in failure_receipt_path.read_text(encoding="utf-8")

        retry_source = "fn main() -> I64:\n    return 1 / 0\n"
        retry_bytes = retry_source.encode("utf-8")
        (root / "retryable.sely").write_bytes(retry_bytes)
        retry_spec = root / "retry-spec.json"
        retry_spec.write_text(
            json.dumps(
                {
                    "kind": "seloryn-run.v1",
                    "source": "retryable.sely",
                    "source_sha256": hashlib.sha256(retry_bytes).hexdigest(),
                    "timeout_ms": 2_000,
                },
                sort_keys=True,
            )
            + "\n",
            encoding="utf-8",
        )
        ok(run(root, "create", str(retry_spec), "--id=retryable"))
        first_failure = ok(run(root, "run", "retryable", "--owner=probe"))
        assert first_failure["status"] == "failed", first_failure
        assert first_failure["attempt"] == 1, first_failure
        first_receipt = (
            root / ".seloryn" / "jobs" / "retryable" / "result.json"
        ).read_bytes()
        retried = ok(run(root, "retry", "retryable"))
        assert retried["operation"] == "retry", retried
        assert retried["status"] == "queued", retried
        assert retried["attempt"] == 1, retried
        assert not (
            root / ".seloryn" / "jobs" / "retryable" / "result.json"
        ).exists()
        archived = (
            root / ".seloryn" / "jobs" / "retryable" / "attempts" / "attempt-1.json"
        )
        assert archived.read_bytes() == first_receipt

        second_failure = ok(run(root, "run", "retryable", "--owner=probe"))
        assert second_failure["status"] == "failed", second_failure
        assert second_failure["attempt"] == 2, second_failure
        second_receipt = (
            root / ".seloryn" / "jobs" / "retryable" / "result.json"
        ).read_bytes()
        repeated_retry = ok(run(root, "retry", "retryable"))
        assert repeated_retry["status"] == "queued", repeated_retry
        assert repeated_retry["attempt"] == 2, repeated_retry
        assert (
            root / ".seloryn" / "jobs" / "retryable" / "attempts" / "attempt-2.json"
        ).read_bytes() == second_receipt

        rejected_retry = run(root, "retry", "worker")
        assert rejected_retry.returncode == 65, rejected_retry
        assert json.loads(rejected_retry.stderr)["code"] == "SEL-J015"

    print("Seloryn job worker verification: preflight receipts, terminal retry, archived attempts, lease fencing, bounded execution and terminal state passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
