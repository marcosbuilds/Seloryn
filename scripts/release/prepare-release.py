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

from release_metadata import (
    PLATFORMS,
    PREVIEW_CHANNEL,
    UNSTABLE_ABI,
    binary_name,
    current_metadata,
    ensure_text,
    sha256_bytes,
    validate_release_binary,
)


ROOT = Path(__file__).resolve().parents[2]


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


def runtime_provenance(
    path: Path, platform: str, language: str, runtime: str, commit: str
) -> dict[str, object]:
    try:
        archive = zipfile.ZipFile(path)
    except (OSError, zipfile.BadZipFile) as error:
        raise SystemExit(f"runtime archive is invalid: {error}") from error
    with archive:
        try:
            if archive.testzip() is not None:
                raise SystemExit("runtime archive contains a corrupt entry")
            expected_names = {binary_name(platform), "LICENSE", "runtime-manifest.json"}
            names = archive.namelist()
            if len(names) != len(expected_names) or set(names) != expected_names:
                raise SystemExit("runtime archive has an unexpected file set")
            document = json.loads(archive.read("runtime-manifest.json"))
        except (KeyError, json.JSONDecodeError, UnicodeDecodeError) as error:
            raise SystemExit(f"runtime archive has invalid provenance metadata: {error}") from error
        if not isinstance(document, dict):
            raise SystemExit("runtime provenance must be a JSON object")
        required = [
            "abi",
            "artifact",
            "binary_sha256",
            "binary_name",
            "build_profile",
            "cargo_lock_sha256",
            "channel",
            "commit",
            "language_version",
            "platform",
            "provenance_schema",
            "runtime_package",
            "rust_toolchain_sha256",
            "source_tree_sha256",
        ]
        for key in required:
            ensure_text(document, key, "runtime provenance")
        binary = archive.read(binary_name(platform))
    if document["provenance_schema"] != "seloryn.provenance.v1":
        raise SystemExit("runtime provenance schema is unsupported")
    if document["artifact"] != path.name:
        raise SystemExit("runtime provenance artifact name does not match the archive")
    if document["abi"] != UNSTABLE_ABI:
        raise SystemExit("runtime provenance ABI is not unstable")
    if document["build_profile"] != "release":
        raise SystemExit("runtime provenance must describe a release build")
    if document["channel"] != PREVIEW_CHANNEL:
        raise SystemExit(f"runtime provenance is not {PREVIEW_CHANNEL}")
    if document["commit"] != commit:
        raise SystemExit("runtime provenance commit does not match staging commit")
    if document["language_version"] != language:
        raise SystemExit("runtime provenance language version does not match VERSION.toml")
    if document["runtime_package"] != runtime:
        raise SystemExit("runtime provenance runtime package does not match VERSION.toml")
    if document["platform"] != platform:
        raise SystemExit("runtime provenance platform does not match staging platform")
    if document["binary_name"] != binary_name(platform):
        raise SystemExit("runtime provenance binary name does not match staging platform")
    if not isinstance(document.get("binary_bytes"), int) or document["binary_bytes"] != len(binary):
        raise SystemExit("runtime provenance binary byte count does not match the archive")
    if document["binary_sha256"] != sha256_bytes(binary):
        raise SystemExit("runtime provenance binary SHA-256 does not match the archive")
    for key in ("binary_sha256", "cargo_lock_sha256", "rust_toolchain_sha256", "source_tree_sha256"):
        if not isinstance(document[key], str) or not re.fullmatch(r"[0-9a-f]{64}", document[key]):
            raise SystemExit(f"runtime provenance `{key}` is not a lowercase SHA-256")
    return document


def extension_package_version(path: Path) -> str:
    try:
        with zipfile.ZipFile(path) as archive:
            package = json.loads(archive.read("extension/package.json"))
    except (OSError, KeyError, json.JSONDecodeError, UnicodeDecodeError, zipfile.BadZipFile) as error:
        raise SystemExit(f"VSIX has invalid extension/package.json: {error}") from error
    if not isinstance(package, dict) or package.get("name") != "seloryn-language":
        raise SystemExit("VSIX package name is not seloryn-language")
    version = package.get("version")
    if not isinstance(version, str) or not version:
        raise SystemExit("VSIX package version is missing")
    return version


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
    commit: str,
) -> Path:
    runtime = current_metadata(ROOT)["runtime_package"]
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
    metadata = current_metadata(ROOT)
    language = metadata["language"]
    runtime = metadata["runtime_package"]
    extension_version = metadata["extension_package"]
    binary = validate_release_binary(options.binary, options.platform, ROOT)
    extension = options.extension.resolve()
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
            options.commit,
        )
        staged_runtime = output_dir / runtime_archive.name
        shutil.copyfile(runtime_archive, staged_runtime)

    staged_extension = output_dir / f"seloryn-language-{extension_version}.vsix"
    shutil.copyfile(extension, staged_extension)
    provenance = runtime_provenance(
        staged_runtime, options.platform, language, runtime, options.commit
    )
    if extension_package_version(staged_extension) != extension_version:
        raise SystemExit("VSIX package version does not match VERSION.toml")
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
        "channel": metadata["channel"],
        "language_version": language,
        "runtime_package": runtime,
        "extension_package": extension_version,
        "commit": options.commit,
        "abi": UNSTABLE_ABI,
        "platform": options.platform,
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
        "abi": UNSTABLE_ABI,
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
