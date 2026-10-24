"""Contract tests for visible agent transcript token reconstruction."""

from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
for name in ("token_count", "transcript_token_count"):
    path = ROOT / "research" / "agent-token-economy" / f"{name}.py"
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
TOKEN_MODULE = sys.modules["transcript_token_count"]


class TranscriptTokenCountTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        if importlib.util.find_spec("tiktoken") is None:
            raise unittest.SkipTest("optional tiktoken dependency is not installed")

    def test_input_and_assistant_roles_are_separated(self) -> None:
        document = {
            "schema": "seloryn.agent-visible-transcript.v1",
            "run_id": "demo-1",
            "task_id": "A-01",
            "condition": "seloryn-language",
            "messages": [
                {"role": "user", "text": "Fix the typed result."},
                {"role": "assistant", "text": "I would inspect the diagnostic and add a regression test."},
            ],
        }
        result = TOKEN_MODULE.measure(document, encoding=None, model="gpt-4o")
        self.assertEqual(result["evidence_class"], "visible-transcript-only")
        self.assertTrue(result["not_provider_billing"])
        self.assertGreater(result["visible_text_tokens"]["input_roles"], 0)
        self.assertGreater(result["visible_text_tokens"]["assistant_roles"], 0)
        self.assertEqual(
            result["visible_text_tokens"]["all_visible_messages"],
            result["visible_text_tokens"]["input_roles"] + result["visible_text_tokens"]["assistant_roles"],
        )


if __name__ == "__main__":
    unittest.main()
