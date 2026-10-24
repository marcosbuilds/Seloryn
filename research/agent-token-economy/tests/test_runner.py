#!/usr/bin/env python3
"""Black-box tests for the frozen suite runner."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SUITE_ROOT = Path(__file__).resolve().parents[1]
REPO_ROOT = SUITE_ROOT.parents[1]
RUNNER = SUITE_ROOT / "runner.py"
RECORDS = SUITE_ROOT / "fixtures" / "replay" / "provider-records.jsonl"
WORKSPACES = SUITE_ROOT / "fixtures" / "replay" / "workspaces"


def run_cli(*arguments: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(RUNNER), *arguments],
        cwd=REPO_ROOT,
        text=True,
        encoding="utf-8",
        errors="replace",
        capture_output=True,
        check=False,
    )


class RunnerTests(unittest.TestCase):
    def test_manifest_validation(self) -> None:
        result = run_cli("validate")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("10 frozen tasks", result.stdout)

    def test_dry_run_has_no_provider_observations(self) -> None:
        with tempfile.TemporaryDirectory(prefix="agent-token-economy-dry-run-") as temporary:
            output = Path(temporary) / "dry-run.json"
            result = run_cli("dry-run", "--output", str(output))
            self.assertEqual(result.returncode, 0, result.stderr)
            report = json.loads(output.read_text(encoding="utf-8"))
        self.assertEqual(report["mode"], "dry-run")
        self.assertTrue(report["not_an_evaluation"])
        self.assertEqual(len(report["results"]), 10)
        self.assertTrue(all(item["acceptance"]["status"] == "not-run" for item in report["results"]))
        self.assertTrue(all(item["provider_metrics"]["tokens"]["total"] is None for item in report["results"]))
        self.assertEqual(report["hypotheses"]["A"]["conclusion"], "inconclusive")
        self.assertEqual(report["hypotheses"]["B"]["conclusion"], "inconclusive")

    def test_synthetic_replay_runs_all_independent_acceptance_adapters(self) -> None:
        with tempfile.TemporaryDirectory(prefix="agent-token-economy-replay-") as temporary:
            output = Path(temporary) / "replay.json"
            result = run_cli("replay", "--records", str(RECORDS), "--workspace", str(WORKSPACES), "--output", str(output))
            self.assertEqual(result.returncode, 0, result.stderr)
            report = json.loads(output.read_text(encoding="utf-8"))
        self.assertEqual(report["evidence_class"], "synthetic-replay")
        self.assertTrue(report["not_an_evaluation"])
        self.assertEqual(report["provider_record_count"], 10)
        self.assertEqual([item["acceptance"]["status"] for item in report["results"]], ["passed"] * 10)
        self.assertTrue(all(item["provider_metrics"]["success"] is None for item in report["results"]))

    def test_missing_total_is_not_derived(self) -> None:
        record = {
            "schema": "seloryn.agent-token-economy.provider-record",
            "schema_version": 1,
            "run_id": "test-partial-metrics",
            "task_id": "A-01",
            "condition": "candidate",
            "evidence_class": "provider-export",
            "provider": {"name": "test-provider"},
            "metrics": {
                "tokens": {"input": 11, "output": 7},
                "attempts": 1,
                "success": True,
                "files_changed": 2,
            },
        }
        with tempfile.TemporaryDirectory(prefix="agent-token-economy-partial-") as temporary:
            root = Path(temporary)
            records = root / "records.jsonl"
            records.write_text(json.dumps(record) + "\n", encoding="utf-8")
            output = root / "report.json"
            result = run_cli("replay", "--records", str(records), "--workspace", str(WORKSPACES), "--output", str(output))
            self.assertEqual(result.returncode, 0, result.stderr)
            report = json.loads(output.read_text(encoding="utf-8"))
        first = report["results"][0]
        self.assertEqual(first["condition"], "candidate")
        self.assertEqual(first["provider_metrics"]["tokens"]["input"], 11)
        self.assertEqual(first["provider_metrics"]["tokens"]["output"], 7)
        self.assertIsNone(first["provider_metrics"]["tokens"]["total"])
        self.assertTrue(first["provider_metrics"]["success"])
        self.assertEqual(first["provider_metrics"]["attempts"], 1)
        self.assertEqual(first["provider_metrics"]["files_changed"], 2)
        self.assertIsNone(first["provider_metrics"]["lines_added"])


if __name__ == "__main__":
    unittest.main()
