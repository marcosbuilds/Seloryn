"""Validate a benchmark manifest or generated report with stdlib only."""

from __future__ import annotations

import argparse
from pathlib import Path

from schema_validator import load_and_validate


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("document", type=Path)
    parser.add_argument("--schema", type=Path, required=True)
    args = parser.parse_args()
    load_and_validate(args.document, args.schema)
    print(f"valid: {args.document} against {args.schema}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError) as error:
        print(f"invalid JSON document: {error}")
        raise SystemExit(1) from error
