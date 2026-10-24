"""Black-box probe for compact analysis context and diagnostic behavior."""

from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target" / "release" / ("seloryn.exe" if __import__("sys").platform == "win32" else "seloryn")
FIXTURE = ROOT / "tests" / "fixtures" / "structured-inspection"


def run(*args: str, cwd: Path = ROOT) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [str(BINARY), *args],
        cwd=cwd,
        text=True,
        capture_output=True,
        check=False,
    )


def main() -> int:
    if not BINARY.is_file():
        raise SystemExit(f"release CLI is required at {BINARY}")

    inspected = run("inspect", str(FIXTURE), "--format=json")
    assert inspected.returncode == 0, inspected.stderr
    assert inspected.stderr == "", inspected.stderr
    report = json.loads(inspected.stdout)
    assert report["schema"] == "seloryn.analysis.v1"
    assert report["diagnostics"] == []
    assert report["context"]["kind"] == "project"
    files = {entry["path"]: entry for entry in report["context"]["files"]}
    assert set(files) == {
        "src/inspection_contract.sely",
        "src/inspection-entry.sely",
    }
    contract = files["src/inspection_contract.sely"]
    assert [symbol["name"] for symbol in contract["symbols"]] == [
        "Contract",
        "make_contract",
    ]
    assert all(symbol["public"] for symbol in contract["symbols"])
    entry = files["src/inspection-entry.sely"]
    assert [item["module"] for item in entry["imports"]] == [
        "inspection_contract",
        "json",
    ]
    assert [item["kind"] for item in entry["imports"]] == ["local", "standard"]
    assert entry["imports"][0]["symbols"] == ["Contract", "make_contract"]
    assert all(
        symbol["start_byte"] < symbol["end_byte"]
        for item in files.values()
        for symbol in item["symbols"]
    )

    human = run("inspect", str(FIXTURE), "--format=human")
    assert human.returncode == 0, human.stderr
    assert human.stdout == (
        f"inspected project {FIXTURE} files=2 symbols=3 imports=2 diagnostics=0\n"
    )

    with tempfile.TemporaryDirectory(prefix="seloryn-inspection-") as directory:
        invalid = Path(directory) / "invalid-project"
        shutil.copytree(FIXTURE, invalid)
        entry_path = invalid / "src" / "inspection-entry.sely"
        entry_path.write_text(
            "from missing_module import Missing\n\nfn main() -> Unit:\n    return\n",
            encoding="utf-8",
        )
        failed = run("inspect", str(invalid), "--format=json")
        assert failed.returncode == 65
        assert failed.stderr == ""
        failure_report = json.loads(failed.stdout)
        assert failure_report["schema"] == "seloryn.analysis.v1"
        assert failure_report["diagnostics"][0]["code"] == "SEL-M002"
        assert failure_report["context"] == {"files": [], "kind": "project"}

    with tempfile.NamedTemporaryFile(
        mode="w", suffix=".sely", encoding="utf-8", delete=False
    ) as source_file:
        source_file.write("fn main() -> Unit:\n    if 1:\n        return\n")
        source_path = Path(source_file.name)
    try:
        failed_source = run("inspect", str(source_path), "--format=json")
        assert failed_source.returncode == 65
        source_report = json.loads(failed_source.stdout)
        assert source_report["diagnostics"][0]["code"] == "SEL-T004"
        assert source_report["context"]["kind"] == "source"
    finally:
        source_path.unlink(missing_ok=True)

    print("structured inspection probe passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
