"""Black-box proof for the documented first-run CLI path."""

from __future__ import annotations

import shutil
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
POSIX_SCRIPT = ROOT / "scripts" / "quickstart" / "first-run-posix.sh"
POWERSHELL_SCRIPT = ROOT / "scripts" / "quickstart" / "first-run-powershell.ps1"
POSIX_DOCTOR = ROOT / "scripts" / "quickstart" / "doctor-posix.sh"
POWERSHELL_DOCTOR = ROOT / "scripts" / "quickstart" / "doctor-powershell.ps1"


def run(command: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(command, cwd=ROOT, text=True, capture_output=True, check=False)


def assert_success(result: subprocess.CompletedProcess[str], label: str) -> None:
    assert result.returncode == 0, f"{label} failed ({result.returncode})\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}"


def assert_script_contract() -> None:
    for script in (POSIX_SCRIPT, POWERSHELL_SCRIPT, POSIX_DOCTOR, POWERSHELL_DOCTOR):
        assert script.is_file(), f"missing first-run script: {script}"
    posix = POSIX_SCRIPT.read_text(encoding="utf-8")
    assert "set -euo pipefail" in posix
    for phase in ("doctor", "check", "run", "conformance"):
        assert phase in posix, f"POSIX quickstart omits phase: {phase}"
    powershell = POWERSHELL_SCRIPT.read_text(encoding="utf-8")
    assert "LASTEXITCODE" in powershell
    for phase in ("doctor", "check", "run", "conformance"):
        assert phase in powershell, f"PowerShell quickstart omits phase: {phase}"


def platform_commands() -> tuple[list[str], list[str]]:
    if sys.platform == "win32":
        shell = shutil.which("pwsh") or shutil.which("powershell")
        assert shell, "PowerShell 5.1 or 7 is required to run the Windows first-run probe"
        return (
            [shell, "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", str(POWERSHELL_SCRIPT)],
            [shell, "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", str(POWERSHELL_DOCTOR)],
        )
    shell = shutil.which("bash")
    assert shell, "bash is required to run the POSIX first-run probe"
    return ([shell, str(POSIX_SCRIPT)], [shell, str(POSIX_DOCTOR)])


def main() -> int:
    assert_script_contract()
    first_run, doctor = platform_commands()

    positive = run(first_run)
    assert_success(positive, "first-run")
    combined = positive.stdout + positive.stderr
    assert "SELORYN-QUICKSTART: PASS" in combined
    assert "Tensor[F32, shape=[2, 2], values=[19.0, 22.0, 43.0, 50.0]]" in combined

    repeat = run(first_run + (["-SkipBuild"] if sys.platform == "win32" else ["--skip-build"]))
    assert_success(repeat, "repeat first-run")
    assert "SELORYN-QUICKSTART: PASS" in (repeat.stdout + repeat.stderr)

    missing_python = "seloryn-missing-python-for-first-run-test"
    negative = run(
        doctor
        + (["-PythonCommand", missing_python] if sys.platform == "win32" else ["--python-command", missing_python])
    )
    assert negative.returncode != 0
    negative_output = negative.stdout + negative.stderr
    assert "Python 3.10" in negative_output
    assert "install" in negative_output.lower()

    missing_cargo = "seloryn-missing-cargo-for-first-run-test"
    cargo_negative = run(
        doctor
        + (["-CargoCommand", missing_cargo] if sys.platform == "win32" else ["--cargo-command", missing_cargo])
    )
    assert cargo_negative.returncode != 0
    cargo_output = cargo_negative.stdout + cargo_negative.stderr
    assert "Rust/Cargo" in cargo_output
    assert "cargo --version" in cargo_output

    missing_node = "seloryn-missing-node-for-first-run-test"
    node_negative = run(
        doctor
        + (
            ["-RequireNode", "-NodeCommand", missing_node]
            if sys.platform == "win32"
            else ["--require-node", "--node-command", missing_node]
        )
    )
    assert node_negative.returncode != 0
    node_output = node_negative.stdout + node_negative.stderr
    assert "Node.js" in node_output
    assert "node --version" in node_output

    binary = ROOT / "target" / "release" / ("seloryn.exe" if sys.platform == "win32" else "seloryn")
    regression = run([str(binary), "run", "tests/result-error.sely"])
    assert regression.returncode == 65, regression.stderr
    assert regression.stderr, "negative runtime regression must explain the failure"

    print("Seloryn first-run verification: positive path, repeatability, missing-Python diagnostic and runtime failure regression passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
