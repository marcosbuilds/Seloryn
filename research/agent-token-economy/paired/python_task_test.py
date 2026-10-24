"""Focused contract tests for the Python TOKEN-PAIR-01 condition."""

from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("python_task.py")
SPEC = importlib.util.spec_from_file_location("python_task", SCRIPT)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def manifest() -> dict[str, object]:
    return {
        "schema": "seloryn.token-pair-task.v1",
        "schema_version": 1,
        "run_id": "TOKEN-PAIR-01",
        "task_id": "visible-text-metrics",
        "prompt": "Compute visible Unicode codepoint and UTF-8 byte lengths for each sample.",
        "inputs": [
            {"id": "ascii", "text": "hello world"},
            {"id": "portuguese", "text": "caractere por caractere"},
            {"id": "tensor", "text": "Tensor[F32, shape=[2, 2]]"},
        ],
        "environment": {"network": "disabled", "seed": 0},
    }


class PythonTaskTests(unittest.TestCase):
    def test_run_is_transcript_shaped_and_deterministic(self) -> None:
        first = MODULE.run(manifest())
        second = MODULE.run(manifest())

        self.assertEqual(first, second)
        self.assertEqual(first["schema"], "seloryn.agent-visible-transcript.v1")
        self.assertEqual(first["condition"], "python")
        self.assertTrue(first["not_provider_billing"])
        self.assertEqual([message["role"] for message in first["messages"]], ["user", "assistant"])
        self.assertEqual(first["visible_input"], first["messages"][0]["text"])
        self.assertEqual(first["visible_output"], first["messages"][1]["text"])

        result = first["result"]
        self.assertEqual(result["totals"], {"items": 3, "unicode_codepoints": 59, "utf8_bytes": 59})
        self.assertEqual(result["items"][0]["unicode_codepoints"], 11)
        self.assertEqual(result["items"][1]["utf8_bytes"], 23)
        self.assertEqual(result["items"][2]["unicode_codepoints"], 25)

    def test_manifest_validation_rejects_duplicate_ids(self) -> None:
        invalid = manifest()
        invalid["inputs"] = [
            {"id": "same", "text": "one"},
            {"id": "same", "text": "two"},
        ]
        manifest_path = self._write_manifest(invalid)
        try:
            with self.assertRaisesRegex(ValueError, "duplicate input id"):
                MODULE.load_manifest(manifest_path)
        finally:
            manifest_path.unlink()

    def test_cli_writes_json_accepted_by_transcript_counter_contract(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest_path = root / "manifest.json"
            output_path = root / "result.json"
            manifest_path.write_text(json.dumps(manifest(), ensure_ascii=False), encoding="utf-8")
            completed = subprocess.run(
                [sys.executable, str(SCRIPT), "--manifest", str(manifest_path), "--output", str(output_path)],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr or completed.stdout)
            output = json.loads(output_path.read_text(encoding="utf-8"))
            self.assertEqual(output["schema"], "seloryn.agent-visible-transcript.v1")
            self.assertEqual(len(output["messages"]), 2)
            self.assertIn('"condition":"python"', output["visible_output"])

    @staticmethod
    def _write_manifest(value: dict[str, object]) -> Path:
        temporary = tempfile.NamedTemporaryFile("w", encoding="utf-8", suffix=".json", delete=False)
        with temporary:
            json.dump(value, temporary, ensure_ascii=False)
        path = Path(temporary.name)
        return path


if __name__ == "__main__":
    unittest.main()
