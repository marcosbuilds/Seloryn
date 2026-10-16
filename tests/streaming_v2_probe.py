"""Independent black-box proof for the versioned, hash-chained training stream."""

from __future__ import annotations

import importlib.util
import copy
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target" / "release" / ("seloryn.exe" if sys.platform == "win32" else "seloryn")
VERIFIER = ROOT / "scripts" / "streaming" / "verify-event-chain.py"
REPLAYER = ROOT / "scripts" / "streaming" / "replay-receipt.py"


def load_verifier():
    spec = importlib.util.spec_from_file_location("seloryn_event_chain", VERIFIER)
    if spec is None or spec.loader is None:
        raise AssertionError(f"cannot load verifier: {VERIFIER}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def run_stream(project: Path, log_format: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [str(BINARY), "run", str(project), f"--log={log_format}"],
        cwd=ROOT,
        text=True,
        encoding="utf-8",
        errors="replace",
        capture_output=True,
        check=False,
    )


def assert_rejected(path: Path, expected: str) -> None:
    result = subprocess.run(
        [sys.executable, str(VERIFIER), str(path)],
        cwd=ROOT,
        text=True,
        encoding="utf-8",
        capture_output=True,
        check=False,
    )
    assert result.returncode != 0, path.name
    assert expected in result.stdout or expected in result.stderr, result


