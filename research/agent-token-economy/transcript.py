#!/usr/bin/env python3
"""Reconstruct visible transcript token counts with a local tiktoken encoding."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from typing import Any

from token_count import count_text


SCHEMA = "seloryn.agent-visible-transcript-token-count.v1"
INPUT_ROLES = {"system", "user", "tool"}
OUTPUT_ROLES = {"assistant"}


def load_transcript(path: Path) -> dict[str, Any]:
    document = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(document, dict) or document.get("schema") != "seloryn.agent-visible-transcript.v1":
        raise ValueError("transcript schema must be seloryn.agent-visible-transcript.v1")
    if not isinstance(document.get("run_id"), str) or not document["run_id"]:
        raise ValueError("transcript run_id is required")
    if not isinstance(document.get("task_id"), str) or not document["task_id"]:
        raise ValueError("transcript task_id is required")
    messages = document.get("messages")
    if not isinstance(messages, list) or not messages:
        raise ValueError("transcript must contain at least one message")
    for message in messages:
        if not isinstance(message, dict) or message.get("role") not in INPUT_ROLES | OUTPUT_ROLES:
            raise ValueError("each message needs a supported role")
        if not isinstance(message.get("text"), str):
            raise ValueError("each message needs text")
    return document


def measure(document: dict[str, Any], *, encoding: str | None, model: str | None) -> dict[str, Any]:
    messages: list[dict[str, Any]] = []
    visible_input = 0
    visible_output = 0
    for index, message in enumerate(document["messages"]):
        measured = count_text(message["text"], encoding_name=encoding, model=model)
        tokens = measured["units"]["tokens"]
        if message["role"] in INPUT_ROLES:
            visible_input += tokens
        else:
            visible_output += tokens
        messages.append(
            {
                "index": index,
                "role": message["role"],
                "text_sha256": hashlib.sha256(message["text"].encode("utf-8")).hexdigest(),
                "unicode_codepoints": measured["units"]["unicode_codepoints"],
                "utf8_bytes": measured["units"]["utf8_bytes"],
                "tokens": tokens,
            }
        )
    return {
        "schema": SCHEMA,
        "status": "measured",
        "evidence_class": "visible-transcript-only",
        "not_provider_billing": True,
        "run_id": document["run_id"],
        "task_id": document["task_id"],
        "condition": document.get("condition"),
        "tokenizer": {
            "encoding": messages and count_text(document["messages"][0]["text"], encoding_name=encoding, model=model)["tokenizer"]["encoding"],
            "model": model,
        },
        "visible_text_tokens": {
            "input_roles": visible_input,
            "assistant_roles": visible_output,
            "all_visible_messages": visible_input + visible_output,
        },
        "messages": messages,
        "limitations": [
            "This counts visible text only; chat wrappers, tool serialization, hidden context and provider-specific accounting are absent.",
            "The counts are a local reconstruction and must not be used as provider billing values.",
            "A real comparison still requires paired transcripts and provider telemetry for all attempts.",
        ],
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--encoding", help="tiktoken encoding; defaults to o200k_base when --model is absent")
    parser.add_argument("--model")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args(argv)
    try:
        result = measure(load_transcript(args.input), encoding=args.encoding, model=args.model)
    except (OSError, ValueError, RuntimeError) as error:
        print(f"transcript token counter: {error}")
        return 2
    rendered = json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
    if args.output:
        args.output.write_text(rendered, encoding="utf-8")
        print(f"wrote {args.output}")
    else:
        print(rendered, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
