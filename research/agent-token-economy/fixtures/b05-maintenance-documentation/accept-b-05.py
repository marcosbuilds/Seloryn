#!/usr/bin/env python3
"""Independent B-05 acceptance; provider metrics are deliberately out of scope."""
import json
import sys
from pathlib import Path

FORBIDDEN = {"tokens", "cost_usd", "calls", "attempts", "success", "human_intervention", "files_changed", "lines_added", "lines_deleted", "regressions"}

def main() -> int:
    if len(sys.argv) != 2: return 2
    try: data = json.loads((Path(sys.argv[1]) / "submission-b-05.json").read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError): return 1
    obs = data.get("observations", {})
    keys = ("command_discoverable", "acceptance_reproducible", "failure_boundary_documented", "rollback_or_limit_documented")
    ok = data.get("task_id") == "B-05" and not (FORBIDDEN & set(data)) and all(obs.get(key) is True for key in keys)
    print(json.dumps({"task_id":"B-05","status":"passed" if ok else "failed"}, sort_keys=True))
    return 0 if ok else 1

if __name__ == "__main__": raise SystemExit(main())
