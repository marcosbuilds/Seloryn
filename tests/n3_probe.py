"""Independent black-box evidence for the narrow Seloryn N3a scalar gate.

The expected derivatives are computed by Python's standard-library math and a
central finite-difference oracle. This script never imports Seloryn internals;
it checks the compiled CLI through real ``check`` and ``run`` invocations.
"""

from __future__ import annotations

import argparse
import math
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Callable


def program(body: str, builtin: str = "backward") -> str:
    return (
        f"from autograd import {builtin}\n"
        "fn main_fn(x: F64) -> F64:\n"
        f"{body}\n"
        "fn main() -> F64:\n"
        f"    return {builtin}(main_fn, POINT)\n"
    )


def run(binary: Path, command: str, path: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [str(binary), command, str(path)],
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=40,
        check=False,
    )


def finite_difference(function: Callable[[float], float], point: float) -> float:
    epsilon = 1.0e-5
    return (function(point + epsilon) - function(point - epsilon)) / (2.0 * epsilon)


def assert_gradient(
    binary: Path,
    root: Path,
    name: str,
    body: str,
    point: float,
    function: Callable[[float], float],
    builtin: str = "backward",
) -> None:
    source = program(body, builtin).replace("POINT", f"{point:.12f}")
    path = root / f"{name}.sely"
    path.write_text(source, encoding="utf-8")
    checked = run(binary, "check", path)
    assert checked.returncode == 0, f"{name} static check: {checked.stderr}"
    actual_run = run(binary, "run", path)
    assert actual_run.returncode == 0, f"{name} runtime: {actual_run.stderr}"
    actual = float(actual_run.stdout.strip())
    expected = finite_difference(function, point)
    assert math.isclose(actual, expected, rel_tol=2.0e-3, abs_tol=2.0e-3), (
        name,
        actual,
        expected,
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", type=Path)
    args = parser.parse_args()
    binary = args.bin or Path(
        "target/release/seloryn.exe" if sys.platform == "win32" else "target/release/seloryn"
    )

    cases = [
        (
            "square-linear",
            "    return x * x + x / 2.0",
            4.0,
            lambda x: x * x + x / 2.0,
            "backward",
        ),
        (
            "helper-call",
            "    return twice(x) * x\n\nfn twice(x: F64) -> F64:\n    return x + x",
            4.0,
            lambda x: (x + x) * x,
            "backward",
        ),
        (
            "constant-power",
            "    return x ** 3.0",
            2.0,
            lambda x: x**3.0,
            "backward",
        ),
        (
            "conditional-positive",
            "    if x > 0.0:\n        return x * x\n    return -x",
            2.0,
            lambda x: x * x if x > 0.0 else -x,
            "backward",
        ),
        (
            "conditional-negative",
            "    if x > 0.0:\n        return x * x\n    return -x",
            -2.0,
            lambda x: x * x if x > 0.0 else -x,
            "gradcheck",
        ),
    ]

    with tempfile.TemporaryDirectory(prefix="seloryn-n3a-proof-") as directory:
        root = Path(directory)
        for case in cases:
            assert_gradient(binary, root, *case)

        unsupported = program("    return x ** x", "backward").replace("POINT", "2.0")
        unsupported_path = root / "variable-power.sely"
        unsupported_path.write_text(unsupported, encoding="utf-8")
        failed = run(binary, "run", unsupported_path)
        assert failed.returncode == 65, failed.returncode
        assert "SEL-R203" in failed.stderr, failed.stderr
        assert "variable exponents" in failed.stderr, failed.stderr

        invalid_callable = (
            "from autograd import backward\n"
            "fn wrong(flag: Bool) -> F64:\n"
            "    return 1.0\n"
            "fn main() -> F64:\n"
            "    return backward(wrong, 2.0)\n"
        )
        invalid_path = root / "invalid-callable.sely"
        invalid_path.write_text(invalid_callable, encoding="utf-8")
        invalid = run(binary, "check", invalid_path)
        assert invalid.returncode == 65, invalid.returncode
        assert "SEL-M002" in invalid.stderr, invalid.stderr
        assert "one F32/F64 parameter" in invalid.stderr, invalid.stderr

    print(f"Seloryn N3a black-box verification: {len(cases) + 2} cases passed")


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:
        print(f"N3a verification FAILED: {exc!r}", file=sys.stderr)
        raise SystemExit(1) from exc
