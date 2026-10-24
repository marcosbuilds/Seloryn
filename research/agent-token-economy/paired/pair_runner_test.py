"""Contract tests for the shared-input TOKEN-PAIR-01 runner."""

from __future__ import annotations

import importlib.util
import json
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
SCRIPT = Path(__file__).with_name("run_pair.py")
SPEC = importlib.util.spec_from_file_location("pair_runner", SCRIPT)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class PairRunnerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.manifest = MODULE.load_manifest(Path(__file__).with_name("TOKEN-PAIR-01.manifest.json"))

    def test_visible_input_is_shared_and_seloryn_transcript_is_explicitly_bounded(self) -> None:
        document = MODULE.seloryn_transcript(self.manifest, '{"value":59}')
        self.assertEqual(document["messages"][0]["text"], MODULE.visible_input(self.manifest))
        self.assertEqual(document["messages"][1]["text"], '{"value":59}')
        self.assertEqual(document["condition"], "seloryn")
        self.assertTrue(document["not_provider_billing"])
        self.assertIn("does not yet implement OpenAI-compatible BPE", " ".join(document["limitations"]))

    def test_manifest_is_stable_json_and_declares_same_inputs(self) -> None:
        self.assertEqual(self.manifest["schema"], "seloryn.token-pair-task.v1")
        self.assertEqual([item["id"] for item in self.manifest["inputs"]], ["ascii", "portuguese", "tensor"])
        self.assertEqual(MODULE.manifest_sha256(self.manifest), MODULE.manifest_sha256(json.loads(json.dumps(self.manifest))))


if __name__ == "__main__":
    unittest.main()
