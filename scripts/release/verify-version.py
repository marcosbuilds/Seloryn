"""Verify earned language and package versions against public evidence."""

from __future__ import annotations

import json
import re
import subprocess
from pathlib import Path
from typing import NoReturn

try:
    import tomllib
except ModuleNotFoundError:  # Python 3.10 and older supported developer hosts.
    import tomli as tomllib


ROOT = Path(__file__).resolve().parents[2]
LANGUAGE_PATTERN = re.compile(r"(?:0|[1-9][0-9]*)\.[0-9]{2}\Z")
PACKAGE_PATTERN = re.compile(r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\Z")


def fail(message: str) -> NoReturn:
    raise SystemExit(f"version verification failed: {message}")


def parse_language_version(value: str, label: str) -> tuple[int, int]:
    if not LANGUAGE_PATTERN.fullmatch(value):
        fail(f"{label} is not canonical M.mm")
    major, minor = value.split(".")
    return int(major), int(minor)


def parse_package_version(value: str, label: str) -> tuple[int, int, int]:
    if not PACKAGE_PATTERN.fullmatch(value):
        fail(f"{label} is not canonical SemVer X.Y.Z")
    major, minor, patch = value.split(".")
    return int(major), int(minor), int(patch)


def package_base_for_language(language: tuple[int, int]) -> tuple[int, int]:
    """Map language M.mm to the package SemVer base M.m."""
    return language[0], language[1]


def require_text(record: dict[str, object], key: str, label: str) -> str:
    value = record.get(key)
    if not isinstance(value, str) or not value:
        fail(f"{label} is missing")
    return value


def require_evidence(root: Path, records: list[dict[str, object]], label: str) -> None:
    resolved_root = root.resolve()
    for record in records:
        record_id = require_text(record, "id", f"{label} id")
        evidence = require_text(record, "evidence", f"{label} {record_id} evidence")
        for relative_path in (item.strip() for item in evidence.split(";")):
            if not relative_path:
                fail(f"{label} {record_id} contains an empty evidence path")
            candidate_path = Path(relative_path)
            if candidate_path.is_absolute() or ".." in candidate_path.parts:
                fail(f"{label} {record_id} evidence path must stay inside the public repository: {relative_path}")
            resolved_path = (root / candidate_path).resolve()
            if resolved_root not in (resolved_path, *resolved_path.parents):
                fail(f"{label} {record_id} evidence path escapes the public repository: {relative_path}")
            if not resolved_path.exists():
                fail(f"{label} {record_id} evidence path does not exist: {relative_path}")
            tracked = subprocess.run(
                ["git", "-C", str(root), "ls-files", "--", relative_path],
                capture_output=True,
                text=True,
                check=False,
            )
            if tracked.returncode != 0 or not tracked.stdout.strip():
                fail(f"{label} {record_id} evidence path is not tracked publicly: {relative_path}")


def require_increasing(current: tuple[int, ...], previous: tuple[int, ...], label: str) -> None:
    if current <= previous:
        fail(f"{label} version must advance monotonically")


def validate(root: Path = ROOT) -> None:
    version_text = (root / "VERSION.toml").read_text(encoding="utf-8")
    document = tomllib.loads(version_text)
    current = document.get("current")
    comparison = document.get("comparison")
    language_gains = document.get("gains")
    extension_gains = document.get("extension_gains")
    runtime_gains = document.get("runtime_gains", [])
    if not isinstance(current, dict) or not isinstance(comparison, dict):
        fail("VERSION.toml must define [current] and [comparison]")
    if not isinstance(language_gains, list) or not language_gains:
        fail("VERSION.toml must record at least one language gain")
    if not isinstance(extension_gains, list) or not extension_gains:
        fail("VERSION.toml must record at least one extension gain")
    if not isinstance(runtime_gains, list):
        fail("VERSION.toml runtime_gains must be an array")

    current_language = require_text(current, "language", "current language version")
    runtime_package = require_text(current, "runtime_package", "runtime package version")
    extension_package = require_text(current, "extension_package", "extension package version")
    previous_language = require_text(comparison, "previous_language", "previous language version")
    previous_runtime = require_text(comparison, "previous_runtime_package", "previous runtime version")
    previous_extension = require_text(comparison, "previous_extension_package", "previous extension version")
    current_language_number = parse_language_version(current_language, "current language version")
    previous_language_number = parse_language_version(previous_language, "previous language version")
    current_runtime_number = parse_package_version(runtime_package, "current runtime version")
    previous_runtime_number = parse_package_version(previous_runtime, "previous runtime version")
    current_extension_number = parse_package_version(extension_package, "current extension version")
    previous_extension_number = parse_package_version(previous_extension, "previous extension version")

    language_package_base = package_base_for_language(current_language_number)
    if current_runtime_number[:2] != language_package_base:
        fail("runtime package major/minor must follow the current language version")
    if current_runtime_number[2] != 0 and not runtime_gains:
        fail("runtime package patch updates require an explicit runtime gain ledger")
    if current_extension_number[:2] != language_package_base:
        fail("extension package major/minor must follow the current language version")

    language_ids = [require_text(record, "id", "language gain id") for record in language_gains]
    extension_ids = [require_text(record, "id", "extension gain id") for record in extension_gains]
    if len(set(language_ids)) != len(language_ids):
        fail("language gain ids must be unique")
    if len(set(extension_ids)) != len(extension_ids):
        fail("extension gain ids must be unique")
    require_evidence(root, language_gains, "language gain")
    require_evidence(root, extension_gains, "extension gain")
    if runtime_gains:
        runtime_ids = [require_text(record, "id", "runtime gain id") for record in runtime_gains]
        if len(set(runtime_ids)) != len(runtime_ids):
            fail("runtime gain ids must be unique")
        require_evidence(root, runtime_gains, "runtime gain")

    expected_gain = (0, 1)
    for record in language_gains:
        gain_version = parse_language_version(
            require_text(record, "language", "language gain version"),
            "language gain version",
        )
        if gain_version != expected_gain:
            fail("language gains must form an ordered one-credit sequence")
        expected_gain = (gain_version[0] + (gain_version[1] + 1) // 100, (gain_version[1] + 1) % 100)

    previous_extension_gain = None
    for record in extension_gains:
        extension_language = parse_language_version(
            require_text(record, "language", "extension gain language"),
            "extension gain language",
        )
        extension_gain = parse_package_version(
            require_text(record, "package", "extension gain package"),
            "extension gain package",
        )
        if extension_gain[:2] != package_base_for_language(extension_language):
            fail("extension gain package must follow its language compatibility line")
        if extension_language > current_language_number:
            fail("extension gain cannot target a future language version")
        if previous_extension_gain is not None and extension_gain <= previous_extension_gain:
            fail("extension gains must advance monotonically")
        previous_extension_gain = extension_gain

    previous_runtime_gain = None
    for record in runtime_gains:
        runtime_language = parse_language_version(
            require_text(record, "language", "runtime gain language"),
            "runtime gain language",
        )
        runtime_gain = parse_package_version(
            require_text(record, "package", "runtime gain package"),
            "runtime gain package",
        )
        if runtime_language != current_language_number:
            fail("runtime gain must target the current language compatibility line")
        if runtime_gain[:2] != language_package_base:
            fail("runtime gain package must follow its language compatibility line")
        if runtime_gain[2] == 0:
            fail("runtime gain package must be a patch update")
        if previous_runtime_gain is not None and runtime_gain <= previous_runtime_gain:
            fail("runtime gains must advance monotonically")
        previous_runtime_gain = runtime_gain

    latest_language = language_gains[-1]
    if require_text(latest_language, "language", "latest language gain version") != current_language:
        fail("latest language gain does not match current language version")
    expected_language = (
        previous_language_number[0] + (previous_language_number[1] + 1) // 100,
        (previous_language_number[1] + 1) % 100,
    )
    if current_language_number == previous_language_number:
        if current_runtime_number[2] == 0 or previous_runtime_gain != current_runtime_number:
            fail("same-language runtime updates require a matching runtime gain ledger")
    elif current_language_number != expected_language:
        fail("current language version must advance by exactly one accepted gain")
    elif runtime_gains:
        fail("language gains and runtime-only patch gains cannot be mixed")

    require_increasing(current_runtime_number, previous_runtime_number, "runtime package")
    latest_extension = extension_gains[-1]
    if require_text(latest_extension, "package", "latest extension gain package") != extension_package:
        fail("latest extension gain does not match current extension version")
    if require_text(latest_extension, "language", "latest extension gain language") != current_language:
        fail("latest extension gain must target the current language version")
    if current_extension_number < previous_extension_number:
        fail("extension package version must not move backwards")

    cargo = (root / "Cargo.toml").read_text(encoding="utf-8")
    cargo_package = re.search(r'^version = "([^"]+)"$', cargo, re.MULTILINE)
    cargo_language = re.search(r'^language-version = "([^"]+)"$', cargo, re.MULTILINE)
    if not cargo_package or cargo_package.group(1) != runtime_package:
        fail("Cargo.toml does not match VERSION.toml")
    if not cargo_language or cargo_language.group(1) != current_language:
        fail("Cargo language metadata does not match VERSION.toml")

    main = (root / "src" / "main.rs").read_text(encoding="utf-8")
    cli_language = re.search(r'^(?:pub\(crate\) )?const LANGUAGE_VERSION: &str = "([^"]+)";', main, re.MULTILINE)
    if not cli_language or cli_language.group(1) != current_language:
        fail("CLI language version does not match VERSION.toml")

    diagnostic = (root / "src" / "diagnostic.rs").read_text(encoding="utf-8")
    diagnostic_language = re.search(
        r'^pub const LANGUAGE_VERSION: &str = "([^"]+)";', diagnostic, re.MULTILINE
    )
    if not diagnostic_language or diagnostic_language.group(1) != current_language:
        fail("diagnostic/LSP language version does not match VERSION.toml")

    extension = json.loads((root / "tooling" / "vscode" / "package.json").read_text(encoding="utf-8"))
    if extension["version"] != extension_package:
        fail("VS Code package version does not match VERSION.toml")

    lock = (root / "Cargo.lock").read_text(encoding="utf-8")
    lock_block = lock.split('name = "seloryn"', 1)[1].split("\n\n", 1)[0]
    if f'version = "{runtime_package}"' not in lock_block:
        fail("Cargo.lock does not match VERSION.toml")

    npm_lock = json.loads((root / "tooling" / "vscode" / "package-lock.json").read_text(encoding="utf-8"))
    if npm_lock["version"] != extension_package or npm_lock["packages"][""]["version"] != extension_package:
        fail("package-lock.json does not match VERSION.toml")

    print(
        f"Seloryn version comparison: {previous_language} -> {current_language}; "
        f"runtime {previous_runtime} -> {runtime_package}; "
        f"extension {previous_extension} -> {extension_package}; "
        f"language_gain={language_ids[-1]}; extension_gain={extension_ids[-1]}"
    )


def main() -> None:
    try:
        validate()
    except (OSError, tomllib.TOMLDecodeError, json.JSONDecodeError, KeyError, IndexError) as error:
        fail(str(error))


if __name__ == "__main__":
    main()
