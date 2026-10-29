"""Independent stdlib-only oracles for the P0 workloads."""

from __future__ import annotations

import json
import struct
from pathlib import Path


ROOT = Path(__file__).resolve().parent


def array_aggregation() -> int:
    return sum(value * value for value in range(750000) if value % 3 == 0)


def tensor_loop() -> float:
    # This oracle validates the deterministic scalar contract using the same
    # seeded values and a plain Python matrix multiply, not Seloryn internals.
    def f32(value: float) -> float:
        return struct.unpack("<f", struct.pack("<f", value))[0]

    def uniform(seed: int) -> list[list[float]]:
        state = seed & 0xFFFFFFFFFFFFFFFF
        if state == 0:
            state = 0x9E3779B97F4A7C15
        values: list[float] = []
        for _ in range(32 * 32):
            state ^= state >> 12
            state ^= (state << 25) & 0xFFFFFFFFFFFFFFFF
            state ^= state >> 27
            random = (state * 0x2545F4914F6CDD1D) & 0xFFFFFFFFFFFFFFFF
            unit = random / 0xFFFFFFFFFFFFFFFF
            values.append(f32((unit * 2.0 - 1.0) * 0.1))
        return [values[row * 32 : (row + 1) * 32] for row in range(32)]

    left = uniform(20261029)
    right = uniform(20261030)
    product = []
    for row in range(32):
        output_row = []
        for column in range(32):
            value = 0.0
            for inner in range(32):
                value = f32(value + f32(left[row][inner] * right[inner][column]))
            output_row.append(value)
        product.append(output_row)
    total = 0.0
    for row in product:
        for value in row:
            total = f32(total + value)
    # `tensor_item_f32` crosses the tensor boundary as the language's scalar
    # F64 value, so the outer checksum follows the runtime scalar contract.
    checksum = 0.0
    for _ in range(250):
        checksum += total
    return checksum


def numeric_json_pipeline() -> float:
    values = json.loads((ROOT / "numeric-json-pipeline" / "data" / "p0-numeric.json").read_text(encoding="utf-8"))["values"]
    total = 0.0
    for _ in range(50000):
        for value in values:
            total = total + value * value if value > 0.0 else total - value
    return total


if __name__ == "__main__":
    print(json.dumps({
        "array_aggregation": array_aggregation(),
        "tensor_loop": tensor_loop(),
        "numeric_json_pipeline": numeric_json_pipeline(),
    }, sort_keys=True))
