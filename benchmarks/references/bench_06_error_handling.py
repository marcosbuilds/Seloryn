"""Independent stdlib oracle for BENCH-06 Error Handling."""

from __future__ import annotations

from pathlib import Path


def reference(repo_root: Path) -> dict:
    del repo_root
    try:
        value = 0.0
        if value <= 0.0:
            raise ValueError("log input must be greater than zero")
    except ValueError as error:
        return {"status": "error", "category": "domain", "message": str(error)}
    raise AssertionError("the invalid domain input unexpectedly succeeded")
