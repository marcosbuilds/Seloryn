"""Bounded adversarial campaign for the public NDJSON receipt verifier."""

from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
VERIFIER_PATH = ROOT / "scripts" / "streaming" / "verify-event-chain.py"


def load_verifier():
    spec = importlib.util.spec_from_file_location("seloryn_event_chain_adversarial", VERIFIER_PATH)
    if spec is None or spec.loader is None:
        raise AssertionError(f"cannot load verifier: {VERIFIER_PATH}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def event_line(verifier, sequence: int, kind: str, data: dict, previous: str) -> tuple[str, str]:
    content = {
        "schema": 2,
        "run": "r_" + "a" * 64,
        "seq": sequence,
        "kind": kind,
        "phase": "run",
        "data": data,
        "prev": previous,
    }
    raw = json.dumps(content, ensure_ascii=False, separators=(",", ":"))
    raw = raw.replace('"__EXTREME_EXPONENT__"', "1e999999", 1)
    raw = raw.replace('"__EXTREME_UNDERFLOW__"', "1e-999999", 1)
    parsed = json.loads(
        raw,
        object_pairs_hook=verifier.reject_duplicate_keys,
        parse_int=verifier.RawNumber,
        parse_float=verifier.RawNumber,
        parse_constant=verifier.reject_non_finite,
    )
    claimed = verifier.digest(parsed)
    return raw[:-1] + ",\"hash\":" + json.dumps(claimed) + "}", claimed


def valid_lines(verifier) -> list[str]:
    previous = verifier.ZERO_HASH
    lines: list[str] = []
    events = [
        ("run.start", {"payload": "café🙂漢字"}),
        (
            "audit.record",
            {
                "large_integer": 10**200,
                "large_exponent": "__EXTREME_EXPONENT__",
                "small_exponent": "__EXTREME_UNDERFLOW__",
                "marker": "sk-test-never-secret",
            },
        ),
        ("run.end", {"status": "ok", "events_before_end": 2, "last_event_hash": None}),
    ]
    for sequence, (kind, data) in enumerate(events):
        if kind == "run.end":
            data["last_event_hash"] = previous
        line, previous = event_line(verifier, sequence, kind, data, previous)
        lines.append(line)
    return lines


def run_cli(path: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(VERIFIER_PATH), str(path)],
        cwd=ROOT,
        text=True,
        encoding="utf-8",
        errors="strict",
        capture_output=True,
        check=False,
    )


def write_text(path: Path, text: str) -> None:
    path.write_text(text + ("" if text.endswith("\n") else "\n"), encoding="utf-8")


def expect_rejection(path: Path, message: str, secret: str) -> None:
    result = run_cli(path)
    assert result.returncode == 1, (path.name, result.returncode, result.stdout, result.stderr)
    assert result.stdout == message + "\n", (path.name, result.stdout, result.stderr)
    assert result.stderr == "", (path.name, result.stdout, result.stderr)
    assert secret not in result.stdout + result.stderr, (path.name, result.stdout, result.stderr)


def main() -> int:
    verifier = load_verifier()
    lines = valid_lines(verifier)
    secret = "sk-test-never-secret"

    valid, message, count = verifier.verify(lines)
    assert valid, message
    assert count == 3, count

    with tempfile.TemporaryDirectory(prefix="seloryn-adversarial-stream-") as directory:
        root = Path(directory)
        valid_path = root / "valid.ndjson"
        write_text(valid_path, "\n".join(lines))
        valid_result = run_cli(valid_path)
        assert valid_result.returncode == 0, (valid_result.stdout, valid_result.stderr)
        assert valid_result.stderr == "", valid_result.stderr
        assert valid_result.stdout.startswith("stream valid: 3 events, last hash sha256:"), valid_result.stdout

        for label, replacement in (("nan", "NaN"), ("infinity", "Infinity")):
            path = root / f"{label}.ndjson"
            write_text(path, lines[0].replace('"café🙂漢字"', replacement, 1))
            expect_rejection(path, "line 1: non-finite JSON number", secret)

        truncated = root / "truncated.ndjson"
        write_text(truncated, "\n".join(lines[:2] + [lines[2][:-10]]))
        expect_rejection(truncated, "line 3: invalid JSON", secret)

        duplicate = root / "duplicate.ndjson"
        write_text(
            duplicate,
            lines[0].replace(
                '"schema":2,',
                '"schema":2,"authorization=do-not-log":1,"authorization=do-not-log":2,',
                1,
            ),
        )
        expect_rejection(duplicate, "line 1: duplicate key", "authorization=do-not-log")

        tampered = root / "tampered.ndjson"
        write_text(tampered, lines[0].replace('"café🙂漢字"', '"changed"', 1))
        expect_rejection(tampered, "line 1: hash mismatch", secret)

        surrogate = root / "surrogate.ndjson"
        write_text(surrogate, lines[0].replace('"café🙂漢字"', '"\\ud800"', 1))
        expect_rejection(surrogate, "line 1: invalid Unicode scalar", secret)

        deep = root / "deep.ndjson"
        depth = verifier.MAX_NESTING + 1
        write_text(deep, "[" * depth + "0" + "]" * depth)
        expect_rejection(
            deep,
            f"line 1: JSON nesting exceeds {verifier.MAX_NESTING} levels",
            secret,
        )

        oversized = root / "oversized.ndjson"
        write_text(oversized, "x" * verifier.MAX_LINE_BYTES)
        expect_rejection(
            oversized,
            f"line 1: input line exceeds {verifier.MAX_LINE_BYTES} bytes",
            secret,
        )

        stream_limit = root / "stream-limit.ndjson"
        stream_limit.write_text(
            (" " * (verifier.MAX_LINE_BYTES - 1) + "\n") * 9,
            encoding="utf-8",
        )
        expect_rejection(
            stream_limit,
            f"stream exceeds {verifier.MAX_STREAM_BYTES} bytes",
            secret,
        )

        invalid_utf8 = root / "invalid-utf8.ndjson"
        invalid_utf8.write_bytes(b'{"schema":2,\xff}\n')
        invalid_utf8_result = run_cli(invalid_utf8)
        assert invalid_utf8_result.returncode == 1, invalid_utf8_result
        assert invalid_utf8_result.stdout == "", invalid_utf8_result.stdout
        assert invalid_utf8_result.stderr == "stream verification failed: invalid UTF-8 input\n", invalid_utf8_result.stderr

    print("Seloryn adversarial stream verification: bounded Unicode, numeric and corruption campaign passed")
    print(
        "positive | unicode=1 large_integer=1 extreme_exponents=1 "
        "valid_exit=0 stable_prefix=1"
    )
    print(
        "negative | nan=1 infinity=1 truncated=1 duplicate=1 tamper=1 surrogate=1 "
        "depth=1 line_limit=1 stream_limit=1 invalid_utf8=1 exit=1 redaction=1"
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (
        AssertionError,
        OSError,
        subprocess.SubprocessError,
        UnicodeError,
        json.JSONDecodeError,
    ) as error:
        print(f"adversarial stream probe FAILED: {error!r}", file=sys.stderr)
        raise SystemExit(1) from error
