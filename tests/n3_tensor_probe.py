"""Black-box proof for the bounded Tensor[F32] reverse-mode gate."""

from __future__ import annotations

import argparse
import math
import subprocess
import sys
import tempfile
from pathlib import Path


SOURCE = """from autograd import backward
fn objective(input: Tensor[F32]) -> Tensor[F32]:
    let weights: Tensor[F32] = tensor_f32([2.0, 3.0, 4.0, 5.0], [2, 2])
    let prediction: Tensor[F32] = matmul(input, weights)
    return tensor_mean(prediction * prediction)
fn main() -> Tensor[F32]:
    let input: Tensor[F32] = tensor_f32([1.0, 2.0, 3.0, 4.0], [2, 2])
    return backward(objective, input)
"""


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", type=Path)
    args = parser.parse_args()
    binary = args.bin or Path(
        "target/release/seloryn.exe" if sys.platform == "win32" else "target/release/seloryn"
    )
    with tempfile.TemporaryDirectory(prefix="seloryn-n3b-proof-") as directory:
        path = Path(directory) / "tensor-gradient.sely"
        path.write_text(SOURCE, encoding="utf-8")
        checked = subprocess.run(
            [str(binary), "check", str(path)], capture_output=True, text=True, timeout=40
        )
        assert checked.returncode == 0, checked.stderr
        result = subprocess.run(
            [str(binary), "run", str(path)], capture_output=True, text=True, timeout=40
        )
        assert result.returncode == 0, result.stderr
        prefix = "Tensor[F32, shape=[2, 2], values=["
        assert result.stdout.startswith(prefix), result.stdout
        values = [
            float(value)
            for value in result.stdout[len(prefix) :].split("]", 1)[0].split(", ")
        ]

    # d mean((XW)^2) / dX = (2 / 4) * (XW) W^T.
    xw = [[10.0, 13.0], [22.0, 29.0]]
    expected = [
        0.5 * (xw[0][0] * 2.0 + xw[0][1] * 3.0),
        0.5 * (xw[0][0] * 4.0 + xw[0][1] * 5.0),
        0.5 * (xw[1][0] * 2.0 + xw[1][1] * 3.0),
        0.5 * (xw[1][0] * 4.0 + xw[1][1] * 5.0),
    ]
    assert len(values) == len(expected)
    for actual, oracle in zip(values, expected):
        assert math.isclose(actual, oracle, rel_tol=2e-5, abs_tol=2e-5), (
            actual,
            oracle,
        )
    print("Seloryn N3b tensor reverse-mode verification: 1 case passed")


if __name__ == "__main__":
    main()
