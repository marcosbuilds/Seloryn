"""Local contract tests for the sanitized CI evidence helper."""

from __future__ import annotations

import json
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from scripts.ci import ci_evidence  # noqa: E402


def main() -> int:
    workspace_marker = str(ROOT)
    with tempfile.TemporaryDirectory(prefix="seloryn-ci-evidence-test-") as temporary:
        output_dir = Path(temporary)
        command = [
            sys.executable,
            "-c",
            "print('{\"binary\":\"D:/private/runner/seloryn.exe\",\"ok\":true}')",
        ]
        assert ci_evidence.run_probe(output_dir, "helper-command", "test", command) == 0
        record = json.loads((output_dir / "helper-command.json").read_text(encoding="utf-8"))
        assert record["status"] == "passed", record
        assert record["summary"]["value"]["binary"] == "seloryn.exe", record
        assert workspace_marker not in json.dumps(record), record
        assert "private" not in json.dumps(record).lower(), record
        assert ci_evidence.sanitize_value({"token": "do-not-persist"})["token"] == "<redacted>"

        failed_command = [sys.executable, "-c", "raise SystemExit(7)"]
        assert ci_evidence.run_probe(output_dir, "failed-command", "test", failed_command) == 7
        failed = json.loads((output_dir / "failed-command.json").read_text(encoding="utf-8"))
        assert failed["status"] == "failed", failed
        assert failed["exit_code"] == 7, failed

        text_summary = ci_evidence.summarize_output(
            "Seloryn job lifecycle verification: passed\n",
            "",
        )
        assert text_summary["format"] == "text", text_summary
        assert "job lifecycle" in text_summary["stdout_tail"], text_summary

        assert ci_evidence.finalize(
            output_dir, ("helper-command", "failed-command", "missing-command")
        ) == 1
        manifest = json.loads((output_dir / "manifest.json").read_text(encoding="utf-8"))
        assert manifest["status"] == "incomplete", manifest
        assert manifest["missing"] == ["missing-command"], manifest
        assert manifest["failed"] == ["failed-command"], manifest
        assert manifest["records"][2]["status"] == "not-run", manifest
        assert manifest["retention"]["raw_stdout_stderr_persisted"] is False, manifest

    print("Seloryn CI evidence helper verification: sanitized records, portable paths and incomplete manifests passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
