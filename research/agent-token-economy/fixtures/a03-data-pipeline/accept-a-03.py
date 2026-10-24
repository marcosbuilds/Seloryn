#!/usr/bin/env python3
"""Independent A-03 acceptance; provider metrics are deliberately out of scope."""
import json
import sys
from pathlib import Path

FORBIDDEN = {"tokens", "cost_usd", "calls", "attempts", "success", "human_intervention", "files_changed", "lines_added", "lines_deleted", "regressions"}

def main() -> int:
    if len(sys.argv) != 2: return 2
    try: data = json.loads((Path(sys.argv[1]) / "submission-a-03.json").read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError): return 1
    obs = data.get("observations", {})
    ok = data.get("task_id") == "A-03" and not (FORBIDDEN & set(data)) and all(obs.get(key) is True for key in ("typed_input", "accepted_loop", "malformed_input_rejected")) and isinstance(obs.get("shape_or_type_diagnostic"), str) and obs["shape_or_type_diagnostic"].startswith("SEL-")
    print(json.dumps({"task_id":"A-03","status":"passed" if ok else "failed"}, sort_keys=True))
    return 0 if ok else 1

if __name__ == "__main__": raise SystemExit(main())
