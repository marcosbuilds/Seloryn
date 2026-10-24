"""Contract tests for the optional local token-counting adapter."""

from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
MODULE_PATH = ROOT / "research" / "agent-token-economy" / "token_count.py"
SPEC = importlib.util.spec_from_file_location("seloryn_token_count", MODULE_PATH)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class TokenCountContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        if importlib.util.find_spec("tiktoken") is None:
            raise unittest.SkipTest("optional tiktoken dependency is not installed")

    def test_bpe_count_is_distinct_from_character_count(self) -> None:
        result = MODULE.count_text("hello world", encoding_name="o200k_base")
        self.assertEqual(result["schema"], "seloryn.openai-token-count.v1")
        self.assertTrue(result["not_provider_billing"])
        self.assertEqual(result["units"]["unicode_codepoints"], 11)
        self.assertEqual(result["units"]["utf8_bytes"], 11)
        self.assertEqual(result["units"]["tokens"], 2)
        self.assertEqual(result["tokens"][0]["byte_start"], 0)
        self.assertEqual(result["tokens"][-1]["byte_end"], 11)

    def test_unicode_is_counted_as_bytes_and_bpe_tokens_separately(self) -> None:
        result = MODULE.count_text("Olá, Seloryn 🤖", encoding_name="o200k_base")
        self.assertGreater(result["units"]["utf8_bytes"], result["units"]["unicode_codepoints"])
        self.assertGreater(result["units"]["tokens"], 0)
        self.assertEqual(result["tokens"][-1]["byte_end"], result["units"]["utf8_bytes"])

    def test_documentation_validation_strings_have_reproducible_counts(self) -> None:
        expected = {
            "hello world": (11, 11, 2),
            "caractere por caractere": (23, 23, 6),
            "Tensor[F32, shape=[2, 2]]": (25, 25, 11),
        }
        for text, units in expected.items():
            result = MODULE.count_text(text, model="gpt-4o")
            self.assertEqual(
                (
                    result["units"]["unicode_codepoints"],
                    result["units"]["utf8_bytes"],
                    result["units"]["tokens"],
                ),
                units,
                text,
            )


if __name__ == "__main__":
    unittest.main()
