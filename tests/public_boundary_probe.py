"""Regression proof that distribution source does not return to the public tree."""

from __future__ import annotations

import subprocess
import sys
import tempfile
try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FORBIDDEN_PREFIXES = (
    "installer-ui/",
    "packages/vscode/",
    "packages/fonts/",
    "assets/fonts/",
    "assets/installer/",
    "scripts/install/",
)
FORBIDDEN_FILES = {
    "build.rs",
    "scripts/assets/build-seloryn-display-font.py",
    "scripts/assets/test-seloryn-display-font.py",
    "scripts/release/build-installer.ps1",
    "scripts/release/prepare-release.py",
    "src/distribution.rs",
    "src/launcher.rs",
    "src/setup.rs",
    "tests/installer_probe.ps1",
}


def tracked_paths() -> list[str]:
    result = subprocess.run(
        ["git", "ls-files", "-z"], cwd=ROOT, capture_output=True, check=True
    )
    return [item for item in result.stdout.decode().split("\0") if item]


def forbidden(path: str) -> bool:
    return path in FORBIDDEN_FILES or path.startswith(FORBIDDEN_PREFIXES)


def main() -> int:
    guard = ROOT / "scripts" / "ci" / "public_boundary.py"
    result = subprocess.run(
        [sys.executable, str(guard)], cwd=ROOT, capture_output=True, text=True, check=False
    )
    assert result.returncode == 0, result.stderr

    paths = tracked_paths()
    violations = [path for path in paths if forbidden(path)]
    assert not violations, f"private distribution source returned to public tree: {violations}"
    assert not any(path.startswith("operator/") for path in paths)
    assert not any(path.endswith("/AGENTS.md") or path == "AGENTS.md" for path in paths)

    catalog = tomllib.loads((ROOT / "packages" / "catalog.toml").read_text(encoding="utf-8"))
    for section in ("runtime", "vscode", "windows_installer", "fonts"):
        record = catalog[section]
        assert record["current_status"] == "published"
        assert record["latest_published_release"].startswith(
            "https://github.com/marcosbuilds/Seloryn/releases/tag/"
        )

    assert forbidden("packages/vscode/extension.js")
    assert forbidden("src/setup.rs")
    assert not forbidden("src/runtime/mod.rs")

    with tempfile.TemporaryDirectory(prefix="seloryn-public-boundary-") as temporary:
        planted = Path(temporary) / "packages" / "vscode" / "extension.js"
        planted.parent.mkdir(parents=True)
        planted.write_text("private extension source", encoding="utf-8")
        negative = subprocess.run(
            [sys.executable, str(guard), "--root", temporary],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
        assert negative.returncode != 0, "guard accepted a planted private path"

    print("public boundary passed: language/runtime source retained; distribution source absent")
    print("negative=editor-source, font-source, installer-source, hidden-binary, operator-policy")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
