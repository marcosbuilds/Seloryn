"""Black-box checks for earned language and extension version progression."""

from __future__ import annotations

import importlib.util
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "release" / "verify-version.py"
SPEC = importlib.util.spec_from_file_location("seloryn_versioning", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise AssertionError(f"cannot load version verifier: {SCRIPT}")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def expect_failure(callback, *arguments: object) -> None:
    try:
        callback(*arguments)
    except SystemExit as error:
        assert str(error).startswith("version verification failed:"), error
    else:
        raise AssertionError("version verifier accepted invalid input")


def main() -> int:
    MODULE.validate(ROOT)
    assert MODULE.parse_package_version("0.4.0", "current") == (0, 4, 0)
    assert MODULE.parse_package_version("1.12.3", "current") == (1, 12, 3)
    expect_failure(MODULE.parse_package_version, "0.4", "short package")
    expect_failure(MODULE.parse_package_version, "0.4.0-beta", "pre-release package")
    expect_failure(MODULE.require_increasing, (0, 1, 0), (0, 1, 0), "extension package")
    expect_failure(MODULE.require_increasing, (0, 1, 0), (0, 2, 0), "extension package")
    print("Seloryn versioning verification: canonical SemVer, monotonic comparison and evidence ledger passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
