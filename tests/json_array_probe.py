"""Independent proof for typed JSON array ingestion."""

from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target" / "release" / ("seloryn.exe" if __import__("sys").platform == "win32" else "seloryn")
PROJECT = ROOT / "examples" / "projects" / "json-array-pipeline"


def main() -> int:
    result = subprocess.run(
        [str(BINARY), "run", str(PROJECT), "--format=json"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode == 0, result.stderr
    assert json.loads(result.stdout)["value"] == {"ok": True, "value": 11.0}
    assert result.stderr == ""

    with tempfile.TemporaryDirectory(prefix="seloryn-json-array-") as temporary:
        root = Path(temporary)
        shutil.copytree(PROJECT, root / "project")
        data = root / "project" / "data" / "amounts.json"
        data.write_text('{"amounts":[1.5,"bad"]}\n', encoding="utf-8")
        failed = subprocess.run(
            [str(BINARY), "run", str(root / "project"), "--format=json"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
        assert failed.returncode == 65
        assert "as_f64_list requires an array of numbers" in failed.stderr
    print("Seloryn JSON array verification: typed ingestion, iteration and rejection passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
