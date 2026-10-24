"""Black-box proof for the source-level optimizer contract example."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PROJECT = ROOT / "examples" / "projects" / "source-contract-reuse"
BINARY = ROOT / "target" / "release" / ("seloryn.exe" if sys.platform == "win32" else "seloryn")


def run(*arguments: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [str(BINARY), *arguments],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )


def main() -> None:
    assert BINARY.is_file(), BINARY
    assert PROJECT.is_dir(), PROJECT

    contract_parameters = PROJECT / "src" / "contract" / "parameters.sely"
    contract_optim = PROJECT / "src" / "contract" / "optim.sely"
    mlp_consumer = PROJECT / "src" / "consumer" / "mlp.sely"
    language_model_consumer = PROJECT / "src" / "consumer" / "language_model.sely"

    assert "module contract.parameters" in contract_parameters.read_text(encoding="utf-8")
    assert "module contract.optim" in contract_optim.read_text(encoding="utf-8")
    assert "from contract.parameters import" in contract_optim.read_text(encoding="utf-8")
    assert "from contract.optim import" in mlp_consumer.read_text(encoding="utf-8")
    assert "from contract.parameters import" in mlp_consumer.read_text(encoding="utf-8")
    assert "from contract.optim import" in language_model_consumer.read_text(encoding="utf-8")
    assert "from contract.parameters import" in language_model_consumer.read_text(encoding="utf-8")
    assert not (PROJECT / "src" / "consumer" / "parameters.sely").exists()
    assert not (PROJECT / "src" / "consumer" / "optim.sely").exists()

    checked = run("check", str(PROJECT), "--format=json")
    assert checked.returncode == 0, (checked.stdout, checked.stderr)

    executed = run("run", str(PROJECT), "--format=json")
    assert executed.returncode == 0, (executed.stdout, executed.stderr)
    envelope = json.loads(executed.stdout)
    values = envelope["value"]["value"]
    assert abs(values["mlp_first_weight"] - 0.95) < 1e-5, values
    assert abs(values["language_model_last_weight"] - 2.9) < 1e-5, values

    print("Source-level contract reuse proof: PASS")
    print("gates | shared_modules=2 consumers=2 check=0 run=0")


if __name__ == "__main__":
    try:
        main()
    except (AssertionError, OSError, subprocess.SubprocessError, UnicodeError, json.JSONDecodeError) as error:
        print(f"source contract reuse probe FAILED: {error!r}", file=sys.stderr)
        raise SystemExit(1) from error
