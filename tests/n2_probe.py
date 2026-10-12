"""Independent, dependency-free CLI evidence for Seloryn N2.

The reference calculations use only Python's standard-library math module and
never import the Rust tensor implementation.
"""

from __future__ import annotations

import argparse
import ast
import math
import re
import subprocess
import sys
import tempfile
from pathlib import Path


TENSOR = re.compile(
    r"Tensor\[(?P<dtype>F32|I64), shape=(?P<shape>\[[^]]*\]), values=\[(?P<values>.*)\]\]"
)


def source(body: list[str], return_type: str = "Tensor[F32]") -> str:
    return "fn main() -> " + return_type + ":\n" + "\n".join(
        "    " + line for line in body
    ) + "\n"


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


def parse_tensor(output: str) -> tuple[str, list[int], list[float]]:
    match = TENSOR.fullmatch(output.strip())
    assert match, f"not a tensor result: {output!r}"
    shape = ast.literal_eval(match.group("shape"))
    values_text = match.group("values").strip()
    values = [] if not values_text else [float(value) for value in values_text.split(", ")]
    return match.group("dtype"), shape, values


def assert_tensor(
    output: str,
    expected_shape: list[int],
    expected_values: list[float],
    tolerance: float = 2e-5,
) -> None:
    dtype, shape, values = parse_tensor(output)
    assert dtype == "F32", (dtype, output)
    assert shape == expected_shape, (shape, expected_shape)
    assert len(values) == len(expected_values), (values, expected_values)
    for actual, expected in zip(values, expected_values):
        assert math.isclose(actual, expected, rel_tol=tolerance, abs_tol=tolerance), (
            actual,
            expected,
        )


def literal(value: float) -> str:
    return f"{value:.8f}"


def tensor_literal(values: list[float]) -> str:
    return "[" + ", ".join(literal(value) for value in values) + "]"


def seeded_values(seed: int, count: int, lower: float, upper: float) -> list[float]:
    state = seed & 0xFFFFFFFF
    values = []
    for _ in range(count):
        state = (1664525 * state + 1013904223) & 0xFFFFFFFF
        fraction = state / 0xFFFFFFFF
        values.append(lower + (upper - lower) * fraction)
    return values


def axis_reference(values: list[float], shape: list[int], axis: int, *, logsumexp: bool) -> list[float]:
    outer = math.prod(shape[:axis])
    axis_size = shape[axis]
    inner = math.prod(shape[axis + 1 :])
    result = [0.0] * (outer * inner if logsumexp else len(values))
    for outer_index in range(outer):
        for inner_index in range(inner):
            group = [
                values[(outer_index * axis_size + axis_index) * inner + inner_index]
                for axis_index in range(axis_size)
            ]
            maximum = max(group)
            if logsumexp:
                result[outer_index * inner + inner_index] = maximum + math.log(
                    sum(math.exp(value - maximum) for value in group)
                )
            else:
                denominator = sum(math.exp(value - maximum) for value in group)
                for axis_index, value in enumerate(group):
                    result[(outer_index * axis_size + axis_index) * inner + inner_index] = (
                        math.exp(value - maximum) / denominator
                    )
    return result


