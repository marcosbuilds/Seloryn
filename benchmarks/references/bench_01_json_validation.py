"""Independent stdlib oracle for BENCH-01 JSON Validation."""

from __future__ import annotations

import json
from pathlib import Path


def reference(repo_root: Path) -> dict:
    path = repo_root / "examples" / "projects" / "json-config" / "data" / "config.json"
    document = json.loads(path.read_text(encoding="utf-8"))
    model = document["model"]["name"]
    if not isinstance(model, str):
        raise TypeError("model.name must be text")
    return {"status": "ok", "value": f"configured model: {model}"}
