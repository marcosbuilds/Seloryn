"""Create a deterministic preview runtime archive with provenance metadata."""

from __future__ import annotations

import argparse
import json
import subprocess
import zipfile
from pathlib import Path

from release_metadata import (
    PLATFORMS,
    UNSTABLE_ABI,
    binary_name,
    current_metadata,
    sha256_bytes,
    sha256_file,
    validate_release_binary,
)


ROOT = Path(__file__).resolve().parents[2]


def arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--platform", choices=sorted(PLATFORMS), required=True)
    parser.add_argument("--runtime-version")
    parser.add_argument("--language-version")
    parser.add_argument("--commit", required=True)
    return parser.parse_args()


def deterministic_entry(name: str, content: bytes) -> zipfile.ZipInfo:
    entry = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
    entry.compress_type = zipfile.ZIP_DEFLATED
    entry.external_attr = 0o100644 << 16
    entry.create_system = 3
    entry.file_size = len(content)
    return entry


def source_tree_sha256(commit: str) -> str:
    result = subprocess.run(
        ["git", "-C", str(ROOT), "archive", "--format=tar", commit],
        capture_output=True,
        check=True,
    )
    return sha256_bytes(result.stdout)


def main() -> int:
    options = arguments()
    metadata = current_metadata(ROOT)
    if options.runtime_version and options.runtime_version != metadata["runtime_package"]:
        raise SystemExit("runtime package must be read from VERSION.toml")
    if options.language_version and options.language_version != metadata["language"]:
        raise SystemExit("language version must be read from VERSION.toml")
    if not options.commit or any(character.isspace() for character in options.commit):
        raise SystemExit("commit must be a non-empty SHA/ref without whitespace")
    binary = validate_release_binary(options.binary, options.platform, ROOT)
    license_path = ROOT / "LICENSE"
    if not license_path.is_file():
        raise SystemExit(f"license does not exist: {license_path}")

    options.output_dir.mkdir(parents=True, exist_ok=True)
    archive_name = f"seloryn-runtime-{metadata['runtime_package']}-{options.platform}.zip"
    archive_path = options.output_dir / archive_name
    packaged_binary_name = binary_name(options.platform)
    binary_bytes = binary.read_bytes()
    binary_sha256 = sha256_bytes(binary_bytes)
    cargo_lock_sha256 = sha256_file(ROOT / "Cargo.lock")
    toolchain_sha256 = sha256_file(ROOT / "rust-toolchain.toml")
    manifest = {
        "artifact": archive_name,
        "abi": UNSTABLE_ABI,
        "binary_sha256": binary_sha256,
        "binary_bytes": len(binary_bytes),
        "binary_name": packaged_binary_name,
        "build_profile": "release",
        "cargo_lock_sha256": cargo_lock_sha256,
        "channel": metadata["channel"],
        "commit": options.commit,
        "gpu_tensor_training": False,
        "language_version": metadata["language"],
        "platform": options.platform,
        "provenance_schema": "seloryn.provenance.v1",
        "runtime_package": metadata["runtime_package"],
        "rust_toolchain_sha256": toolchain_sha256,
        "source_tree_sha256": source_tree_sha256(options.commit),
    }
    manifest_bytes = (json.dumps(manifest, indent=2, sort_keys=True) + "\n").encode("utf-8")
    with zipfile.ZipFile(archive_path, "w") as archive:
        for name, content in (
            (packaged_binary_name, binary_bytes),
            ("LICENSE", license_path.read_bytes()),
            ("runtime-manifest.json", manifest_bytes),
        ):
            archive.writestr(deterministic_entry(name, content), content)
    print(f"packaged {archive_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
