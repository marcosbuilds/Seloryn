"""Shared release metadata and native target validation."""

from __future__ import annotations

import hashlib
import platform as host_platform
import re
import sys
from pathlib import Path
from typing import Any

try:
    import tomllib
except ModuleNotFoundError:  # Python 3.10 compatibility.
    import tomli as tomllib


ROOT = Path(__file__).resolve().parents[2]
PLATFORMS = {"windows-x64", "linux-x64", "macos-arm64"}
LANGUAGE_PATTERN = re.compile(r"(?:0|[1-9][0-9]*)\.[0-9]{2}\Z")
PACKAGE_PATTERN = re.compile(
    r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\Z"
)
PREVIEW_CHANNEL = "verified-preview"
UNSTABLE_ABI = "unstable"


def current_metadata(root: Path = ROOT) -> dict[str, str]:
    """Read and validate the current release line from VERSION.toml."""

    version_path = root / "VERSION.toml"
    try:
        document = tomllib.loads(version_path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        raise SystemExit(f"cannot read {version_path}: {error}") from error
    current = document.get("current")
    if not isinstance(current, dict):
        raise SystemExit("VERSION.toml is missing [current]")

    values: dict[str, str] = {}
    for key in ("language", "runtime_package", "extension_package", "channel"):
        value = current.get(key)
        if not isinstance(value, str) or not value:
            raise SystemExit(f"VERSION.toml current.{key} is missing")
        values[key] = value
    if not LANGUAGE_PATTERN.fullmatch(values["language"]):
        raise SystemExit(f"current language version is not canonical: {values['language']}")
    for key in ("runtime_package", "extension_package"):
        if not PACKAGE_PATTERN.fullmatch(values[key]):
            raise SystemExit(f"current {key} is not canonical: {values[key]}")
        if tuple(int(part) for part in values[key].split(".")[:2]) != tuple(
            int(part) for part in values["language"].split(".")
        ):
            raise SystemExit(f"current {key} is not on the language compatibility line")
    if values["channel"] != PREVIEW_CHANNEL:
        raise SystemExit(
            f"release staging only supports {PREVIEW_CHANNEL}; got {values['channel']}"
        )
    return values


def binary_name(target: str) -> str:
    if target not in PLATFORMS:
        raise SystemExit(f"unsupported release platform: {target}")
    return "seloryn.exe" if target == "windows-x64" else "seloryn"


def host_target(
    system: str | None = None, machine: str | None = None
) -> tuple[str, str]:
    """Return the declared native target and binary name for this host."""

    system = system or platform_system()
    machine = (machine or host_platform.machine()).lower()
    if system == "Windows":
        if machine not in {"amd64", "x86_64"}:
            raise SystemExit(f"Windows host architecture is not declared: {machine}")
        return "windows-x64", binary_name("windows-x64")
    if system == "Linux":
        if machine not in {"amd64", "x86_64"}:
            raise SystemExit(f"Linux host architecture is not declared: {machine}")
        return "linux-x64", binary_name("linux-x64")
    if system == "Darwin":
        if machine not in {"arm64", "aarch64"}:
            raise SystemExit(
                f"macOS host architecture is not declared: {machine}; macos-arm64 is the only target"
            )
        return "macos-arm64", binary_name("macos-arm64")
    raise SystemExit(f"host operating system is not declared: {system}")


def platform_system() -> str:
    if sys.platform == "win32":
        return "Windows"
    if sys.platform == "darwin":
        return "Darwin"
    if sys.platform.startswith("linux"):
        return "Linux"
    return host_platform.system()


def _read_prefix(path: Path, size: int = 4096) -> bytes:
    with path.open("rb") as stream:
        return stream.read(size)


def validate_binary_format(path: Path, target: str) -> None:
    """Reject a correctly named file whose native format/architecture is wrong."""

    data = _read_prefix(path)
    if target == "windows-x64":
        if len(data) < 64 or data[:2] != b"MZ":
            raise SystemExit(f"release binary is not a Windows PE image: {path}")
        pe_offset = int.from_bytes(data[0x3C:0x40], "little")
        if len(data) < pe_offset + 6 or data[pe_offset : pe_offset + 4] != b"PE\0\0":
            raise SystemExit(f"release binary has no valid PE header: {path}")
        machine = int.from_bytes(data[pe_offset + 4 : pe_offset + 6], "little")
        if machine != 0x8664:
            raise SystemExit(f"release binary is not Windows x64 (machine 0x{machine:04x})")
        return
    if target == "linux-x64":
        if len(data) < 20 or data[:4] != b"\x7fELF" or data[4] != 2:
            raise SystemExit(f"release binary is not a Linux ELF64 image: {path}")
        machine = int.from_bytes(data[18:20], "little")
        if machine != 0x3E:
            raise SystemExit(f"release binary is not Linux x64 (machine 0x{machine:04x})")
        return
    if target == "macos-arm64":
        if len(data) < 8 or data[:4] not in {b"\xcf\xfa\xed\xfe", b"\xfe\xed\xfa\xcf"}:
            raise SystemExit(f"release binary is not a 64-bit macOS Mach-O image: {path}")
        little_endian = data[:4] == b"\xcf\xfa\xed\xfe"
        machine = int.from_bytes(data[4:8], "little" if little_endian else "big")
        if machine != 0x0100000C:
            raise SystemExit(f"release binary is not macOS arm64 (machine 0x{machine:08x})")
        return
    raise SystemExit(f"unsupported release platform: {target}")


def validate_release_binary(path: Path, target: str, root: Path = ROOT) -> Path:
    """Require the native Cargo release output, never a symlink or substitute."""

    expected_name = binary_name(target)
    if path.is_symlink():
        raise SystemExit(f"release binary must not be a symlink: {path}")
    if not path.is_file():
        raise SystemExit(f"runtime binary does not exist: {path}")
    if path.name != expected_name:
        raise SystemExit(
            f"release binary for {target} must be the native build output "
            f"{root / 'target' / 'release' / expected_name}"
        )
    expected_path = root / "target" / "release" / expected_name
    try:
        resolved_path = path.resolve(strict=True)
        resolved_expected = expected_path.resolve(strict=True)
    except OSError as error:
        raise SystemExit(f"cannot resolve release binary: {error}") from error
    if resolved_path != resolved_expected or path.name != expected_name:
        raise SystemExit(
            f"release binary for {target} must be the native build output {expected_path}"
        )
    validate_binary_format(path, target)
    return path


def sha256_bytes(content: bytes) -> str:
    return hashlib.sha256(content).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def ensure_text(document: dict[str, Any], key: str, label: str) -> str:
    value = document.get(key)
    if not isinstance(value, str) or not value:
        raise SystemExit(f"{label} is missing `{key}`")
    return value
