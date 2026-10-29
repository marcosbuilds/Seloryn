"""Cross-platform black-box checks for public projects and bundles."""

from __future__ import annotations

import subprocess
import sys
import tempfile
import json
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


def assert_pipeline_output(result: subprocess.CompletedProcess[str]) -> None:
    output = result.stdout
    assert 'pipeline: "regional-orders"' in output
    assert "accepted: 1" in output
    assert "rejected: 2" in output
    assert "total_amount: 120.0" in output
    assert 'labels: ["A-100:accepted", "B-200:below minimum", "C-300:order disabled"]' in output
    assert "Tensor[F32" in output


def main() -> int:
    if not BINARY.is_file():
        raise AssertionError(f"release binary is missing: {BINARY}")

    json_config = run("run", "examples/projects/json-config")
    assert json_config.stdout == "configured model: gpt-local\n"

    training = run("run", "examples/projects/linear-training")
    assert '"loss_decreased":true' in training.stdout

    checkpoint = run("run", "examples/projects/checkpoint-validation")
    assert '"fixture":"schema-valid"' in checkpoint.stdout

    indexed = run("run", "examples/tensors/tensor-index.sely")
    assert indexed.stdout == "3.0\n"

    checked = run("check", "examples/projects/evaluation-pipeline")
    assert "checked project" in checked.stdout
    pipeline = run("run", "examples/projects/evaluation-pipeline")
    assert_pipeline_output(pipeline)

    project = ROOT / "examples" / "projects" / "evaluation-pipeline"
    with tempfile.TemporaryDirectory(prefix="seloryn-release-") as temporary:
        run("build", str(project), "--profile", "release", "--output", temporary)
        artifact = Path(temporary) / "seloryn.selyapp"
        metadata = Path(temporary) / "seloryn-build.json"
        assert artifact.is_file(), artifact
        assert metadata.is_file(), metadata
        metadata_text = metadata.read_text(encoding="utf-8")
        assert '"schema":"seloryn.build.v1"' in metadata_text
        assert '"bundle_format":"seloryn.bundle.v1"' in metadata_text
        metadata_payload = json.loads(metadata_text)
        assert metadata_payload["runtime_package"] == "0.12.0", metadata_payload
        assert metadata_payload["profile"] == "release", metadata_payload
        assert metadata_payload["artifact"] == "seloryn.selyapp", metadata_payload
        assert metadata_payload["integrity"] == "sha256", metadata_payload
        assert metadata_payload["authenticity"] == "none", metadata_payload
        assert metadata_payload["deterministic"] is True, metadata_payload
        bundled = run("exec", str(temporary))
        assert_pipeline_output(bundled)

        def reject_metadata_mutation(mutate, expected):
            mutated = json.loads(json.dumps(metadata_payload))
            mutate(mutated)
            metadata.write_text(json.dumps(mutated), encoding="utf-8")
            rejected = subprocess.run(
                [str(BINARY), "exec", str(temporary)],
                cwd=ROOT,
                text=True,
                encoding="utf-8",
                errors="replace",
                capture_output=True,
                check=False,
            )
            assert rejected.returncode == 65, (rejected.stdout, rejected.stderr)
            assert expected in rejected.stderr, (expected, rejected.stderr)
            metadata.write_text(metadata_text, encoding="utf-8")

        reject_metadata_mutation(
            lambda value: value.update({"runtime_package": "0.0.0"}),
            "runtime version is incompatible",
        )
        reject_metadata_mutation(
            lambda value: value.update({"profile": "unsupported"}),
            "profile is unsupported",
        )
        reject_metadata_mutation(
            lambda value: value.update({"artifact": "other.selyapp"}),
            "artifact is unsupported",
        )
        reject_metadata_mutation(
            lambda value: value.update({"integrity": "sha512"}),
            "integrity algorithm is unsupported",
        )
        reject_metadata_mutation(
            lambda value: value.update({"authenticity": "signed"}),
            "does not authenticate authorship",
        )
        reject_metadata_mutation(
            lambda value: value.update({"deterministic": False}),
            "deterministic flag is invalid",
        )
        reject_metadata_mutation(
            lambda value: value.update({"unexpected": True}),
            "contains unsupported field `unexpected`",
        )

        metadata.write_text(metadata_text.replace('"sha256":"', '"sha256":"0', 1), encoding="utf-8")
        tampered = subprocess.run(
            [str(BINARY), "exec", str(temporary)],
            cwd=ROOT,
            text=True,
            encoding="utf-8",
            errors="replace",
            capture_output=True,
            check=False,
        )
        assert tampered.returncode == 65
        assert "SHA-256 does not match" in tampered.stderr

    print("Seloryn public project and bundle checks completed successfully")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (AssertionError, OSError) as error:
        print(f"release verification FAILED: {error!r}", file=sys.stderr)
        raise SystemExit(1) from error
