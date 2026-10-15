"""Independent black-box proof for strict public project manifests."""

from __future__ import annotations

import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target" / "release" / ("seloryn.exe" if sys.platform == "win32" else "seloryn")


def run(command: str, project: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [str(BINARY), command, str(project), "--format=json"],
        cwd=ROOT,
        text=True,
        encoding="utf-8",
        errors="replace",
        capture_output=True,
        check=False,
    )


def assert_rejected(project: Path, expected: str) -> None:
    for command in ("check", "run"):
        result = run(command, project)
        assert result.returncode == 65, (command, result.returncode, result.stdout, result.stderr)
        assert result.stdout == "", (command, result.stdout)
        assert '"code":"SEL-M001"' in result.stderr, (command, result.stderr)
        assert expected in result.stderr, (command, result.stderr)


def write_project(root: Path, manifest: str) -> None:
    source_root = root / "src"
    source_root.mkdir(parents=True)
    (root / "seloryn.toml").write_text(manifest, encoding="utf-8")
    (source_root / "main.sely").write_text("fn main() -> Unit:\n    return\n", encoding="utf-8")


def main() -> int:
    if not BINARY.is_file():
        raise AssertionError(f"release binary is missing: {BINARY}")
    with tempfile.TemporaryDirectory(prefix="seloryn-manifest-probe-") as temporary:
        root = Path(temporary)
        malformed = root / "malformed"
        write_project(malformed, "this is not valid TOML\n")
        assert_rejected(malformed, "invalid project manifest")

        future = root / "future-language"
        write_project(
            future,
            "[package]\nname = \"future\"\nlanguage = \"99.99\"\nsource_root = \"src\"\nentry = \"main.sely\"\n",
        )
        assert_rejected(future, "requires a newer Seloryn runtime")

        wrong_types = root / "wrong-types"
        write_project(
            wrong_types,
            "[package]\nname = \"wrong-types\"\nlanguage = 0.05\nsource_root = [\"src\"]\nentry = \"main.sely\"\n",
        )
        assert_rejected(wrong_types, "must be a string")

        wrong_capability = root / "wrong-capability"
        write_project(
            wrong_capability,
            "[package]\nname = \"wrong-capability\"\nlanguage = \"0.05\"\nsource_root = \"src\"\nentry = \"main.sely\"\n\n[capabilities]\nread = \"data\"\n",
        )
        assert_rejected(wrong_capability, "must be an array of strings")

        wrong_limit = root / "wrong-limit"
        write_project(
            wrong_limit,
            "[package]\nname = \"wrong-limit\"\nlanguage = \"0.05\"\nsource_root = \"src\"\nentry = \"main.sely\"\n\n[limits]\njson_max_bytes = \"large\"\n",
        )
        assert_rejected(wrong_limit, "must be a positive integer")

        unknown_field = root / "unknown-field"
        write_project(
            unknown_field,
            "[package]\nname = \"unknown-field\"\nlanguage = \"0.05\"\nsource_root = \"src\"\nentry = \"main.sely\"\nextra = true\n",
        )
        assert_rejected(unknown_field, "unknown project manifest field")
    print("Seloryn project manifest verification: malformed and future-language projects rejected")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (AssertionError, OSError) as error:
        print(f"project manifest verification FAILED: {error!r}", file=sys.stderr)
        raise SystemExit(1) from error
