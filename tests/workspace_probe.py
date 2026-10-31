"""Black-box evidence for the read-only doctor and transactional init commands."""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target" / "release" / ("seloryn.exe" if sys.platform == "win32" else "seloryn")


def run(*arguments: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [str(BINARY), *arguments],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )


def main() -> int:
    assert BINARY.is_file(), f"missing release binary: {BINARY}"
    with tempfile.TemporaryDirectory(prefix="seloryn-workspace-probe-") as temporary:
        root = Path(temporary)
        minimal = root / "minimal-app"
        initialized = run("init", str(minimal), "--template=minimal", "--format=json")
        assert initialized.returncode == 0, initialized.stderr
        init_result = json.loads(initialized.stdout)
        assert init_result["schema"] == "seloryn.init.v1"
        assert init_result["template"] == "minimal"
        assert (minimal / "seloryn.toml").is_file()
        assert (minimal / "src" / "main.sely").is_file()

        doctor = run("doctor", str(minimal), "--format=json")
        assert doctor.returncode == 0, doctor.stderr
        doctor_result = json.loads(doctor.stdout)
        assert doctor_result["schema"] == "seloryn.doctor.v1"
        assert doctor_result["status"] == "warning"
        assert doctor_result["production_ready"] is False
        assert any(issue["code"] == "SEL-D001" for issue in doctor_result["issues"])

        checked = run("check", str(minimal), "--quiet")
        assert checked.returncode == 0, checked.stderr
        executed = run("run", str(minimal))
        assert executed.returncode == 0, executed.stderr
        assert executed.stdout == '"hello from Seloryn"\n'

        refused = run("init", str(minimal), "--format=json")
        assert refused.returncode == 65
        refusal = json.loads(refused.stderr)
        assert refusal["schema"] == "seloryn.init.v1"
        assert refusal["code"] == "SEL-I001"

        tabular = root / "tabular-mlp"
        tabular_init = run("init", str(tabular), "--template=tabular-mlp", "--format=json")
        assert tabular_init.returncode == 0, tabular_init.stderr
        assert (tabular / "data" / "train.json").is_file()
        assert (tabular / "data" / "test.json").is_file()
        assert (tabular / "checkpoints" / "README.md").is_file()
        tabular_doctor = run("doctor", str(tabular), "--format=json")
        assert tabular_doctor.returncode == 0, tabular_doctor.stderr
        assert json.loads(tabular_doctor.stdout)["status"] == "warning"
        tabular_check = run("check", str(tabular), "--quiet")
        assert tabular_check.returncode == 0, tabular_check.stderr
        tabular_run = run("run", str(tabular), "--format=json")
        assert tabular_run.returncode == 0, tabular_run.stderr
        assert json.loads(tabular_run.stdout)["value"]["loss_decreased"] is True
        assert (tabular / "checkpoints" / "model.json").is_file()

        source_mlp = root / "source-mlp"
        source_init = run("init", str(source_mlp), "--template=source-mlp", "--format=json")
        assert source_init.returncode == 0, source_init.stderr
        source_result = json.loads(source_init.stdout)
        assert source_result["schema"] == "seloryn.init.v1"
        assert source_result["template"] == "source-mlp"
        assert (source_mlp / "src" / "main.sely").is_file()
        source_text = (source_mlp / "src" / "main.sely").read_text(encoding="utf-8")
        manifest_text = (source_mlp / "seloryn.toml").read_text(encoding="utf-8")
        assert "train_mlp" not in source_text
        assert 'read = ["data", "checkpoints"]' in manifest_text
        assert 'write = ["checkpoints"]' in manifest_text
        source_doctor = run("doctor", str(source_mlp), "--format=json")
        assert source_doctor.returncode == 0, source_doctor.stderr
        assert json.loads(source_doctor.stdout)["status"] == "warning"
        source_check = run("check", str(source_mlp), "--quiet")
        assert source_check.returncode == 0, source_check.stderr
        source_run = run("run", str(source_mlp), "--format=json", "--log=off")
        assert source_run.returncode == 0, source_run.stderr
        source_value = json.loads(source_run.stdout)["value"]
        source_report = source_value["value"]["fields"]
        assert source_report["architecture"] == "source_owned_relu_mlp", source_report
        assert source_report["final_loss"] < source_report["initial_loss"], source_report
        assert abs(source_report["reloaded_loss"] - source_report["evaluation_loss"]) < 1e-6, source_report
        assert (source_mlp / "checkpoints" / "model.json").is_file()

        outside = root / "outside.json"
        outside.write_text('{"inputs": [0], "targets": [0]}\n', encoding="utf-8")

        read_escape = root / "source-read-escape"
        shutil.copytree(source_mlp, read_escape)
        (read_escape / "src" / "main.sely").write_text(
            source_text.replace('"data/train.json"', '"../outside.json"'),
            encoding="utf-8",
        )
        read_failure = run("run", str(read_escape), "--format=json", "--log=off")
        assert read_failure.returncode != 0, read_failure.stdout
        assert "granted read root" in (read_failure.stdout + read_failure.stderr), (
            read_failure.stdout,
            read_failure.stderr,
        )

        write_escape = root / "source-write-escape"
        shutil.copytree(source_mlp, write_escape)
        (write_escape / "src" / "main.sely").write_text(
            source_text.replace('"checkpoints/model.json"', '"data/model.json"'),
            encoding="utf-8",
        )
        write_failure = run("run", str(write_escape), "--format=json", "--log=off")
        assert write_failure.returncode != 0, write_failure.stdout
        assert "declared write capability" in (write_failure.stdout + write_failure.stderr), (
            write_failure.stdout,
            write_failure.stderr,
        )

        malformed = root / "source-malformed-checkpoint"
        shutil.copytree(source_mlp, malformed)
        checkpoint_path = malformed / "checkpoints" / "model.json"
        checkpoint = json.loads(checkpoint_path.read_text(encoding="utf-8"))
        checkpoint["weights_sha256"] = "0" * 64
        checkpoint_path.write_text(json.dumps(checkpoint), encoding="utf-8")
        checkpoint_prefix = source_text.split("fn main() ->", 1)[0]
        (malformed / "src" / "main.sely").write_text(
            checkpoint_prefix
            + 'fn main() -> Result[Model, JsonIoError]:\n'
            + '    return load_model("checkpoints/model.json")\n',
            encoding="utf-8",
        )
        checkpoint_failure = run("run", str(malformed), "--format=json", "--log=off")
        assert checkpoint_failure.returncode != 0, checkpoint_failure.stdout
        assert "weights hash does not match" in (
            checkpoint_failure.stdout + checkpoint_failure.stderr
        )

        invalid = root / "not-a-project"
        invalid.mkdir()
        invalid_doctor = run("doctor", str(invalid), "--format=json")
        assert invalid_doctor.returncode == 65
        invalid_result = json.loads(invalid_doctor.stderr)
        assert invalid_result["schema"] == "seloryn.doctor.v1"
        assert invalid_result["status"] == "error"

    print("Seloryn workspace verification: doctor, init templates, source-owned MLP, no-overwrite and static validation passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
