"""Black-box verification of the public runtime release contract."""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RELEASE_SCRIPTS = ROOT / "scripts" / "release"
sys.path.insert(0, str(RELEASE_SCRIPTS))

from release_metadata import (  # noqa: E402
    PLATFORMS,
    binary_name,
    current_metadata,
    host_target,
)


def digest_bytes(content: bytes) -> str:
    return hashlib.sha256(content).hexdigest()


def digest(path: Path) -> str:
    return digest_bytes(path.read_bytes())


def package(binary: Path, output: Path, platform: str, commit: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [
            sys.executable,
            str(RELEASE_SCRIPTS / "package-runtime.py"),
            "--binary",
            str(binary),
            "--output-dir",
            str(output),
            "--platform",
            platform,
            "--commit",
            commit,
        ],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
    )


def tamper_archive(source: Path, destination: Path, binary_name_in_archive: str) -> None:
    with zipfile.ZipFile(source) as original, zipfile.ZipFile(destination, "w") as tampered:
        for info in original.infolist():
            content = original.read(info.filename)
            if info.filename == binary_name_in_archive:
                content = content[:-1] + bytes([content[-1] ^ 1])
            tampered.writestr(info, content)


def main() -> int:
    metadata = current_metadata(ROOT)
    target, expected_binary_name = host_target()
    assert target in PLATFORMS
    binary = ROOT / "target" / "release" / expected_binary_name
    assert binary.is_file(), f"release binary is missing: {binary}"
    assert not binary.is_symlink(), f"release binary must not be a symlink: {binary}"
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()

    with tempfile.TemporaryDirectory(prefix="seloryn-runtime-release-probe-") as temporary:
        output = Path(temporary)
        result = package(binary, output, target, commit)
        assert result.returncode == 0, result.stderr

        archive_path = output / f"seloryn-runtime-{metadata['runtime_package']}-{target}.zip"
        assert archive_path.is_file()
        with zipfile.ZipFile(archive_path) as archive:
            assert archive.testzip() is None
            assert set(archive.namelist()) == {expected_binary_name, "LICENSE", "runtime-manifest.json"}
            document = json.loads(archive.read("runtime-manifest.json"))
            packaged_binary = archive.read(expected_binary_name)
        assert document["artifact"] == archive_path.name
        assert document["abi"] == "unstable"
        assert document["channel"] == metadata["channel"]
        assert document["commit"] == commit
        assert document["language_version"] == metadata["language"]
        assert document["platform"] == target
        assert document["binary_name"] == expected_binary_name
        assert document["binary_sha256"] == digest_bytes(packaged_binary)
        assert document["binary_bytes"] == len(packaged_binary)
        assert document["source_tree_sha256"]

        assert len(digest(archive_path)) == 64

        tampered = output / "tampered-runtime.zip"
        tamper_archive(archive_path, tampered, expected_binary_name)
        with zipfile.ZipFile(tampered) as archive:
            altered = json.loads(archive.read("runtime-manifest.json"))
            altered_binary = archive.read(expected_binary_name)
        assert altered["binary_sha256"] != digest_bytes(altered_binary)

        missing = output / f"missing-{expected_binary_name}"
        rejected = package(missing, output / "missing-output", target, commit)
        assert rejected.returncode != 0
        assert "does not exist" in (rejected.stdout + rejected.stderr).lower()

        wrong_target = next(candidate for candidate in sorted(PLATFORMS) if candidate != target)
        wrong_platform = package(binary, output / "wrong-platform", wrong_target, commit)
        assert wrong_platform.returncode != 0
        assert wrong_target.split("-", maxsplit=1)[0] in (
            wrong_platform.stdout + wrong_platform.stderr
        ).lower()

    print(f"runtime release staging passed: platform={target} binary={expected_binary_name}")
    print("negative=missing-binary, platform-mismatch, tampered-binary-hash")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
