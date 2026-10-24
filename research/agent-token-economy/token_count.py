#!/usr/bin/env python3
"""Count visible text with an OpenAI-compatible tiktoken BPE encoding.

This is a local measurement adapter, not an API billing oracle. It reports
Unicode code points, UTF-8 bytes and BPE tokens separately. A character count
is never silently presented as a token count.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.metadata
import json
import sys
from pathlib import Path
from typing import Any


SCHEMA = "seloryn.openai-token-count.v1"


class TokenizerUnavailable(RuntimeError):
    """The optional tiktoken dependency is not available."""


def load_encoding(*, encoding_name: str | None, model: str | None) -> tuple[Any, str, str | None]:
    try:
        import tiktoken
    except ImportError as error:  # pragma: no cover - depends on the host
        raise TokenizerUnavailable(
            "tiktoken is not installed; install the optional tokenizer requirements "
            "and rerun. Character and byte counts are not token counts."
        ) from error

    if model and encoding_name:
        raise ValueError("choose --model or --encoding, not both")
    if model:
        try:
            encoding = tiktoken.encoding_for_model(model)
        except KeyError as error:
            raise ValueError(f"tiktoken does not know model {model!r}; choose an explicit --encoding") from error
    else:
        encoding = tiktoken.get_encoding(encoding_name or "o200k_base")
    return encoding, encoding.name, model


def count_text(text: str, *, encoding_name: str | None = None, model: str | None = None) -> dict[str, Any]:
    encoding, resolved_encoding, resolved_model = load_encoding(encoding_name=encoding_name, model=model)
    token_ids = encoding.encode(text, disallowed_special=())
    token_items: list[dict[str, Any]] = []
    byte_offset = 0
    for index, token_id in enumerate(token_ids):
        token_bytes = encoding.decode_single_token_bytes(token_id)
        token_items.append(
            {
                "index": index,
                "id": token_id,
                "byte_start": byte_offset,
                "byte_end": byte_offset + len(token_bytes),
                "bytes_hex": token_bytes.hex(),
                "text_approximation": token_bytes.decode("utf-8", errors="replace"),
            }
        )
        byte_offset += len(token_bytes)

    utf8 = text.encode("utf-8")
    if byte_offset != len(utf8):
        raise ValueError("token bytes do not reconstruct the input UTF-8 bytes")
    codepoints = len(text)
    token_count = len(token_ids)
    return {
        "schema": SCHEMA,
        "status": "measured",
        "source": "local_tiktoken_reconstruction",
        "not_provider_billing": True,
        "tokenizer": {
            "library": "tiktoken",
            "version": importlib.metadata.version("tiktoken"),
            "encoding": resolved_encoding,
            "model": resolved_model,
        },
        "text_sha256": hashlib.sha256(utf8).hexdigest(),
        "units": {
            "unicode_codepoints": codepoints,
            "utf8_bytes": len(utf8),
            "tokens": token_count,
            "tokens_per_unicode_codepoint": token_count / codepoints if codepoints else None,
            "utf8_bytes_per_token": len(utf8) / token_count if token_count else None,
        },
        "tokens": token_items,
    }


def read_text(args: argparse.Namespace) -> str:
    supplied = sum(
        (
            args.text is not None,
            args.file is not None,
            bool(args.stdin),
        )
    )
    if supplied != 1:
        raise ValueError("choose exactly one of --text, --file or --stdin")
    if args.text is not None:
        return args.text
    if args.file is not None:
        return args.file.read_text(encoding="utf-8")
    return sys.stdin.read()


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--text")
    source.add_argument("--file", type=Path)
    source.add_argument("--stdin", action="store_true")
    parser.add_argument("--encoding", help="tiktoken encoding, for example o200k_base or cl100k_base")
    parser.add_argument("--model", help="resolve the model's current tiktoken encoding")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args(argv)
    try:
        result = count_text(read_text(args), encoding_name=args.encoding, model=args.model)
    except (OSError, TokenizerUnavailable, ValueError) as error:
        print(f"token counter: {error}", file=sys.stderr)
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
