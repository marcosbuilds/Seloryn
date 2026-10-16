"""Stage one Seloryn runtime/editor release without publishing it."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # Python 3.10 and older developer hosts.
    import tomli as tomllib


ROOT = Path(__file__).resolve().parents[2]
PACKAGE_PATTERN = re.compile(r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\Z")
LANGUAGE_PATTERN = re.compile(r"(?:0|[1-9][0-9]*)\.[0-9]{2}\Z")
PLATFORMS = {"windows-x64", "linux-x64", "macos-arm64"}


def arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--extension", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--platform", choices=sorted(PLATFORMS), required=True)
    parser.add_argument("--commit", required=True)
    return parser.parse_args()


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def runtime_provenance(path: Path) -> dict[str, str]:
    with zipfile.ZipFile(path) as archive:
        try:
            document = json.loads(archive.read("runtime-manifest.json"))
        except (KeyError, json.JSONDecodeError) as error:
            raise SystemExit(f"runtime archive has invalid provenance metadata: {error}") from error
    required = [
        "abi",
        "binary_sha256",
        "build_profile",
        "cargo_lock_sha256",
        "commit",
        "provenance_schema",
        "rust_toolchain_sha256",
        "source_tree_sha256",
    ]
    for key in required:
        if not isinstance(document.get(key), str) or not document[key]:
            raise SystemExit(f"runtime provenance is missing `{key}`")
    if document["provenance_schema"] != "seloryn.provenance.v1":
        raise SystemExit("runtime provenance schema is unsupported")
    if document["build_profile"] != "release":
        raise SystemExit("runtime provenance must describe a release build")
    return {key: document[key] for key in required}


def require_version(value: object, pattern: re.Pattern[str], label: str) -> str:
    if not isinstance(value, str) or not pattern.fullmatch(value):
        raise SystemExit(f"{label} has an invalid canonical value: {value!r}")
    return value


def current_versions() -> tuple[str, str, str]:
    document = tomllib.loads((ROOT / "VERSION.toml").read_text(encoding="utf-8"))
    current = document.get("current")
    if not isinstance(current, dict):
        raise SystemExit("VERSION.toml is missing [current]")
    language = require_version(current.get("language"), LANGUAGE_PATTERN, "language version")
    runtime = require_version(current.get("runtime_package"), PACKAGE_PATTERN, "runtime package")
    extension = require_version(
        current.get("extension_package"), PACKAGE_PATTERN, "extension package"
    )
    language_base = tuple(int(part) for part in language.split("."))
    for label, package in (("runtime", runtime), ("extension", extension)):
        package_base = tuple(int(part) for part in package.split(".")[:2])
        if package_base != language_base:
            raise SystemExit(f"{label} package is not on language compatibility line {language}")
    return language, runtime, extension


def verify_versions() -> None:
    subprocess.run(
        [sys.executable, str(ROOT / "scripts" / "release" / "verify-version.py")],
        cwd=ROOT,
        check=True,
    )


def package_runtime(
    binary: Path,
    output_dir: Path,
    platform: str,
    runtime: str,
    language: str,
    commit: str,
) -> Path:
    archive = output_dir / f"seloryn-runtime-{runtime}-{platform}.zip"
    subprocess.run(
        [
            sys.executable,
            str(ROOT / "scripts" / "release" / "package-runtime.py"),
            "--binary",
            str(binary),
            "--output-dir",
            str(output_dir),
            "--platform",
            platform,
            "--runtime-version",
            runtime,
            "--language-version",
            language,
            "--commit",
            commit,
        ],
        cwd=ROOT,
        check=True,
    )
    if not archive.is_file():
        raise SystemExit(f"runtime packager did not create {archive}")
    return archive


def stage() -> int:
    options = arguments()
    verify_versions()
    language, runtime, extension_version = current_versions()
    binary = options.binary.resolve()
    extension = options.extension.resolve()
    if not binary.is_file():
        raise SystemExit(f"runtime binary does not exist: {binary}")
    if not extension.is_file():
        raise SystemExit(f"VSIX does not exist: {extension}")
    if not options.commit or any(character.isspace() for character in options.commit):
        raise SystemExit("commit must be a non-empty SHA/ref without whitespace")

    output_dir = options.output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="seloryn-release-") as temporary:
        runtime_archive = package_runtime(
            binary,
            Path(temporary),
            options.platform,
            runtime,
            language,
            options.commit,
        )
        staged_runtime = output_dir / runtime_archive.name
        shutil.copyfile(runtime_archive, staged_runtime)

    staged_extension = output_dir / f"seloryn-language-{extension_version}.vsix"
    shutil.copyfile(extension, staged_extension)
    provenance = runtime_provenance(staged_runtime)
    artifacts = [
        {
            "kind": "runtime",
            "name": staged_runtime.name,
            "platform": options.platform,
            "sha256": sha256(staged_runtime),
            "bytes": staged_runtime.stat().st_size,
        },
        {
            "kind": "vscode-extension",
            "name": staged_extension.name,
            "platform": "any",
            "sha256": sha256(staged_extension),
            "bytes": staged_extension.stat().st_size,
        },
    ]
    manifest = {
        "schema": 1,
        "channel": "verified-preview",
        "language_version": language,
        "runtime_package": runtime,
        "extension_package": extension_version,
        "commit": options.commit,
        "abi": "unstable",
        "artifacts": artifacts,
        "provenance": provenance,
    }
    manifest_path = output_dir / "release-manifest.json"
    manifest_path.write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    checksums = "".join(f"{item['sha256']}  {item['name']}\n" for item in artifacts)
    (output_dir / "SHA256SUMS").write_text(checksums, encoding="utf-8")
    provenance_document = {
        "schema": "seloryn.provenance.v1",
        "language_version": language,
        "runtime_package": runtime,
        "extension_package": extension_version,
        "commit": options.commit,
        "abi": "unstable",
        "artifacts": {item["name"]: item["sha256"] for item in artifacts},
        "runtime": provenance,
    }
    (output_dir / "PROVENANCE.json").write_text(
        json.dumps(provenance_document, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(f"staged Seloryn {language}: {output_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(stage())
