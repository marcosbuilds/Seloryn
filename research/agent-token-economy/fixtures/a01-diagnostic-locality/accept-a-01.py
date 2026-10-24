#!/usr/bin/env python3
"""Independent A-01 acceptance; provider metrics are deliberately out of scope."""
import json
import sys
from pathlib import Path

FORBIDDEN = {"tokens", "cost_usd", "calls", "attempts", "success", "human_intervention", "files_changed", "lines_added", "lines_deleted", "regressions"}

def main() -> int:
    if len(sys.argv) != 2: return 2
    try: data = json.loads((Path(sys.argv[1]) / "submission-a-01.json").read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError): return 1
    obs = data.get("observations", {})
    span = obs.get("primary_span", {})
    ok = data.get("task_id") == "A-01" and not (FORBIDDEN & set(data)) and isinstance(obs.get("diagnostic_code"), str) and obs["diagnostic_code"].startswith("SEL-") and isinstance(span, dict) and span.get("end", -1) > span.get("start", -1) and obs.get("machine_readable") is True and obs.get("negative_case") is True
    print(json.dumps({"task_id":"A-01","status":"passed" if ok else "failed"}, sort_keys=True))
    return 0 if ok else 1

if __name__ == "__main__": raise SystemExit(main())
