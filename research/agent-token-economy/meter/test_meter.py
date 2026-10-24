"""Unit tests for the provider-neutral, privacy-first agent meter."""

from __future__ import annotations

import json
import sys
import unittest
from pathlib import Path
from unittest.mock import patch


sys.path.insert(0, str(Path(__file__).resolve().parent))
from meter import measure, render  # noqa: E402


class MeterTests(unittest.TestCase):
    def test_redacts_prompt_and_response_while_preserving_local_units(self) -> None:
        record = {
            "record_id": "run-1",
            "prompt": "hello world",
            "response": "Olá, Seloryn 🤖",
            "environment": {"model": "gpt-4o", "seed": 0},
        }
        result = measure(record)
        rendered = render(result)
        self.assertTrue(result["local_visible"])
        self.assertTrue(result["not_provider_billing"])
        self.assertNotIn("hello world", rendered)
        self.assertNotIn("Olá, Seloryn", rendered)
        self.assertEqual(result["local_visible_usage"]["input"]["unicode_codepoints"], 11)
        self.assertGreater(result["local_visible_usage"]["output"]["utf8_bytes"], 0)
        self.assertEqual(result["schema_version"], 1)

    def test_provider_usage_is_separate_and_missing_total_is_not_inferred(self) -> None:
        result = measure(
            {
                "prompt": "p",
                "response": "r",
                "usage": {"input_tokens": 8, "output_tokens": 5},
            }
        )
        usage = result["provider_usage"]
        self.assertEqual(usage["source"], "provider_reported")
        self.assertEqual(usage["input_tokens"], 8)
        self.assertEqual(usage["output_tokens"], 5)
        self.assertIsNone(usage["total_tokens"])
        self.assertIn("provider_usage_partial", result["uncertainty"]["reasons"])
        self.assertNotEqual(result["local_visible_usage"]["input"]["tokens"], 8)

    def test_unicode_codepoints_bytes_and_tokens_are_distinct_units(self) -> None:
        result = measure({"prompt": "🤖", "response": "café"})
        prompt = result["local_visible_usage"]["input"]
        response = result["local_visible_usage"]["output"]
        self.assertEqual(prompt["unicode_codepoints"], 1)
        self.assertEqual(prompt["utf8_bytes"], 4)
        self.assertEqual(response["unicode_codepoints"], 4)
        self.assertEqual(response["utf8_bytes"], 5)
        self.assertIsInstance(prompt["text_sha256"], str)

    def test_tools_files_and_reasoning_are_explicitly_outside_visible_scope(self) -> None:
        result = measure(
            {
                "prompt": [
                    {"role": "user", "content": [{"type": "input_text", "text": "weather"}]},
                    {"type": "input_image", "image_url": "https://example.invalid/x"},
                    {"type": "function", "arguments": '{"city":"Paris"}'},
                ],
                "response": {"output_text": "It is sunny.", "reasoning": {"tokens": 4}, "tool_calls": []},
            }
        )
        reasons = result["uncertainty"]["reasons"]
        self.assertIn("tool_serialization_not_counted", reasons)
        self.assertIn("files_or_images_not_counted", reasons)
        self.assertIn("reasoning_or_hidden_content_not_counted", reasons)
        self.assertIn("reasoning_or_hidden_tokens_not_visible", reasons)

    def test_unknown_model_keeps_local_tokens_unknown(self) -> None:
        class UnknownModelTokenizer:
            @staticmethod
            def encoding_for_model(model: str) -> object:
                raise KeyError(model)

        with patch.dict(sys.modules, {"tiktoken": UnknownModelTokenizer()}):
            result = measure(
                {"model": "model-that-is-not-known-to-tiktoken", "prompt": "x", "response": "y"}
            )

        self.assertIsNone(result["local_visible_usage"]["input"]["tokens"])
        self.assertEqual(result["tokenizer"]["error"], "unknown_model")
        self.assertIn("local_tokenizer_unknown_model", result["uncertainty"]["reasons"])

    def test_missing_optional_tokenizer_keeps_local_tokens_unknown(self) -> None:
        with patch.dict(sys.modules, {"tiktoken": None}):
            result = measure({"model": "gpt-4o", "prompt": "x", "response": "y"})

        self.assertIsNone(result["local_visible_usage"]["input"]["tokens"])
        self.assertEqual(result["tokenizer"]["error"], "dependency_missing")
        self.assertIn("local_tokenizer_dependency_missing", result["uncertainty"]["reasons"])

    def test_direct_input_text_part_is_counted_as_text(self) -> None:
        result = measure({"prompt": {"input_text": "hello"}, "response": {"output_text": "ok"}})
        self.assertTrue(result["local_visible_usage"]["input"]["text_present"])
        self.assertEqual(result["local_visible_usage"]["input"]["unicode_codepoints"], 5)

    def test_sensitive_environment_values_are_redacted(self) -> None:
        result = measure(
            {
                "prompt": "x",
                "response": "y",
                "environment": {"model": "gpt-4o", "api_key": "super-secret", "seed": 0},
            }
        )
        self.assertEqual(result["environment"]["api_key"], "<redacted>")
        self.assertNotIn("super-secret", render(result))
        self.assertIn("environment_sensitive_value_redacted", result["uncertainty"]["reasons"])

    def test_unknown_environment_keys_are_redacted_by_default(self) -> None:
        result = measure(
            {"prompt": "x", "response": "y", "environment": {"misc": "credential-value"}}
        )
        self.assertEqual(result["environment"]["misc"], "<redacted>")
        self.assertNotIn("credential-value", render(result))
        self.assertIn("environment_key_not_allowlisted", result["uncertainty"]["reasons"])

    def test_json_is_deterministic(self) -> None:
        record = {"response": "ok", "prompt": "go", "environment": {"seed": 1, "model": "gpt-4o"}}
        first = render(measure(record))
        second = render(measure(record))
        self.assertEqual(first, second)
        self.assertEqual(json.loads(first), json.loads(second))


if __name__ == "__main__":
    unittest.main()
