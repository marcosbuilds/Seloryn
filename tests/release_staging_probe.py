"""Black-box verification of the native, unpublished release staging contract."""

from __future__ import annotations

import hashlib
import importlib.util
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


def load_prepare_release():
    module_path = RELEASE_SCRIPTS / "prepare-release.py"
    spec = importlib.util.spec_from_file_location("seloryn_prepare_release", module_path)
    if spec is None or spec.loader is None:
        raise AssertionError(f"cannot load {module_path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def stage(
    binary: Path,
    extension: Path,
    output: Path,
    target: str,
    commit: str,
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [
            sys.executable,
            str(RELEASE_SCRIPTS / "prepare-release.py"),
            "--binary",
            str(binary),
            "--extension",
            str(extension),
            "--output-dir",
            str(output),
            "--platform",
            target,
            "--commit",
            commit,
        ],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
    )


def assert_rejected(result: subprocess.CompletedProcess[str], label: str, text: str) -> None:
    assert result.returncode != 0, f"{label} unexpectedly passed: {result.stdout} {result.stderr}"
    combined = f"{result.stdout}\n{result.stderr}"
    assert text.lower() in combined.lower(), f"{label} lost its diagnostic: {combined}"


def tamper_binary_entry(source: Path, destination: Path, target: str) -> None:
    packaged_name = binary_name(target)
    with zipfile.ZipFile(source) as original, zipfile.ZipFile(destination, "w") as tampered:
        for info in original.infolist():
            content = original.read(info.filename)
            if info.filename == packaged_name:
                content = content[:-1] + bytes([content[-1] ^ 1])
            tampered.writestr(info, content)


def assert_version_sources_are_derived(metadata: dict[str, str]) -> None:
    release_sources = [
        Path(__file__),
        RELEASE_SCRIPTS / "release_metadata.py",
        RELEASE_SCRIPTS / "package-runtime.py",
        RELEASE_SCRIPTS / "prepare-release.py",
    ]
    for path in release_sources:
        source = path.read_text(encoding="utf-8")
        for key in ("language", "runtime_package", "extension_package"):
            value = metadata[key]
            assert value not in source, f"{path} duplicated VERSION.toml {key}={value}"


