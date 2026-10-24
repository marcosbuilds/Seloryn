"""Black-box proof of the local bearer lease gate without a worker."""

from __future__ import annotations

import concurrent.futures
import json
import subprocess
import sys
import tempfile
import time
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


def error(result: subprocess.CompletedProcess[str], *codes: str) -> dict:
    assert result.returncode == 65, (result.returncode, result.stdout, result.stderr)
    assert result.stdout == "", result.stdout
    payload = json.loads(result.stderr)
    assert payload["code"] in codes, payload
    return payload


def main() -> int:
    assert BINARY.is_file(), f"missing release binary: {BINARY}"
    with tempfile.TemporaryDirectory(prefix="seloryn-job-lease-") as temporary:
        root = Path(temporary)
        spec = root / "spec.json"
        spec.write_text('{"kind":"lease-test","seed":23}\n', encoding="utf-8")
        ok(run(root, "create", str(spec), "--id=lease"))

        def concurrent_claim(index: int) -> subprocess.CompletedProcess[str]:
            return run(
                root,
                "claim",
                "lease",
                f"--owner=worker-{index}",
                "--ttl-seconds=30",
            )

        with concurrent.futures.ThreadPoolExecutor(max_workers=32) as pool:
            claims = list(pool.map(concurrent_claim, range(100)))
        successes = [result for result in claims if result.returncode == 0]
        failures = [result for result in claims if result.returncode != 0]
        assert len(successes) == 1, [(result.returncode, result.stderr) for result in claims]
        assert len(failures) == 99, len(failures)
        assert all(error(result, "SEL-J014", "SEL-J011") for result in failures)

        claimed = ok(successes[0])
        assert claimed["status"] == "starting", claimed
        lease_id = claimed["lease_id"]
        attempt_id = claimed["attempt_id"]
        assert claimed["attempt"] == 1, claimed
        assert claimed["owner"].startswith("worker-"), claimed
        assert claimed["lease_expires_at"] > claimed["claimed_at"], claimed

        job_dir = root / ".seloryn" / "jobs" / "lease"
        events_path = job_dir / "events.jsonl"
        assert len(events_path.read_text(encoding="utf-8").splitlines()) == 2

        retried = ok(
            run(
                root,
                "claim",
                "lease",
                f"--lease-id={lease_id}",
                f"--owner={claimed['owner']}",
                "--ttl-seconds=30",
            )
        )
        assert retried["idempotent"] is True, retried
        assert retried["attempt"] == 1, retried
        assert retried["lease_id"] == lease_id, retried
        assert len(events_path.read_text(encoding="utf-8").splitlines()) == 2
        error(run(root, "claim", "lease", "--lease-id=lease-new"), "SEL-J014")
        error(run(root, "claim", "lease", f"--lease-id={lease_id}", "--owner="), "SEL-J014")
        error(
            run(root, "claim", "lease", f"--lease-id={lease_id}", "--owner=other-worker"),
            "SEL-J017",
        )
        assert len(events_path.read_text(encoding="utf-8").splitlines()) == 2

        error(run(root, "heartbeat", "lease", "--lease-id=lease-invalid"), "SEL-J017")
        assert len(events_path.read_text(encoding="utf-8").splitlines()) == 2

        renewed = ok(
            run(
                root,
                "heartbeat",
                "lease",
                f"--lease-id={lease_id}",
                "--ttl-seconds=30",
            )
        )
        assert renewed["status"] == "starting", renewed
        assert renewed["lease_id"] == lease_id, renewed
        assert renewed["lease_expires_at"] > claimed["lease_expires_at"], renewed

        for forged in ("running", "committing", "succeeded", "failed", "lost", "retry_wait"):
            error(run(root, "transition", "lease", forged), "SEL-J018")
        current = ok(run(root, "status", "lease"))
        assert current["status"] == "starting", current
        assert current["lease_id"] == lease_id, current
        assert len(events_path.read_text(encoding="utf-8").splitlines()) == 3

        released = ok(run(root, "release", "lease", f"--lease-id={lease_id}"))
        assert released["status"] == "queued", released
        assert "lease_id" not in released, released
        assert "owner" not in released, released

        reclaimed = ok(
            run(root, "claim", "lease", "--owner=worker-reclaim", "--ttl-seconds=1")
        )
        assert reclaimed["status"] == "starting", reclaimed
        assert reclaimed["attempt"] == 2, reclaimed
        assert reclaimed["lease_id"] != lease_id, reclaimed
        assert reclaimed["attempt_id"] != attempt_id, reclaimed
        second_lease = reclaimed["lease_id"]

        error(run(root, "recover", "lease"), "SEL-J016")
        before_expiry = ok(run(root, "status", "lease"))
        assert before_expiry["status"] == "starting", before_expiry

        recovery_deadline = time.monotonic() + 5.0
        while True:
            recovery_attempt = run(root, "recover", "lease")
            if recovery_attempt.returncode == 0:
                recovered = ok(recovery_attempt)
                break
            error(recovery_attempt, "SEL-J016")
            if time.monotonic() >= recovery_deadline:
                raise AssertionError("lease did not expire within the bounded recovery window")
            time.sleep(0.05)
        assert recovered["status"] == "lost", recovered
        assert "lease_id" not in recovered, recovered
        assert recovered["owner"] == "worker-reclaim", recovered
        assert recovered["attempt_id"] == reclaimed["attempt_id"], recovered

        recovered_again = ok(run(root, "recover", "lease"))
        assert recovered_again["status"] == "lost", recovered_again
        assert recovered_again["reused"] is True, recovered_again
        error(run(root, "claim", "lease", "--owner=worker-after-lost"), "SEL-J015")
        error(run(root, "transition", "lease", "succeeded"), "SEL-J018")
        final = ok(run(root, "status", "lease"))
        assert final["status"] == "lost", final
        assert final["attempt"] == 2, final

        events = [json.loads(line) for line in events_path.read_text(encoding="utf-8").splitlines()]
        assert [event["sequence"] for event in events] == list(range(len(events))), events
        assert [event["to"] for event in events] == [
            "accepted",
            "starting",
            "starting",
            "queued",
            "starting",
            "lost",
        ], events
        assert [event["kind"] for event in events] == [
            "create",
            "claim",
            "heartbeat",
            "release",
            "claim",
            "recover",
        ], events
        assert [event["attempt"] for event in events] == [1, 1, 1, 1, 2, 2], events
        assert all(event["to"] != "succeeded" for event in events), events

        tampered_spec = root / "tampered-spec.json"
        tampered_spec.write_text('{"kind":"tampered"}\n', encoding="utf-8")
        ok(run(root, "create", str(tampered_spec), "--id=tampered"))
        ok(run(root, "transition", "tampered", "queued"))
        tampered_events_path = root / ".seloryn" / "jobs" / "tampered" / "events.jsonl"
        tampered_events = [
            json.loads(line) for line in tampered_events_path.read_text(encoding="utf-8").splitlines()
        ]
        tampered_events[1]["kind"] = "admin"
        tampered_events[1]["to"] = "succeeded"
        tampered_events_path.write_text(
            "".join(json.dumps(event, separators=(",", ":")) + "\n" for event in tampered_events),
            encoding="utf-8",
        )
        error(run(root, "status", "tampered"), "SEL-J007")

        legacy_spec = root / "legacy-spec.json"
        legacy_spec.write_text('{"kind":"legacy"}\n', encoding="utf-8")
        ok(run(root, "create", str(legacy_spec), "--id=legacy"))
        ok(run(root, "transition", "legacy", "queued"))
        legacy_events_path = root / ".seloryn" / "jobs" / "legacy" / "events.jsonl"
        legacy_events = [
            json.loads(line) for line in legacy_events_path.read_text(encoding="utf-8").splitlines()
        ]
        for event in legacy_events:
            event.pop("kind", None)
            event.pop("attempt", None)
        legacy_events_path.write_text(
            "".join(json.dumps(event, separators=(",", ":")) + "\n" for event in legacy_events),
            encoding="utf-8",
        )
        legacy = ok(run(root, "status", "legacy"))
        assert legacy["status"] == "queued", legacy
        assert legacy["attempt"] == 1, legacy

    print("Seloryn lease verification: 100-way claim, bearer heartbeat, release/reclaim, expiry recovery, bypass rejection and ordered history passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
