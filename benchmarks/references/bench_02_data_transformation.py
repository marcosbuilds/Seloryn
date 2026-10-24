"""Independent stdlib oracle for BENCH-02 Data Transformation."""

from __future__ import annotations

import json
from pathlib import Path


def reference(repo_root: Path) -> dict:
    path = repo_root / "examples" / "projects" / "json-array-pipeline" / "data" / "amounts.json"
    document = json.loads(path.read_text(encoding="utf-8"))
    amounts = document["amounts"]
    if not isinstance(amounts, list) or not all(isinstance(value, (int, float)) for value in amounts):
        raise TypeError("amounts must be an array of numbers")
    return {
        "status": "ok",
        "value": float(sum(amounts)),
        "count": len(amounts),
        "minimum": min(amounts),
        "maximum": max(amounts),
    }
