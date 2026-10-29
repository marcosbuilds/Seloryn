"""Fail closed when private product content crosses the public boundary.

The default mode audits the Git index, which is the boundary that CI can
review.  ``--root`` audits an extracted tree as a second line of defence for
archives and handoffs.  Both modes reject private product paths, generated
distributions, native binaries, symlinks/submodules and known binary magic
hidden behind an innocent-looking extension.
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path


PRIVATE_PARTS = {
    "installer-ui",
    "packages/vscode",
    "packages/fonts",
    "assets/fonts",
    "assets/installer",
    "scripts/assets",
}
PRIVATE_FILES = {
    "build.rs",
    "src/setup.rs",
    "src/launcher.rs",
    "src/distribution.rs",
    "scripts/release/build-installer.ps1",
}
FORBIDDEN_SUFFIXES = {
    ".vsix",
    ".ttf",
    ".otf",
    ".woff",
    ".woff2",
    ".exe",
    ".msi",
    ".appx",
    ".dmg",
    ".deb",
    ".rpm",
}
MAGIC = {
    b"\x00\x01\x00\x00": "TrueType font",
    b"OTTO": "OpenType font",
    b"wOFF": "WOFF font",
    b"wOF2": "WOFF2 font",
    b"MZ": "PE executable",
}


def normalise(value: str) -> str:
    return value.replace("\\", "/").lstrip("./")


def path_reason(relative: str) -> str | None:
    value = normalise(relative)
    if value in PRIVATE_FILES:
        return "private product file"
    for private in PRIVATE_PARTS:
        if value == private or value.startswith(private + "/"):
            return f"private product path ({private})"
    if Path(value).suffix.lower() in FORBIDDEN_SUFFIXES:
        return "private/generated distribution suffix"
    return None


def content_reason(path: Path) -> str | None:
    try:
        with path.open("rb") as handle:
            prefix = handle.read(4096)
    except OSError as error:
        return f"unreadable file: {error}"
    for signature, label in MAGIC.items():
        if prefix.startswith(signature):
            return f"{label} signature hidden under a public path"
    return None


def tracked_entries(root: Path) -> list[tuple[str, str]]:
    result = subprocess.run(
        ["git", "-C", str(root), "ls-files", "--stage", "-z"],
        capture_output=True,
        check=False,
    )
    if result.returncode:
        raise SystemExit(result.stderr.decode(errors="replace").strip())
    entries: list[tuple[str, str]] = []
    for raw in result.stdout.split(b"\0"):
        if not raw:
            continue
        metadata, raw_path = raw.split(b"\t", 1)
        mode = metadata.split(b" ", 1)[0].decode()
        entries.append((mode, raw_path.decode("utf-8")))
    return entries


def audit_tracked(root: Path) -> list[str]:
    failures: list[str] = []
    for mode, relative in tracked_entries(root):
        reason = path_reason(relative)
        if reason:
            failures.append(f"{relative}: {reason}")
        if mode in {"120000", "160000"}:
            failures.append(f"{relative}: symlink/submodule mode {mode} is not public content")
        path = root / Path(relative)
        if path.is_file():
            reason = content_reason(path)
            if reason:
                failures.append(f"{relative}: {reason}")
    return failures


def audit_tree(root: Path) -> list[str]:
    failures: list[str] = []
    for path in root.rglob("*"):
        relative = path.relative_to(root).as_posix()
        reason = path_reason(relative)
        if reason:
            failures.append(f"{relative}: {reason}")
        if path.is_symlink():
            failures.append(f"{relative}: symlink is not public content")
        elif path.is_file():
            reason = content_reason(path)
            if reason:
                failures.append(f"{relative}: {reason}")
    return failures


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, help="audit an extracted tree instead of the Git index")
    arguments = parser.parse_args()
    root = (arguments.root or Path(__file__).resolve().parents[2]).resolve()
    failures = audit_tree(root) if arguments.root else audit_tracked(root)
    if failures:
        print("PUBLIC BOUNDARY FAILED", file=sys.stderr)
        for failure in failures:
            print(f"- {failure}", file=sys.stderr)
        return 1
    scope = f"tree {root}" if arguments.root else "tracked Git content"
    print(f"PUBLIC BOUNDARY PASS: {scope}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