def main() -> int:
    assert BINARY.is_file(), BINARY
    verifier = load_verifier()

    with tempfile.TemporaryDirectory(prefix="seloryn-stream-v2-") as directory:
        root = Path(directory)
        project_one = root / "json-v1"
        project_two = root / "json-v2"
        shutil.copytree(ROOT / "examples" / "projects" / "mlp-training", project_one)
        shutil.copytree(ROOT / "examples" / "projects" / "mlp-training", project_two)
        for project in (project_one, project_two):
            checkpoint = project / "checkpoints" / "model.json"
            if checkpoint.exists():
                checkpoint.unlink()

        version_one = run_stream(project_one, "json")
        assert version_one.returncode == 0, version_one.stderr
        version_two = run_stream(project_two, "json2")
        assert version_two.returncode == 0, version_two.stderr
        assert version_one.stdout == version_two.stdout

        lines = [line for line in version_two.stderr.splitlines() if line.strip()]
        assert lines, "json2 emitted no events"
        events = [json.loads(line) for line in lines]
        assert events[0]["kind"] == "run.start"
        assert events[1]["kind"] == "training.start"
        assert any(event["kind"] == "training.step" for event in events)
        assert any(event["kind"] == "training.complete" for event in events)
        assert events[-1]["kind"] == "run.end"
        assert events[-1]["data"]["status"] == "ok"
        assert events[-1]["data"]["events_before_end"] == len(events) - 1
        assert events[-1]["data"]["last_event_hash"] == events[-2]["hash"]
        assert [event["seq"] for event in events] == list(range(len(events)))

        valid, message, count = verifier.verify(lines)
        assert valid, message
        assert count == len(events)

        linear_project = root / "linear"
        shutil.copytree(ROOT / "examples" / "projects" / "linear-training", linear_project)
        linear_v1 = run_stream(linear_project, "json")
        assert linear_v1.returncode == 0, linear_v1.stderr
        linear_checkpoint = linear_project / "data" / "checkpoint.json"
        if linear_checkpoint.exists():
            linear_checkpoint.unlink()
        linear_v2 = run_stream(linear_project, "json2")
        assert linear_v2.returncode == 0, linear_v2.stderr
        assert linear_v1.stdout == linear_v2.stdout
        linear_lines = [line for line in linear_v2.stderr.splitlines() if line.strip()]
        linear_valid, linear_message, _ = verifier.verify(linear_lines)
        assert linear_valid, linear_message
        assert [json.loads(line)["kind"] for line in linear_lines][0] == "run.start"
        assert [json.loads(line)["kind"] for line in linear_lines][-1] == "run.end"

        valid_path = root / "valid.ndjson"
        valid_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
        subprocess.run(
            [sys.executable, str(VERIFIER), str(valid_path)],
            cwd=ROOT,
            check=True,
            text=True,
        )
        replay = subprocess.run(
            [
                sys.executable,
                str(REPLAYER),
                "--receipt",
                str(valid_path),
                "--project",
                str(project_two),
                "--binary",
                str(BINARY),
            ],
            cwd=ROOT,
            check=False,
            text=True,
            capture_output=True,
        )
        assert replay.returncode == 0, replay.stderr

        signed_project = root / "signed"
        shutil.copytree(ROOT / "examples" / "projects" / "mlp-training", signed_project)
        signed_checkpoint = signed_project / "checkpoints" / "model.json"
        if signed_checkpoint.exists():
            signed_checkpoint.unlink()
        signed_environment = dict(os.environ)
        signed_environment["SELORYN_RECEIPT_KEY"] = "streaming-probe-secret"
        signed_process = subprocess.run(
            [str(BINARY), "run", str(signed_project), "--log=json2-signed"],
            cwd=ROOT,
            env=signed_environment,
            text=True,
            encoding="utf-8",
            errors="replace",
            capture_output=True,
            check=False,
        )
        assert signed_process.returncode == 0, signed_process.stderr
        signed_lines = [line for line in signed_process.stderr.splitlines() if line.strip()]
        signed_valid, signed_message, _ = verifier.verify(
            signed_lines, b"streaming-probe-secret"
        )
        assert signed_valid, signed_message
        assert all("signature" in json.loads(line) for line in signed_lines)
        signed_path = root / "signed.ndjson"
        signed_path.write_text("\n".join(signed_lines) + "\n", encoding="utf-8")
        signed_replay = subprocess.run(
            [
                sys.executable,
                str(REPLAYER),
                "--receipt",
                str(signed_path),
                "--project",
                str(signed_project),
                "--binary",
                str(BINARY),
                "--hmac-key",
                "streaming-probe-secret",
            ],
            cwd=ROOT,
            check=False,
            text=True,
            capture_output=True,
        )
        assert signed_replay.returncode == 0, signed_replay.stderr

        altered = copy.deepcopy(events)
        altered[2]["data"]["step"] = altered[2]["data"].get("step", 0) + 1
        altered_path = root / "altered.ndjson"
        altered_path.write_text(
            "\n".join(json.dumps(event, ensure_ascii=False, separators=(",", ":")) for event in altered)
            + "\n",
            encoding="utf-8",
        )
        assert_rejected(altered_path, "hash mismatch")

        removed_path = root / "removed.ndjson"
        removed_path.write_text("\n".join(lines[:2] + lines[3:]) + "\n", encoding="utf-8")
        assert_rejected(removed_path, "seq")

        reordered_path = root / "reordered.ndjson"
        reordered = list(lines)
        reordered[2], reordered[3] = reordered[3], reordered[2]
        reordered_path.write_text("\n".join(reordered) + "\n", encoding="utf-8")
        assert_rejected(reordered_path, "seq")

        truncated_path = root / "truncated.ndjson"
        truncated_path.write_text("\n".join(lines[:-1]) + "\n", encoding="utf-8")
        assert_rejected(truncated_path, "run.end")

        duplicate_key_path = root / "duplicate-key.ndjson"
        duplicate_key_path.write_text(
            lines[0].replace('"schema":2', '"schema":2,"schema":2', 1) + "\n",
            encoding="utf-8",
        )
        assert_rejected(duplicate_key_path, "duplicate key")

        limited_project = root / "limited"
        shutil.copytree(ROOT / "examples" / "projects" / "mlp-training", limited_project)
        checkpoint = limited_project / "checkpoints" / "model.json"
        if checkpoint.exists():
            checkpoint.unlink()
        manifest = limited_project / "seloryn.toml"
        manifest.write_text(
            manifest.read_text(encoding="utf-8").replace(
                "json_max_bytes = 1048576", "json_max_bytes = 1"
            ),
            encoding="utf-8",
        )
        limited = run_stream(limited_project, "json2")
        assert limited.returncode == 65, limited
        assert "exceeds maximum 1 bytes" in limited.stderr, limited.stderr
        assert '"kind":"training.start"' not in limited.stderr
        assert not checkpoint.exists()

        broken_checkpoint_project = root / "broken-checkpoint"
        shutil.copytree(ROOT / "examples" / "projects" / "mlp-training", broken_checkpoint_project)
        broken_checkpoint = broken_checkpoint_project / "checkpoints" / "model.json"
        if broken_checkpoint.exists():
            broken_checkpoint.unlink()
        broken_checkpoint.mkdir()
        broken = run_stream(broken_checkpoint_project, "json2")
        assert broken.returncode == 65, broken
        broken_lines = [line for line in broken.stderr.splitlines() if line.strip()]
        broken_events = [json.loads(line) for line in broken_lines]
        assert broken_events[-1]["kind"] == "run.end"
        assert broken_events[-1]["data"]["status"] == "error"
        valid, message, _ = verifier.verify(broken_lines)
        assert valid, message
        assert not list((broken_checkpoint_project / "checkpoints").glob(".model.json.tmp-*"))

        stdout_failure_project = root / "stdout-failure"
        shutil.copytree(ROOT / "examples" / "projects" / "mlp-training", stdout_failure_project)
        stdout_failure_checkpoint = stdout_failure_project / "checkpoints" / "model.json"
        if stdout_failure_checkpoint.exists():
            stdout_failure_checkpoint.unlink()
        process = subprocess.Popen(
            [str(BINARY), "run", str(stdout_failure_project), "--log=json2"],
            cwd=ROOT,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
        assert process.stdout is not None
        process.stdout.close()
        assert process.stderr is not None
        stdout_failure_stderr = process.stderr.read()
        stdout_failure_status = process.wait()
        assert stdout_failure_status == 74, stdout_failure_status
        stdout_failure_lines = [
            line for line in stdout_failure_stderr.splitlines() if line.strip()
        ]
        valid, message, _ = verifier.verify(stdout_failure_lines)
        assert valid, message
        stdout_failure_end = json.loads(stdout_failure_lines[-1])
        assert stdout_failure_end["data"]["status"] == "error"
        assert stdout_failure_end["data"]["error_code"] == "SEL-C002"
        assert "SEL-C002 failed to write stdout" not in stdout_failure_stderr

    print("Seloryn streaming v2 verification: lifecycle, chain and tamper probes passed")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (AssertionError, OSError, json.JSONDecodeError) as error:
        print(f"streaming v2 verification FAILED: {error!r}", file=sys.stderr)
        raise SystemExit(1) from error
