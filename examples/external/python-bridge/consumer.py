"""External application example using the installed Seloryn bridge."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from seloryn_bridge import SelorynBridge


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True)
    parser.add_argument("--project", required=True)
    args = parser.parse_args()

    binary = Path(args.binary).resolve()
    project = Path(args.project).resolve()
    bridge = SelorynBridge(binary, cwd=project.parent)
    result = bridge.run(project, input={"name": "Seloryn"})
    print(json.dumps(result.value, ensure_ascii=False, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
