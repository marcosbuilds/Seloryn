"""Verify that the earned version is consistent across release surfaces."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path
from typing import NoReturn


ROOT = Path(__file__).resolve().parents[2]


def fail(message: str) -> NoReturn:
    raise SystemExit(f"version verification failed: {message}")


def field(text: str, pattern: str, label: str) -> str:
    match = re.search(pattern, text, re.MULTILINE)
    if not match:
        fail(f"{label} is missing")
    return match.group(1)


def main() -> None:
    version_text = (ROOT / "VERSION.toml").read_text(encoding="utf-8")
    current_language = field(version_text, r'^language = "([^"]+)"$', "current language version")
    runtime_package = field(version_text, r'^runtime_package = "([^"]+)"$', "runtime package version")
    extension_package = field(version_text, r'^extension_package = "([^"]+)"$', "extension package version")
    previous_language = field(version_text, r'^previous_language = "([^"]+)"$', "previous language version")
    previous_runtime = field(version_text, r'^previous_runtime_package = "([^"]+)"$', "previous runtime version")
    previous_extension = field(version_text, r'^previous_extension_package = "([^"]+)"$', "previous extension version")
    gain_id = field(version_text, r'^id = "([^"]+)"$', "gain id")

    cargo = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    if field(cargo, r'^version = "([^"]+)"$', "Cargo package version") != runtime_package:
        fail("Cargo.toml does not match VERSION.toml")
    if field(cargo, r'^language-version = "([^"]+)"$', "Cargo language version") != current_language:
        fail("Cargo language metadata does not match VERSION.toml")

    main = (ROOT / "src" / "main.rs").read_text(encoding="utf-8")
    if field(main, r'^const LANGUAGE_VERSION: &str = "([^"]+)";', "CLI language version") != current_language:
        fail("CLI language version does not match VERSION.toml")

    extension = json.loads((ROOT / "tooling" / "vscode" / "package.json").read_text(encoding="utf-8"))
    if extension["version"] != extension_package:
        fail("VS Code package version does not match VERSION.toml")

    lock = (ROOT / "Cargo.lock").read_text(encoding="utf-8")
    lock_block = lock.split('name = "seloryn"', 1)[1].split('\n\n', 1)[0]
    if f'version = "{runtime_package}"' not in lock_block:
        fail("Cargo.lock does not match VERSION.toml")

    npm_lock = json.loads((ROOT / "tooling" / "vscode" / "package-lock.json").read_text(encoding="utf-8"))
    if npm_lock["version"] != extension_package or npm_lock["packages"][""]["version"] != extension_package:
        fail("package-lock.json does not match VERSION.toml")

    if current_language == previous_language:
        fail("current language version did not advance")
    if runtime_package == previous_runtime:
        fail("runtime package version did not advance")
    if extension_package == previous_extension:
        fail("extension package version did not advance")
    if not gain_id:
        fail("no earned gain is recorded")

    print(
        f"Seloryn version comparison: {previous_language} -> {current_language}; "
        f"runtime {previous_runtime} -> {runtime_package}; "
        f"extension {previous_extension} -> {extension_package}; gain={gain_id}"
    )


if __name__ == "__main__":
    main()
