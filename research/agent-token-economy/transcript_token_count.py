#!/usr/bin/env python3
"""Compatibility shim for the canonical :mod:`transcript` module.

The old filename remains executable because it is present in historical
fixtures and external commands. New code should import or execute
``transcript.py``.
"""

from __future__ import annotations

import importlib.util
from pathlib import Path


_CANONICAL = Path(__file__).with_name("transcript.py")
_SPEC = importlib.util.spec_from_file_location("seloryn_agent_transcript", _CANONICAL)
if _SPEC is None or _SPEC.loader is None:  # pragma: no cover - import setup failure
    raise ImportError(f"could not load canonical transcript module: {_CANONICAL}")
_MODULE = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(_MODULE)

SCHEMA = _MODULE.SCHEMA
INPUT_ROLES = _MODULE.INPUT_ROLES
OUTPUT_ROLES = _MODULE.OUTPUT_ROLES
load_transcript = _MODULE.load_transcript
measure = _MODULE.measure
main = _MODULE.main


if __name__ == "__main__":
    raise SystemExit(main())
