#!/usr/bin/env python3
"""Verify Seloryn streaming v2 NDJSON integrity and lifecycle."""

from __future__ import annotations

import hashlib
import hmac
import json
import re
import sys
from pathlib import Path
from typing import Iterable


ZERO_HASH = "sha256:" + "0" * 64
HASH_PATTERN = re.compile(r"^sha256:[0-9a-f]{64}$")
REQUIRED = ("schema", "run", "seq", "kind", "phase", "data", "prev", "hash")
RUN_PATTERN = re.compile(r"^r_[0-9a-f]{64}$")
MAX_LINE_BYTES = 1_048_576
MAX_STREAM_BYTES = 8_388_608
MAX_NESTING = 128


class RawNumber(str):
    """JSON number whose source spelling must survive canonicalization."""


class DuplicateKeyError(ValueError):
    pass


class NonFiniteNumberError(ValueError):
    pass


def reject_duplicate_keys(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        if key in result:
            raise DuplicateKeyError(f"duplicate key {key!r}")
        result[key] = value
    return result


def reject_non_finite(value: str) -> None:
    raise NonFiniteNumberError(value)


def exceeds_nesting(raw: str) -> bool:
    depth = 0
    maximum = 0
    in_string = False
    escaped = False
    for character in raw:
        if in_string:
            if escaped:
                escaped = False
            elif character == "\\":
                escaped = True
            elif character == '"':
                in_string = False
            continue
        if character == '"':
            in_string = True
        elif character in "[{":
            depth += 1
            maximum = max(maximum, depth)
        elif character in "]}":
            depth -= 1
    return maximum > MAX_NESTING


def contains_surrogate(value: object) -> bool:
    if isinstance(value, str):
        return any(0xD800 <= ord(character) <= 0xDFFF for character in value)
    if isinstance(value, list):
        return any(contains_surrogate(item) for item in value)
    if isinstance(value, dict):
        return any(
            contains_surrogate(key) or contains_surrogate(item)
            for key, item in value.items()
        )
    return False


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


def verify(lines: Iterable[str], hmac_key: bytes | None = None) -> tuple[bool, str, int]:
    previous = ZERO_HASH
    run_id: str | None = None
    expected_sequence = 0
    count = 0
    first_kind: str | None = None
    last_kind: str | None = None
    stream_bytes = 0

    for line_number, raw in enumerate(lines, 1):
        line_bytes = len(raw.encode("utf-8"))
        stream_bytes += line_bytes
        if line_bytes > MAX_LINE_BYTES:
            return False, f"line {line_number}: input line exceeds {MAX_LINE_BYTES} bytes", count
        if stream_bytes > MAX_STREAM_BYTES:
            return False, f"stream exceeds {MAX_STREAM_BYTES} bytes", count
        if exceeds_nesting(raw):
            return False, f"line {line_number}: JSON nesting exceeds {MAX_NESTING} levels", count
        if not raw.strip():
            continue
        try:
            parsed = json.loads(
                raw,
                object_pairs_hook=reject_duplicate_keys,
                parse_int=RawNumber,
                parse_float=RawNumber,
                parse_constant=reject_non_finite,
            )
        except DuplicateKeyError:
            return False, f"line {line_number}: duplicate key", count
        except NonFiniteNumberError:
            return False, f"line {line_number}: non-finite JSON number", count
        except json.JSONDecodeError:
            return False, f"line {line_number}: invalid JSON", count
        except RecursionError:
            return False, f"line {line_number}: JSON nesting exceeds {MAX_NESTING} levels", count
        if not isinstance(parsed, dict):
            return False, f"line {line_number}: event is not an object", count
        if contains_surrogate(parsed):
            return False, f"line {line_number}: invalid Unicode scalar", count
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
        signature = content.pop("signature", None)
        try:
            actual = digest(content)
        except (TypeError, UnicodeEncodeError):
            return False, f"line {line_number}: unsupported JSON value", count
        if actual != claimed:
            return False, f"line {line_number}: hash mismatch", count
        if signature is not None:
            if not isinstance(signature, str) or not re.fullmatch(
                r"hmac-sha256:[0-9a-f]{64}", signature
            ):
                return False, f"line {line_number}: signature format is invalid", count
            if hmac_key is None:
                return False, f"line {line_number}: signature requires --hmac-key", count
            expected_signature = "hmac-sha256:" + hmac.new(
                hmac_key, claimed.encode("ascii"), hashlib.sha256
            ).hexdigest()
            if not hmac.compare_digest(signature, expected_signature):
                return False, f"line {line_number}: signature mismatch", count
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
    if len(argv) not in (2, 4) or (len(argv) == 4 and argv[2] != "--hmac-key"):
        print(
            f"usage: {Path(argv[0]).name} EVENTS.ndjson [--hmac-key KEY]",
            file=sys.stderr,
        )
        return 2
    hmac_key = argv[3].encode("utf-8") if len(argv) == 4 else None
    try:
        with open(argv[1], "r", encoding="utf-8") as handle:
            ok, message, _ = verify(handle, hmac_key)
    except UnicodeDecodeError:
        print("stream verification failed: invalid UTF-8 input", file=sys.stderr)
        return 1
    except OSError:
        print("stream verification failed: cannot read input", file=sys.stderr)
        return 1
    print(message)
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
