"""Black-box proof for Seloryn's real physical GPU compute probe.

The test is intentionally capability-aware: hosts without a supported adapter
skip the GPU assertion, while a reported adapter must execute the shader and
produce the expected readback. This keeps the matrix honest without claiming
GPU training on CPU-only CI runners.
"""

from __future__ import annotations

import json
import subprocess
import sys
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
        timeout=60,
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

    devices = json.loads(run("devices", "--format=json").stdout)
    adapters = [
        device
        for device in devices["devices"]
        if device["device_type"] in {"discretegpu", "integratedgpu"}
    ]
    if not adapters:
        print("Seloryn GPU verification skipped: no physical non-CPU adapter reported")
        return 0

    probe = json.loads(run("gpu-probe", "--format=json").stdout)
    assert probe["schema"] == 1
    assert probe["event"] == "gpu.probe"
    assert probe["elements"] == 8
    assert probe["adapter"]["device_type"] in {
        "discretegpu",
        "integratedgpu",
        "integratedgpu",
    }
    assert probe["max_abs_error"] <= 1.0e-6
    assert probe["result"] == [3.0, -3.0, 8.0, 9.5, -10.0, 13.0, 16.5, 18.0]
    print(
        "Seloryn GPU verification: real physical non-fallback compute/readback passed "
        f"on {probe['adapter']['name']} backend={probe['adapter']['backend']}"
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (AssertionError, OSError, json.JSONDecodeError) as error:
        print(f"Seloryn GPU verification FAILED: {error!r}", file=sys.stderr)
        raise SystemExit(1) from error
