"""Black-box proof that every symbol in a local import is validated."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target" / "release" / ("seloryn.exe" if sys.platform == "win32" else "seloryn")


MANIFEST = """[package]
name = "import-contract"
language = "0.12"
source_root = "src"
entry = "main.sely"

[capabilities]
read = []
write = []
"""


def run(project: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [str(BINARY), "run", str(project), "--format=json"],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )


def main() -> None:
    assert BINARY.is_file(), BINARY
    with tempfile.TemporaryDirectory(prefix="seloryn-import-contract-") as temporary:
        project = Path(temporary) / "project"
        (project / "src").mkdir(parents=True)
        (project / "seloryn.toml").write_text(MANIFEST, encoding="utf-8")
        (project / "src" / "app.sely").write_text(
            "module app\n\npub fn one() -> F64:\n    return 1.0\n\npub fn two() -> F64:\n    return 2.0\n",
            encoding="utf-8",
        )
        entry = project / "src" / "main.sely"
        entry.write_text(
            "from app import one, two\n\nfn main() -> F64:\n    return one() + two()\n",
            encoding="utf-8",
        )
        valid = run(project)
        assert valid.returncode == 0, (valid.stdout, valid.stderr)
        envelope = json.loads(valid.stdout)
        assert envelope["value"] == 3.0, envelope

        entry.write_text(
            "from app import one, missing\n\nfn main() -> F64:\n    return one()\n",
            encoding="utf-8",
        )
        invalid = run(project)
        assert invalid.returncode == 65, (invalid.returncode, invalid.stdout, invalid.stderr)
        assert "SEL-M004" in invalid.stdout or "SEL-M004" in invalid.stderr
        assert "missing" in invalid.stdout or "missing" in invalid.stderr
    print("Seloryn import contract proof: PASS")
    print("gates | multi_symbol_import=0 missing_symbol=65")


if __name__ == "__main__":
    try:
        main()
    except (AssertionError, OSError, subprocess.SubprocessError, UnicodeError, json.JSONDecodeError) as error:
        print(f"import contract probe FAILED: {error!r}", file=sys.stderr)
        raise SystemExit(1) from error
