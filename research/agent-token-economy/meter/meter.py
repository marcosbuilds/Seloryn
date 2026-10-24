#!/usr/bin/env python3
"""Measure visible prompt/response text without pretending to measure billing.

The meter accepts a JSON-like record with ``prompt``, ``response``,
``environment`` and optional provider ``usage``.  It produces a deterministic
JSON-safe report.  Local token values are a tiktoken reconstruction of text
fields only; provider-reported usage is kept as a separate source of truth.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.metadata
import json
import re
import sys
from collections.abc import Mapping, Sequence
from pathlib import Path
from typing import Any


SCHEMA = "seloryn.agent-meter"
SCHEMA_VERSION = 1
_MISSING = object()
_SENSITIVE = re.compile(
    r"(?:token|secret|password|passwd|api[_-]?key|authorization|credential|private|prompt|response|content|argument|payload|data|file)",
    re.IGNORECASE,
)
_TOOL_KEYS = {"tool", "tools", "tool_call", "tool_calls", "function", "functions", "arguments"}
_FILE_KEYS = {"file", "files", "image", "images", "input_image", "file_id", "file_data", "file_url"}
_REASONING_KEYS = {"reasoning", "reasoning_tokens", "hidden", "analysis"}
_SAFE_ENV_KEYS = {
    "architecture",
    "condition",
    "encoding",
    "language",
    "language_version",
    "model",
    "network",
    "platform",
    "provider",
    "runtime",
    "seed",
    "version",
}


class MeterInputError(ValueError):
    """The input record is not a supported JSON-like measurement record."""


def _sha256(value: str | bytes) -> str:
    raw = value.encode("utf-8") if isinstance(value, str) else value
    return hashlib.sha256(raw).hexdigest()


def _json_bytes(value: Any) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode("utf-8")


def _text_from(value: Any, flags: set[str]) -> tuple[str, bool]:
    """Extract visible text and record structured content outside local scope."""

    if isinstance(value, str):
        return value, True
    if value is None:
        flags.add("null_or_missing_text")
        return "", False
    if isinstance(value, Sequence) and not isinstance(value, (str, bytes, bytearray)):
        parts: list[str] = []
        present = False
        for item in value:
            text, item_present = _text_from(item, flags)
            if item_present:
                present = True
                parts.append(text)
        return "\n".join(parts), present
    if isinstance(value, Mapping):
        lowered = {str(key).lower() for key in value}
        if lowered & _TOOL_KEYS:
            flags.add("tool_serialization_not_counted")
        content_type = str(value.get("type", "")).lower()
        has_file_shape = (
            bool(lowered & _FILE_KEYS)
            or any(key.startswith("image_") or key.startswith("file_") for key in lowered)
            or content_type in {"input_image", "output_image", "input_file", "output_file"}
        )
        if has_file_shape:
            flags.add("files_or_images_not_counted")
        if lowered & _REASONING_KEYS:
            flags.add("reasoning_or_hidden_content_not_counted")

        # These are the text-bearing fields supported by the local scope.
        for key in ("content", "text", "input_text", "output_text", "input", "instructions", "messages"):
            if key in value:
                return _text_from(value[key], flags)
        flags.add("unrecognized_structured_content")
        return "", False
    flags.add("non_text_content_not_counted")
    return "", False


def _tokenizer(model: str | None, encoding_name: str | None) -> tuple[Any | None, dict[str, Any]]:
    info: dict[str, Any] = {
        "library": "tiktoken",
        "model": model,
        "requested_encoding": encoding_name,
        "available": False,
        "resolved_encoding": None,
        "version": None,
    }
    if model and encoding_name:
        info["error"] = "model_and_encoding_both_supplied"
        return None, info
    try:
        import tiktoken
    except ImportError:
        info["error"] = "dependency_missing"
        return None, info

    try:
        if model:
            encoding = tiktoken.encoding_for_model(model)
        else:
            encoding = tiktoken.get_encoding(encoding_name or "o200k_base")
    except KeyError:
        info["error"] = "unknown_model"
        return None, info
    except Exception as error:  # pragma: no cover - depends on optional host setup
        info["error"] = f"tokenizer_error:{type(error).__name__}"
        return None, info

    info["available"] = True
    info["resolved_encoding"] = encoding.name
    try:
        info["version"] = importlib.metadata.version("tiktoken")
    except importlib.metadata.PackageNotFoundError:  # pragma: no cover - unusual import setup
        info["version"] = None
    return encoding, info


def _local_stats(value: Any, encoding: Any | None) -> tuple[dict[str, Any], set[str]]:
    flags: set[str] = set()
    if value is _MISSING:
        return {
            "present": False,
            "text_present": False,
            "text_sha256": None,
            "unicode_codepoints": None,
            "utf8_bytes": None,
            "tokens": None,
            "scope": "visible_text_fields_only",
        }, flags

    text, present = _text_from(value, flags)
    if not present:
        return {
            "present": True,
            "text_present": False,
            "text_sha256": None,
            "unicode_codepoints": None,
            "utf8_bytes": None,
            "tokens": None,
            "scope": "visible_text_fields_only",
        }, flags

    utf8 = text.encode("utf-8")
    tokens: int | None = None
    if encoding is not None:
        try:
            tokens = len(encoding.encode(text, disallowed_special=()))
        except Exception:  # pragma: no cover - tokenizer-specific failure
            flags.add("local_tokenization_failed")
    return {
        "present": True,
        "text_present": True,
        "text_sha256": _sha256(utf8),
        "unicode_codepoints": len(text),
        "utf8_bytes": len(utf8),
        "tokens": tokens,
        "scope": "visible_text_fields_only",
    }, flags


def _safe_environment(environment: Any) -> tuple[dict[str, Any], set[str]]:
    flags: set[str] = set()
    if environment is _MISSING or environment is None:
        return {}, flags
    if not isinstance(environment, Mapping):
        flags.add("environment_not_mapping")
        return {"value_type": type(environment).__name__}, flags

    def clean(value: Any, key: str) -> Any:
        normalized_key = key.lower()
        if _SENSITIVE.search(key):
            flags.add("environment_sensitive_value_redacted")
            return "<redacted>"
        if normalized_key not in _SAFE_ENV_KEYS:
            flags.add("environment_key_not_allowlisted")
            return "<redacted>"
        if value is None or isinstance(value, (bool, int, float)):
            return value
        if isinstance(value, str):
            if len(value) <= 256:
                return value
            flags.add("environment_long_value_hashed")
            return {"sha256": _sha256(value), "length": len(value), "redacted": True}
        flags.add("environment_value_redacted")
        return {"type": type(value).__name__, "redacted": True}

    return {str(k): clean(v, str(k)) for k, v in sorted(environment.items(), key=lambda item: str(item[0]))}, flags


def _integer(value: Any) -> int | None:
    if isinstance(value, bool) or not isinstance(value, int) or value < 0:
        return None
    return value


def _first_integer(mapping: Mapping[str, Any], names: Sequence[str]) -> tuple[int | None, str | None]:
    for name in names:
        if name in mapping:
            return _integer(mapping[name]), name
    return None, None


def _provider_usage(record: Mapping[str, Any]) -> tuple[dict[str, Any], set[str]]:
    usage = record.get("provider_usage", record.get("usage", _MISSING))
    if not isinstance(usage, Mapping):
        return {
            "present": False,
            "source": "missing",
            "input_tokens": None,
            "output_tokens": None,
            "total_tokens": None,
            "reasoning_tokens": None,
            "cached_input_tokens": None,
            "reported_fields": [],
        }, {"provider_usage_missing"}

    input_tokens, input_name = _first_integer(usage, ("input_tokens", "prompt_tokens"))
    output_tokens, output_name = _first_integer(usage, ("output_tokens", "completion_tokens"))
    total_tokens, total_name = _first_integer(usage, ("total_tokens",))
    output_details = usage.get("output_tokens_details")
    input_details = usage.get("input_tokens_details")
    reasoning_tokens, reasoning_name = (None, None)
    cached_tokens, cached_name = (None, None)
    if isinstance(output_details, Mapping):
        reasoning_tokens, reasoning_name = _first_integer(output_details, ("reasoning_tokens",))
    if isinstance(input_details, Mapping):
        cached_tokens, cached_name = _first_integer(input_details, ("cached_tokens",))

    names = [name for name in (input_name, output_name, total_name, reasoning_name, cached_name) if name]
    missing = {"provider_usage_partial"} if any(value is None for value in (input_tokens, output_tokens, total_tokens)) else set()
    return {
        "present": True,
        "source": "provider_reported",
        "input_tokens": input_tokens,
        "output_tokens": output_tokens,
        "total_tokens": total_tokens,
        "reasoning_tokens": reasoning_tokens,
        "cached_input_tokens": cached_tokens,
        "reported_fields": sorted(names),
    }, missing


def measure(record: Mapping[str, Any]) -> dict[str, Any]:
    """Return a deterministic, redacted measurement for one model run."""

    if not isinstance(record, Mapping):
        raise MeterInputError("measurement record must be a JSON object")
    environment, environment_flags = _safe_environment(record.get("environment", _MISSING))
    model = record.get("model") or (environment.get("model") if isinstance(environment.get("model"), str) else None)
    requested_encoding = record.get("encoding") or (
        environment.get("encoding") if isinstance(environment.get("encoding"), str) else None
    )
    encoding, tokenizer = _tokenizer(model, requested_encoding)
    prompt_stats, prompt_flags = _local_stats(record.get("prompt", _MISSING), encoding)
    response_stats, response_flags = _local_stats(record.get("response", _MISSING), encoding)
    provider_usage, provider_flags = _provider_usage(record)

    reasons = {
        "message_wrappers_not_counted",
        "reasoning_or_hidden_tokens_not_visible",
        *environment_flags,
        *prompt_flags,
        *response_flags,
        *provider_flags,
    }
    if not tokenizer["available"]:
        reasons.add(f"local_tokenizer_{tokenizer.get('error', 'unavailable')}")
    if prompt_stats["tokens"] is None or response_stats["tokens"] is None:
        reasons.add("local_token_count_unknown")

    local_scope_complete = (
        tokenizer["available"]
        and prompt_stats["text_present"]
        and response_stats["text_present"]
        and not (prompt_flags | response_flags)
    )
    output = {
        "schema": SCHEMA,
        "schema_version": SCHEMA_VERSION,
        "record_id": record.get("record_id") if isinstance(record.get("record_id"), str) else None,
        "status": "measured",
        "local_visible": True,
        "not_provider_billing": True,
        "environment": environment,
        "tokenizer": tokenizer,
        "local_visible_usage": {
            "input": prompt_stats,
            "output": response_stats,
            "scope": "visible_text_fields_only",
        },
        "provider_usage": provider_usage,
        "uncertainty": {
            "status": "complete" if local_scope_complete and provider_usage["present"] and not provider_flags else "partial",
            "local_visible_text": "complete" if local_scope_complete else "partial",
            "provider_billing": (
                "reported"
                if provider_usage["present"] and not provider_flags
                else "partial"
                if provider_usage["present"]
                else "unknown"
            ),
            "reasons": sorted(reasons),
        },
        "privacy": {
            "raw_content": "redacted_by_default",
            "content_identity": "sha256_only",
            "environment": "sanitized",
        },
    }
    return json.loads(json.dumps(output, ensure_ascii=False, sort_keys=True, separators=(",", ":")))


def render(result: Mapping[str, Any]) -> str:
    """Render a result with stable key ordering and a final newline."""

    return json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True) + "\n"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", nargs="?", type=Path, help="JSON record; stdin when omitted")
    parser.add_argument("--output", type=Path, help="write the redacted report to this path")
    args = parser.parse_args(argv)
    try:
        raw = args.input.read_text(encoding="utf-8") if args.input else sys.stdin.read()
        record = json.loads(raw)
        result = measure(record)
    except (OSError, json.JSONDecodeError, MeterInputError) as error:
        print(f"agent meter: {error}", file=sys.stderr)
        return 2
    rendered = render(result)
    if args.output:
        args.output.write_text(rendered, encoding="utf-8")
    else:
        print(rendered, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
