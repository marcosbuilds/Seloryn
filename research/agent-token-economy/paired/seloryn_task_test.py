"""Black-box proof for the executable Seloryn TOKEN-PAIR-01 condition."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
SOURCE = Path(__file__).with_name("seloryn_task.sely")
BINARY = ROOT / "target" / "release" / ("seloryn.exe" if sys.platform == "win32" else "seloryn")

INPUTS = (
    "hello world",
    "caractere por caractere",
    "Tensor[F32, shape=[2, 2]]",
)
EXPECTED_CODEPOINTS = (11, 23, 25)
EXPECTED_TOTAL = sum(EXPECTED_CODEPOINTS)


def run(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [str(BINARY), *args],
        cwd=ROOT,
        text=True,
        encoding="utf-8",
        capture_output=True,
        check=False,
    )


def main() -> int:
    if not BINARY.is_file():
        raise SystemExit(f"release CLI is required at {BINARY}")

    source = SOURCE.read_text(encoding="utf-8")
    assert all(value in source for value in INPUTS), "shared TOKEN-PAIR-01 inputs changed"
    assert "task_id: visible-text-metrics" in source
    assert "supported_metric: unicode_codepoints" in source
    assert "utf8_bytes_status: unsupported" in source
    assert "stable_json_status: unsupported" in source
    assert "tokenization_status: unsupported" in source
    assert "openai_bpe_status: unsupported" in source
    assert "billing_status: not_provider_billing" in source

    checked = run("check", str(SOURCE))
    assert checked.returncode == 0, checked.stderr
    assert checked.stderr == "", checked.stderr
    assert "checked" in checked.stdout

    first = run("run", str(SOURCE), "--format=json")
    second = run("run", str(SOURCE), "--format=json")
    for result in (first, second):
        assert result.returncode == 0, result.stderr
        assert result.stderr == "", result.stderr

    first_value = json.loads(first.stdout)
    second_value = json.loads(second.stdout)
    assert first_value == second_value, (first_value, second_value)
    assert first_value["value"] == EXPECTED_TOTAL, first_value
    assert EXPECTED_TOTAL == 59

    print(
        "TOKEN-PAIR-01 Seloryn passed: "
        f"codepoints={EXPECTED_CODEPOINTS} total={EXPECTED_TOTAL}; "
        "tokenization=unsupported; billing=not_provider_billing"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
