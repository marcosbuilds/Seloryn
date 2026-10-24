"""Independent stdlib oracle for BENCH-03 Numerical Computation."""

from __future__ import annotations

from pathlib import Path


LEFT = [
    [1.0, 2.0, 3.0, 4.0],
    [5.0, 6.0, 7.0, 8.0],
    [9.0, 10.0, 11.0, 12.0],
    [13.0, 14.0, 15.0, 16.0],
]
RIGHT = [
    [16.0, 15.0, 14.0, 13.0],
    [12.0, 11.0, 10.0, 9.0],
    [8.0, 7.0, 6.0, 5.0],
    [4.0, 3.0, 2.0, 1.0],
]


def reference(repo_root: Path) -> dict:
    del repo_root
    values = [
        [sum(LEFT[row][inner] * RIGHT[inner][column] for inner in range(4)) for column in range(4)]
        for row in range(4)
    ]
    return {"status": "ok", "shape": [4, 4], "values": [value for row in values for value in row]}
