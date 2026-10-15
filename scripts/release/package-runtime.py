"""Create a deterministic preview runtime archive with provenance metadata."""

from __future__ import annotations

import argparse
import json
import re
import zipfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
PLATFORMS = {"windows-x64", "linux-x64", "macos-arm64"}
PACKAGE_PATTERN = re.compile(r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\Z")
LANGUAGE_PATTERN = re.compile(r"(?:0|[1-9][0-9]*)\.[0-9]{2}\Z")


def arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--platform", choices=sorted(PLATFORMS), required=True)
    parser.add_argument("--runtime-version", required=True)
    parser.add_argument("--language-version", required=True)
    parser.add_argument("--commit", required=True)
    return parser.parse_args()


def validate_version(value: str, pattern: re.Pattern[str], label: str) -> None:
    if not pattern.fullmatch(value):
        raise SystemExit(f"{label} has an invalid canonical value: {value}")


def deterministic_entry(name: str, content: bytes) -> zipfile.ZipInfo:
    entry = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
    entry.compress_type = zipfile.ZIP_DEFLATED
    entry.external_attr = 0o100644 << 16
    entry.create_system = 3
    entry.file_size = len(content)
    return entry


def main() -> int:
    options = arguments()
    validate_version(options.runtime_version, PACKAGE_PATTERN, "runtime package")
    validate_version(options.language_version, LANGUAGE_PATTERN, "language")
    if not options.commit or any(character.isspace() for character in options.commit):
        raise SystemExit("commit must be a non-empty SHA/ref without whitespace")
    binary = options.binary.resolve()
    if not binary.is_file():
        raise SystemExit(f"runtime binary does not exist: {binary}")
    license_path = ROOT / "LICENSE"
    if not license_path.is_file():
        raise SystemExit(f"license does not exist: {license_path}")

    options.output_dir.mkdir(parents=True, exist_ok=True)
    archive_name = f"seloryn-runtime-{options.runtime_version}-{options.platform}.zip"
    archive_path = options.output_dir / archive_name
    binary_name = "seloryn.exe" if options.platform == "windows-x64" else "seloryn"
    manifest = {
        "artifact": archive_name,
        "channel": "verified-preview",
        "commit": options.commit,
        "gpu_tensor_training": False,
        "language_version": options.language_version,
        "platform": options.platform,
        "runtime_package": options.runtime_version,
    }
    manifest_bytes = (json.dumps(manifest, indent=2, sort_keys=True) + "\n").encode("utf-8")
    with zipfile.ZipFile(archive_path, "w") as archive:
        for name, content in (
            (binary_name, binary.read_bytes()),
            ("LICENSE", license_path.read_bytes()),
            ("runtime-manifest.json", manifest_bytes),
        ):
            archive.writestr(deterministic_entry(name, content), content)
    print(f"packaged {archive_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
