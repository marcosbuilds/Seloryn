#!/usr/bin/env python3
"""Verify Seloryn streaming v2 NDJSON integrity and lifecycle."""

from __future__ import annotations

import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Iterable


ZERO_HASH = "sha256:" + "0" * 64
HASH_PATTERN = re.compile(r"^sha256:[0-9a-f]{64}$")
REQUIRED = ("schema", "run", "seq", "kind", "phase", "data", "prev", "hash")
RUN_PATTERN = re.compile(r"^r_[0-9a-f]{64}$")


class RawNumber(str):
    """JSON number whose source spelling must survive canonicalization."""


class DuplicateKeyError(ValueError):
    pass


def reject_duplicate_keys(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        if key in result:
            raise DuplicateKeyError(f"duplicate key {key!r}")
        result[key] = value
    return result


def _canonical(value: object) -> str:
    if isinstance(value, RawNumber):
        return value
    if value is None:
        return "null"
    if value is True:
        return "true"
    if value is False:
        return "false"
    if isinstance(value, str):
        return json.dumps(value, ensure_ascii=False, allow_nan=False, separators=(",", ":"))
    if isinstance(value, list):
        return "[" + ",".join(_canonical(item) for item in value) + "]"
    if isinstance(value, dict):
        items = sorted(value.items())
        return "{" + ",".join(
            _canonical(str(key)) + ":" + _canonical(item) for key, item in items
        ) + "}"
    raise TypeError(f"unsupported JSON value: {type(value).__name__}")


def canonical(value: object) -> bytes:
    return _canonical(value).encode("utf-8")


def digest(value: object) -> str:
    return "sha256:" + hashlib.sha256(canonical(value)).hexdigest()


def verify(lines: Iterable[str]) -> tuple[bool, str, int]:
    previous = ZERO_HASH
    run_id: str | None = None
    expected_sequence = 0
    count = 0
    first_kind: str | None = None
    last_kind: str | None = None

    for line_number, raw in enumerate(lines, 1):
        if not raw.strip():
            continue
        try:
            parsed = json.loads(
                raw,
                object_pairs_hook=reject_duplicate_keys,
                parse_int=RawNumber,
                parse_float=RawNumber,
            )
        except DuplicateKeyError as error:
            return False, f"line {line_number}: duplicate key ({error})", count
        except json.JSONDecodeError as error:
            return False, f"line {line_number}: invalid JSON ({error})", count
        if not isinstance(parsed, dict):
            return False, f"line {line_number}: event is not an object", count
        missing = [key for key in REQUIRED if key not in parsed]
        if missing:
            return False, f"line {line_number}: missing fields {missing}", count
        if not isinstance(parsed["schema"], RawNumber) or parsed["schema"] != "2":
            return False, f"line {line_number}: schema is not 2", count
        if not isinstance(parsed["run"], str) or not RUN_PATTERN.fullmatch(parsed["run"]):
            return False, f"line {line_number}: run identity is invalid", count
        if run_id is None:
            run_id = parsed["run"]
        if parsed["run"] != run_id:
            return False, f"line {line_number}: run changed", count
        if not isinstance(parsed["seq"], RawNumber) or parsed["seq"] != str(expected_sequence):
            return (
                False,
                f"line {line_number}: seq {parsed['seq']} (expected {expected_sequence})",
                count,
            )
        if not isinstance(parsed["kind"], str) or not parsed["kind"]:
            return False, f"line {line_number}: kind is invalid", count
        if parsed["phase"] != "run" or not isinstance(parsed["data"], dict):
            return False, f"line {line_number}: phase or data is invalid", count
        if parsed["prev"] != previous or not HASH_PATTERN.fullmatch(parsed["prev"]):
            return False, f"line {line_number}: prev does not chain", count
        claimed = parsed["hash"]
        if not isinstance(claimed, str) or not HASH_PATTERN.fullmatch(claimed):
            return False, f"line {line_number}: hash format is invalid", count
        content = dict(parsed)
        del content["hash"]
        actual = digest(content)
        if actual != claimed:
            return False, f"line {line_number}: hash mismatch", count
        if last_kind == "run.end":
            return False, f"line {line_number}: event follows run.end", count
        if parsed["kind"] == "run.end":
            status = parsed["data"].get("status")
            if status not in ("ok", "error"):
                return False, f"line {line_number}: run.end status is invalid", count
            before_end = parsed["data"].get("events_before_end")
            if not isinstance(before_end, RawNumber) or before_end != str(expected_sequence):
                return False, f"line {line_number}: run.end event count is invalid", count
            last_event_hash = parsed["data"].get("last_event_hash")
            if last_event_hash != previous or not HASH_PATTERN.fullmatch(last_event_hash or ""):
                return False, f"line {line_number}: run.end previous hash is invalid", count
        previous = claimed
        expected_sequence += 1
        count += 1
        if first_kind is None:
            first_kind = parsed["kind"]
        last_kind = parsed["kind"]

    if count == 0:
        return False, "empty stream", 0
    if first_kind != "run.start":
        return False, "stream must start with run.start", count
    if last_kind != "run.end":
        return False, "stream is consistent but has no final run.end", count
    # A training receipt must have a run envelope and at least one source event.
    # The exact model-specific event set remains the runtime's versioned contract.
    if count < 3:
        return False, "stream has no source lifecycle", count
    return True, f"stream valid: {count} events, last hash {previous}", count


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print(f"usage: {Path(argv[0]).name} EVENTS.ndjson", file=sys.stderr)
        return 2
    try:
        with open(argv[1], "r", encoding="utf-8") as handle:
            ok, message, _ = verify(handle)
    except OSError as error:
        print(f"stream verification failed: {error}", file=sys.stderr)
        return 1
    print(message)
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
