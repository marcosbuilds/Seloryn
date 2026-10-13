"""Cross-platform black-box checks for public projects and bundles."""

from __future__ import annotations

import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target" / "release" / ("seloryn.exe" if sys.platform == "win32" else "seloryn")


def run(*arguments: str) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(
        [str(BINARY), *arguments],
        cwd=ROOT,
        text=True,
        encoding="utf-8",
        errors="replace",
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        raise AssertionError(
            f"command failed ({result.returncode}): {arguments}\n"
            f"stdout={result.stdout}\nstderr={result.stderr}"
        )
    return result


def main() -> int:
    if not BINARY.is_file():
        raise AssertionError(f"release binary is missing: {BINARY}")

    json_config = run("run", "examples/projects/json-config")
    assert json_config.stdout == "configured model: gpt-local\n"

    training = run("run", "examples/projects/linear-training")
    assert '"loss_decreased":true' in training.stdout

    checked = run("check", "examples/projects/evaluation-pipeline")
    assert "checked project" in checked.stdout
    pipeline = run("run", "examples/projects/evaluation-pipeline")
    assert "regional-orders" in pipeline.stdout
    assert "Tensor[F32" in pipeline.stdout

    project = ROOT / "examples" / "projects" / "evaluation-pipeline"
    with tempfile.TemporaryDirectory(prefix="seloryn-release-") as temporary:
        run("build", str(project), "--profile", "release", "--output", temporary)
        artifact = Path(temporary) / "seloryn.selyapp"
        assert artifact.is_file(), artifact
        bundled = run("exec", str(artifact))
        assert "regional-orders" in bundled.stdout
        assert "Tensor[F32" in bundled.stdout

    print("Seloryn public project and bundle checks completed successfully")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (AssertionError, OSError) as error:
        print(f"release verification FAILED: {error!r}", file=sys.stderr)
        raise SystemExit(1) from error