def sum_to_shape_reference(
    values: list[float], input_shape: list[int], target_shape: list[int]
) -> list[float]:
    input_strides = [math.prod(input_shape[index + 1 :]) for index in range(len(input_shape))]
    target_strides = [math.prod(target_shape[index + 1 :]) for index in range(len(target_shape))]
    rank_offset = len(input_shape) - len(target_shape)
    result = [0.0] * math.prod(target_shape)
    for index, value in enumerate(values):
        target_index = 0
        for input_axis in range(rank_offset, len(input_shape)):
            coordinate = (index // input_strides[input_axis]) % input_shape[input_axis]
            target_axis = input_axis - rank_offset
            target_coordinate = 0 if target_shape[target_axis] == 1 else coordinate
            target_index += target_coordinate * target_strides[target_axis]
        result[target_index] += value
    return result


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", type=Path)
    args = parser.parse_args()
    binary = args.bin or Path("target/release/seloryn.exe" if sys.platform == "win32" else "target/release/seloryn")

    cases = [
        (
            "transpose",
            source(["return transpose(tensor_f32([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], [2, 3]))"]),
            [3, 2],
            [1.0, 4.0, 2.0, 5.0, 3.0, 6.0],
        ),
        (
            "relu",
            source([
                "return tensor_relu(tensor_f32([-1.0, 0.0, 1.0, 2.0], [2, 2]))"
            ]),
            [2, 2],
            [0.0, 0.0, 1.0, 2.0],
        ),
        (
            "exp",
            source(["return tensor_exp(tensor_f32([0.0, 1.0], [2]))"]),
            [2],
            [1.0, math.exp(1.0)],
        ),
        (
            "log",
            source(["return tensor_log(tensor_f32([1.0, 2.718281828], [2]))"]),
            [2],
            [0.0, 1.0],
        ),
        (
            "softmax",
            source(["return tensor_softmax(tensor_f32([1.0, 2.0, 3.0, 4.0], [2, 2]), 1)" ]),
            [2, 2],
            [
                math.exp(1.0) / (math.exp(1.0) + math.exp(2.0)),
                math.exp(2.0) / (math.exp(1.0) + math.exp(2.0)),
                math.exp(3.0) / (math.exp(3.0) + math.exp(4.0)),
                math.exp(4.0) / (math.exp(3.0) + math.exp(4.0)),
            ],
        ),
        (
            "logsumexp",
            source(["return tensor_logsumexp(tensor_f32([1000.0, 1001.0, 1002.0, 1003.0], [2, 2]), 1)" ]),
            [2],
            [1000.0 + math.log1p(math.exp(1.0)), 1002.0 + math.log1p(math.exp(1.0))],
        ),
        (
            "sum-to-shape",
            source(["return tensor_sum_to_shape(tensor_f32([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], [2, 3]), [1, 3])" ]),
            [1, 3],
            [5.0, 7.0, 9.0],
        ),
    ]

    random_values = seeded_values(0x53454C59, 6, -1.75, 1.75)
    random_positive = seeded_values(0x4E325F50, 6, 0.25, 2.0)
    cases.extend(
        [
            (
                "seeded-relu",
                source([
                    f"return tensor_relu(tensor_f32({tensor_literal(random_values)}, [2, 3]))"
                ]),
                [2, 3],
                [max(value, 0.0) for value in random_values],
            ),
            (
                "seeded-exp",
                source([
                    f"return tensor_exp(tensor_f32({tensor_literal(random_values)}, [2, 3]))"
                ]),
                [2, 3],
                [math.exp(value) for value in random_values],
            ),
            (
                "seeded-softmax",
                source([
                    f"return tensor_softmax(tensor_f32({tensor_literal(random_values)}, [2, 3]), 1)"
                ]),
                [2, 3],
                axis_reference(random_values, [2, 3], 1, logsumexp=False),
            ),
            (
                "seeded-logsumexp",
                source([
                    f"return tensor_logsumexp(tensor_f32({tensor_literal(random_values)}, [2, 3]), 0)"
                ]),
                [3],
                axis_reference(random_values, [2, 3], 0, logsumexp=True),
            ),
            (
                "seeded-sum-to-shape",
                source([
                    f"return tensor_sum_to_shape(tensor_f32({tensor_literal(random_positive * 2)}, [2, 2, 3]), [1, 3])"
                ]),
                [1, 3],
                sum_to_shape_reference(random_positive * 2, [2, 2, 3], [1, 3]),
            ),
        ]
    )

    negative = [
        (
            "log-domain",
            source(["return tensor_log(tensor_f32([0.0], [1]))"]),
            "SEL-R210",
        ),
        (
            "empty-axis",
            source(["return tensor_softmax(tensor_zeros_f32([0, 2]), 0)" ]),
            "SEL-R211",
        ),
        (
            "target-shape",
            source(["return tensor_sum_to_shape(tensor_zeros_f32([2, 2]), [3])" ]),
            "SEL-R204",
        ),
        (
            "static-dtype",
            source(["return tensor_relu(tensor_i64([1], [1]))" ], "Tensor[I64]"),
            "SEL-T002",
        ),
    ]

    count = 0
    with tempfile.TemporaryDirectory(prefix="seloryn-n2-proof-") as directory:
        root = Path(directory)
        for name, program, shape, expected in cases:
            path = root / f"{name}.sely"
            path.write_text(program, encoding="utf-8", newline="\r\n" if name == "transpose" else "\n")
            checked = run(binary, "check", path)
            assert checked.returncode == 0, f"{name} static check: {checked.stderr}"
            actual = run(binary, "run", path)
            assert actual.returncode == 0, f"{name} runtime: {actual.stderr}"
            assert_tensor(actual.stdout, shape, expected)
            count += 1

        for name, program, code in negative:
            path = root / f"{name}-入口.sely"
            path.write_text(program, encoding="utf-8", newline="\n")
            first = run(binary, "run", path)
            second = run(binary, "run", path)
            assert first.returncode == 65, (name, first.returncode, first.stderr)
            assert first.stdout == "", (name, first.stdout)
            assert code in first.stderr, (name, code, first.stderr)
            assert (first.returncode, first.stdout, first.stderr) == (
                second.returncode,
                second.stdout,
                second.stderr,
            ), name
            count += 1

    print(f"Seloryn N2 black-box verification: {count} cases passed")


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:
        print(f"N2 verification FAILED: {exc!r}", file=sys.stderr)
        raise SystemExit(1) from exc