def main() -> int:
    metadata = current_metadata(ROOT)
    try:
        host_target("Darwin", "x86_64")
    except SystemExit as error:
        assert "not declared" in str(error)
    else:
        raise AssertionError("macOS x64 was treated as a declared release target")
    target, expected_binary_name = host_target()
    binary = ROOT / "target" / "release" / expected_binary_name
    extension = ROOT / "packages" / "vscode" / f"seloryn-language-{metadata['extension_package']}.vsix"
    commit = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True
    ).strip()

    assert target in PLATFORMS
    assert binary.name == expected_binary_name
    assert binary.is_file(), f"release staging probe requires the release binary: {binary}"
    assert not binary.is_symlink(), f"release staging probe must not use a symlink: {binary}"
    assert extension.is_file(), f"release staging probe requires the VSIX: {extension}"
    assert_version_sources_are_derived(metadata)

    with tempfile.TemporaryDirectory(prefix="seloryn-release-probe-") as temporary:
        root = Path(temporary)
        output = root / "staged"
        result = stage(binary, extension, output, target, commit)
        if result.returncode != 0:
            raise SystemExit(f"staging failed: {result.stdout}\n{result.stderr}")

        manifest = json.loads((output / "release-manifest.json").read_text(encoding="utf-8"))
        assert manifest["schema"] == 1
        assert manifest["channel"] == metadata["channel"]
        assert manifest["language_version"] == metadata["language"]
        assert manifest["runtime_package"] == metadata["runtime_package"]
        assert manifest["extension_package"] == metadata["extension_package"]
        assert manifest["commit"] == commit
        assert manifest["abi"] == "unstable"
        assert manifest["platform"] == target
        assert manifest["provenance"]["provenance_schema"] == "seloryn.provenance.v1"
        assert manifest["provenance"]["commit"] == commit
        assert manifest["provenance"]["platform"] == target
        assert manifest["provenance"]["binary_name"] == expected_binary_name
        assert manifest["provenance"]["build_profile"] == "release"

        runtime_name = f"seloryn-runtime-{metadata['runtime_package']}-{target}.zip"
        extension_name = f"seloryn-language-{metadata['extension_package']}.vsix"
        assert {item["name"] for item in manifest["artifacts"]} == {
            runtime_name,
            extension_name,
        }
        checksums = {}
        for line in (output / "SHA256SUMS").read_text(encoding="utf-8").splitlines():
            checksum, name = line.split("  ", 1)
            checksums[name] = checksum
        assert set(checksums) == {runtime_name, extension_name}
        for item in manifest["artifacts"]:
            artifact = output / item["name"]
            assert artifact.is_file()
            assert item["sha256"] == digest(artifact) == checksums[item["name"]]
            assert item["bytes"] == artifact.stat().st_size
            if item["kind"] == "runtime":
                assert item["platform"] == target
            else:
                assert item["platform"] == "any"

        runtime = output / runtime_name
        with zipfile.ZipFile(runtime) as archive:
            assert archive.testzip() is None
            assert set(archive.namelist()) == {
                expected_binary_name,
                "LICENSE",
                "runtime-manifest.json",
            }
            runtime_manifest = json.loads(archive.read("runtime-manifest.json"))
            packaged_binary = archive.read(expected_binary_name)
        assert runtime_manifest["artifact"] == runtime_name
        assert runtime_manifest["platform"] == target
        assert runtime_manifest["channel"] == metadata["channel"]
        assert runtime_manifest["abi"] == "unstable"
        assert runtime_manifest["commit"] == commit
        assert runtime_manifest["language_version"] == metadata["language"]
        assert runtime_manifest["runtime_package"] == metadata["runtime_package"]
        assert runtime_manifest["binary_name"] == expected_binary_name
        assert runtime_manifest["binary_sha256"] == digest_bytes(packaged_binary)
        assert runtime_manifest["binary_bytes"] == len(packaged_binary)
        assert runtime_manifest["source_tree_sha256"]
        assert runtime_manifest["cargo_lock_sha256"]
        assert runtime_manifest["rust_toolchain_sha256"]

        with zipfile.ZipFile(output / extension_name) as archive:
            package = json.loads(archive.read("extension/package.json"))
        assert package["name"] == "seloryn-language"
        assert package["version"] == metadata["extension_package"]

        prepare_release = load_prepare_release()
        tampered = root / "tampered-runtime"
        tampered.mkdir()
        tampered_archive = tampered / runtime_name
        tamper_binary_entry(runtime, tampered_archive, target)
        try:
            prepare_release.runtime_provenance(
                tampered_archive,
                target,
                metadata["language"],
                metadata["runtime_package"],
                commit,
            )
        except SystemExit as error:
            assert "SHA-256" in str(error)
        else:
            raise AssertionError("tampered runtime binary was accepted")

        missing = ROOT / "target" / "release" / f"missing-{expected_binary_name}"
        assert_rejected(
            stage(missing, extension, root / "missing-output", target, commit),
            "missing release binary",
            "does not exist",
        )

        wrong_target = next(candidate for candidate in sorted(PLATFORMS) if candidate != target)
        assert_rejected(
            stage(binary, extension, root / "wrong-platform-output", wrong_target, commit),
            "platform-divergent release binary",
            "native build output",
        )

    print(f"release staging probe passed: platform={target} binary={binary}")
    print(f"runtime={runtime_name} language={metadata['language']} commit={commit}")
    print("negative=missing-binary,platform-divergence,tampered-binary-hash,version-duplication")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (AssertionError, OSError, subprocess.SubprocessError, UnicodeError, zipfile.BadZipFile) as error:
        print(f"release staging verification FAILED: {error!r}", file=sys.stderr)
        raise SystemExit(1) from error
