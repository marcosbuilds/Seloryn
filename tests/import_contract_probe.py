"""Black-box proof that every symbol in a local import is validated."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib


ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target" / "release" / ("seloryn.exe" if sys.platform == "win32" else "seloryn")
CURRENT_LANGUAGE = tomllib.loads((ROOT / "VERSION.toml").read_text(encoding="utf-8"))["current"]["language"]


MANIFEST = """[package]
name = "import-contract"
language = "{language}"
source_root = "src"
entry = "main.sely"

[capabilities]
read = []
write = []
""".format(language=CURRENT_LANGUAGE)


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
            "module app\n\nlet secret: F64 = 99.0\n\nfn hidden() -> F64:\n    return secret\n\npub fn one() -> F64:\n    return 1.0\n\npub fn two() -> F64:\n    return 2.0\n",
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
            "import app as scope\n\nfn main() -> F64:\n    return scope.one()\n",
            encoding="utf-8",
        )
        aliased = run(project)
        assert aliased.returncode == 0, (aliased.stdout, aliased.stderr)
        assert json.loads(aliased.stdout)["value"] == 1.0, aliased.stdout

        entry.write_text(
            "import app as scope\n\nfn main() -> F64:\n    return scope.hidden()\n",
            encoding="utf-8",
        )
        private_alias = run(project)
        assert private_alias.returncode == 65, (private_alias.stdout, private_alias.stderr)
        assert "SEL-M004" in private_alias.stdout or "SEL-M004" in private_alias.stderr
        assert "not exported" in private_alias.stdout or "not exported" in private_alias.stderr

        entry.write_text(
            "import app\n\nfn main() -> F64:\n    return hidden()\n",
            encoding="utf-8",
        )
        private_unqualified = run(project)
        assert private_unqualified.returncode == 65, (
            private_unqualified.stdout,
            private_unqualified.stderr,
        )
        assert "SEL-M004" in private_unqualified.stdout or "SEL-M004" in private_unqualified.stderr
        assert "not in scope" in private_unqualified.stdout or "not in scope" in private_unqualified.stderr

        entry.write_text(
            "import app\n\nfn main() -> F64:\n    let leaked: F64 = secret\n    return leaked\n",
            encoding="utf-8",
        )
        private_binding = run(project)
        assert private_binding.returncode == 65, (
            private_binding.stdout,
            private_binding.stderr,
        )
        assert "SEL-M004" in private_binding.stdout or "SEL-M004" in private_binding.stderr
        assert "not in scope" in private_binding.stdout or "not in scope" in private_binding.stderr

        entry.write_text(
            "from app import one, missing\n\nfn main() -> F64:\n    return one()\n",
            encoding="utf-8",
        )
        invalid = run(project)
        assert invalid.returncode == 65, (invalid.returncode, invalid.stdout, invalid.stderr)
        assert "SEL-M004" in invalid.stdout or "SEL-M004" in invalid.stderr
        assert "missing" in invalid.stdout or "missing" in invalid.stderr
    print("Seloryn import contract proof: PASS")
    print("gates | multi_symbol_import=0 alias_public=0 private_alias=65 private_unqualified=65 private_binding=65 missing_symbol=65")


if __name__ == "__main__":
    try:
        main()
    except (AssertionError, OSError, subprocess.SubprocessError, UnicodeError, json.JSONDecodeError) as error:
        print(f"import contract probe FAILED: {error!r}", file=sys.stderr)
        raise SystemExit(1) from error
