#!/usr/bin/env python3
"""Run the deterministic Python condition of TOKEN-PAIR-01.

The condition contract is intentionally language-neutral.  A manifest must
have this shape::

    {
      "schema": "seloryn.token-pair-task.v1",
      "schema_version": 1,
      "run_id": "TOKEN-PAIR-01",
      "task_id": "visible-text-metrics",
      "prompt": "...",
      "inputs": [{"id": "sample-1", "text": "..."}],
      "environment": {"network": "disabled"}
    }

The command emits ``seloryn.agent-visible-transcript.v1`` with the
deterministic task result embedded alongside the visible user and assistant
messages.  That makes the output directly consumable by the existing local
transcript counter while keeping token counts separate from execution.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from typing import Any


MANIFEST_SCHEMA = "seloryn.token-pair-task.v1"
RESULT_SCHEMA = "seloryn.agent-visible-transcript.v1"
SCHEMA_VERSION = 1
CONDITION = "python"


def _canonical(value: Any) -> str:
    """Render JSON without incidental whitespace or key-order variation."""

    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


def _require_text(value: Any, field: str) -> str:
    if not isinstance(value, str) or not value:
        raise ValueError(f"{field} must be a non-empty string")
    return value


def load_manifest(path: Path) -> dict[str, Any]:
    """Load and validate the shared language-neutral task manifest."""

    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ValueError(f"could not read manifest: {error}") from error

    if not isinstance(document, dict):
        raise ValueError("manifest must be a JSON object")
    if document.get("schema") != MANIFEST_SCHEMA:
        raise ValueError(f"manifest schema must be {MANIFEST_SCHEMA}")
    if document.get("schema_version") != SCHEMA_VERSION:
        raise ValueError(f"manifest schema_version must be {SCHEMA_VERSION}")
    for field in ("run_id", "task_id", "prompt"):
        _require_text(document.get(field), field)

    inputs = document.get("inputs")
    if not isinstance(inputs, list) or not inputs:
        raise ValueError("manifest inputs must be a non-empty array")

    seen_ids: set[str] = set()
    for index, item in enumerate(inputs):
        if not isinstance(item, dict):
            raise ValueError(f"inputs[{index}] must be an object")
        item_id = _require_text(item.get("id"), f"inputs[{index}].id")
        _require_text(item.get("text"), f"inputs[{index}].text")
        if item_id in seen_ids:
            raise ValueError(f"duplicate input id: {item_id}")
        seen_ids.add(item_id)

    environment = document.get("environment", {})
    if not isinstance(environment, dict):
        raise ValueError("manifest environment must be an object when present")

    return document


def _visible_input(manifest: dict[str, Any]) -> str:
    """Build the exact visible input shared by both language conditions."""

    task_input = {
        "environment": manifest.get("environment", {}),
        "inputs": manifest["inputs"],
    }
    return f'{manifest["prompt"]}\n\nInput JSON:\n{_canonical(task_input)}'


def _task_result(manifest: dict[str, Any]) -> dict[str, Any]:
    """Calculate stable, non-token metrics for each manifest input."""

    rows: list[dict[str, Any]] = []
    total_codepoints = 0
    total_bytes = 0
    for item in manifest["inputs"]:
        text = item["text"]
        utf8_bytes = len(text.encode("utf-8"))
        codepoints = len(text)
        total_codepoints += codepoints
        total_bytes += utf8_bytes
        rows.append(
            {
                "id": item["id"],
                "unicode_codepoints": codepoints,
                "utf8_bytes": utf8_bytes,
                "text_sha256": hashlib.sha256(text.encode("utf-8")).hexdigest(),
            }
        )
    return {
        "condition": CONDITION,
        "task_id": manifest["task_id"],
        "items": rows,
        "totals": {
            "items": len(rows),
            "unicode_codepoints": total_codepoints,
            "utf8_bytes": total_bytes,
        },
    }


def run(manifest: dict[str, Any]) -> dict[str, Any]:
    """Execute the Python condition and return a transcript-shaped record."""

    # Re-run validation for callers that provide an in-memory manifest.
    if manifest.get("schema") != MANIFEST_SCHEMA or manifest.get("schema_version") != SCHEMA_VERSION:
        raise ValueError("manifest schema or schema_version is invalid")
    load_inputs = manifest.get("inputs")
    if not isinstance(load_inputs, list) or not load_inputs:
        raise ValueError("manifest inputs must be a non-empty array")
    for index, item in enumerate(load_inputs):
        if not isinstance(item, dict):
            raise ValueError(f"inputs[{index}] must be an object")
        _require_text(item.get("id"), f"inputs[{index}].id")
        _require_text(item.get("text"), f"inputs[{index}].text")
    _require_text(manifest.get("run_id"), "run_id")
    _require_text(manifest.get("task_id"), "task_id")
    _require_text(manifest.get("prompt"), "prompt")

    result = _task_result(manifest)
    visible_input = _visible_input(manifest)
    visible_output = _canonical(result)
    manifest_sha256 = hashlib.sha256(_canonical(manifest).encode("utf-8")).hexdigest()
    return {
        "schema": RESULT_SCHEMA,
        "run_id": manifest["run_id"],
        "task_id": manifest["task_id"],
        "condition": CONDITION,
        "capture_scope": "manifest_prompt_and_deterministic_task_result_only",
        "manifest_sha256": manifest_sha256,
        "messages": [
            {"role": "user", "text": visible_input},
            {"role": "assistant", "text": visible_output},
        ],
        "result": result,
        "visible_input": visible_input,
        "visible_output": visible_output,
        "not_provider_billing": True,
        "limitations": [
            "The task result reports codepoints, UTF-8 bytes and hashes; it does not calculate model tokens.",
            "The transcript contains only the declared visible prompt and deterministic result, not hidden context or tool serialization.",
            "Local transcript counts are not provider billing or usage telemetry.",
        ],
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args(argv)

    try:
        result = run(load_manifest(args.manifest))
    except (OSError, ValueError) as error:
        print(f"python TOKEN-PAIR-01 condition: {error}")
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
