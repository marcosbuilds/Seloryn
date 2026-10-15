"""Black-box proof for the Seloryn-authored optimization loop."""

import json
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target" / "release" / ("seloryn.exe" if __import__("os").name == "nt" else "seloryn")
PROJECT = ROOT / "examples" / "projects" / "self-hosted-linear"


def main() -> None:
    result = subprocess.run(
        [str(BINARY), "run", str(PROJECT), "--format=json"],
        cwd=ROOT,
        text=True,
        encoding="utf-8",
        capture_output=True,
        check=False,
        timeout=40,
    )
    assert result.returncode == 0, result.stderr
    envelope = json.loads(result.stdout)
    fields = envelope["value"]["fields"]
    assert fields["initial_loss"] > 40.0
    assert fields["final_loss"] < 1.0e-3
    assert abs(fields["weight"] - 2.0) < 0.01
    assert abs(fields["bias"] - 1.0) < 0.01
    print("Seloryn self-hosted optimization verification: gradients, range and SGD loop passed")


if __name__ == "__main__":
    main()
