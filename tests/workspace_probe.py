"""Black-box evidence for the read-only doctor and transactional init commands."""

from __future__ import annotations

import json
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

        invalid = root / "not-a-project"
        invalid.mkdir()
        invalid_doctor = run("doctor", str(invalid), "--format=json")
        assert invalid_doctor.returncode == 65
        invalid_result = json.loads(invalid_doctor.stderr)
        assert invalid_result["schema"] == "seloryn.doctor.v1"
        assert invalid_result["status"] == "error"

    print("Seloryn workspace verification: doctor, init templates, no-overwrite and static validation passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
