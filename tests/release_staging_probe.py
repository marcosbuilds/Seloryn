"""Black-box verification of the unpublished release staging contract."""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> int:
    binary = ROOT / "target" / "release" / "seloryn.exe"
    extension = ROOT / "tooling" / "vscode" / "seloryn-language-0.8.0.vsix"
    if not binary.is_file() or not extension.is_file():
        raise SystemExit("release staging probe requires the release binary and VSIX")
    with tempfile.TemporaryDirectory(prefix="seloryn-release-probe-") as temporary:
        output = Path(temporary)
        result = subprocess.run(
            [
                sys.executable,
                str(ROOT / "scripts" / "release" / "prepare-release.py"),
                "--binary",
                str(binary),
                "--extension",
                str(extension),
                "--output-dir",
                str(output),
                "--platform",
                "windows-x64",
                "--commit",
                "test-commit-0a29245",
            ],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
        if result.returncode != 0:
            raise SystemExit(f"staging failed: {result.stdout}\n{result.stderr}")
        manifest = json.loads((output / "release-manifest.json").read_text(encoding="utf-8"))
        assert manifest["schema"] == 1
        assert manifest["channel"] == "verified-preview"
        assert manifest["language_version"] == "0.08"
        assert manifest["runtime_package"] == "0.8.0"
        assert manifest["extension_package"] == "0.8.0"
        assert manifest["abi"] == "unstable"
        assert len(manifest["artifacts"]) == 2
        checksums = (output / "SHA256SUMS").read_text(encoding="utf-8").splitlines()
        assert len(checksums) == 2
        for item in manifest["artifacts"]:
            artifact = output / item["name"]
            assert artifact.is_file()
            assert item["sha256"] == digest(artifact)
            assert item["bytes"] == artifact.stat().st_size
        runtime = output / "seloryn-runtime-0.8.0-windows-x64.zip"
        with zipfile.ZipFile(runtime) as archive:
            runtime_manifest = json.loads(archive.read("runtime-manifest.json"))
            assert runtime_manifest["language_version"] == "0.08"
            assert runtime_manifest["runtime_package"] == "0.8.0"
            assert runtime_manifest["channel"] == "verified-preview"
        with zipfile.ZipFile(output / "seloryn-language-0.8.0.vsix") as archive:
            package = json.loads(archive.read("extension/package.json"))
            assert package["version"] == "0.8.0"
    print("release staging probe passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
