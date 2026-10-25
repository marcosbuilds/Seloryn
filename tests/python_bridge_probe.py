"""Black-box proof for the dependency-free Python JSON/stdio bridge."""

from __future__ import annotations

import json
from pathlib import Path
import sys
import tempfile


ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target" / "release" / ("seloryn.exe" if sys.platform == "win32" else "seloryn")
sys.path.insert(0, str(ROOT / "sdk" / "python"))

from seloryn_bridge import BridgeError, SelorynBridge  # noqa: E402


def main() -> int:
    assert BINARY.is_file(), BINARY
    project = ROOT / "examples" / "external" / "python-bridge"
    bridge = SelorynBridge(BINARY, timeout=10)
    bridge.check(project)

    result = bridge.run(project, input={"name": "Integração"})
    assert result.protocol == "seloryn-python-bridge.v1", result
    assert result.stdout == "", result.stdout
    assert result.value["ok"] is True, result.value
    assert result.value["value"] == {
        "length": 10,
        "name": "Integração",
        "protocol": "seloryn-json-stdio.v1",
    }, result.value

    try:
        bridge.run(project, input={"wrong": "field"})
    except BridgeError as error:
        assert error.code == "SEL-R127", (error.code, error.stderr)
        assert error.exit_code == 65, error.exit_code
        assert error.stdout == "", error.stdout
        assert json.loads(error.stderr)["diagnostics"][0]["code"] == "SEL-R127"
    else:
        raise AssertionError("invalid bridge input unexpectedly succeeded")

    try:
        SelorynBridge(BINARY, input_limit=4).run(project, input={"name": "too large"})
    except BridgeError as error:
        assert error.code == "SEL-BRIDGE-INPUT_LIMIT", error.code
    else:
        raise AssertionError("input limit was not enforced")

    try:
        SelorynBridge(BINARY, output_limit=1).run(project, input={"name": "x"})
    except BridgeError as error:
        assert error.code == "SEL-BRIDGE-OUTPUT_LIMIT", error.code
    else:
        raise AssertionError("output limit was not enforced")

    with tempfile.TemporaryDirectory(prefix="seloryn-bridge-") as temporary:
        external_copy = Path(temporary) / "consumer"
        external_copy.mkdir()
        for source in project.iterdir():
            if source.is_file():
                (external_copy / source.name).write_bytes(source.read_bytes())
        (external_copy / "src").mkdir()
        (external_copy / "src" / "main.sely").write_text(
            (project / "src" / "main.sely").read_text(encoding="utf-8"),
            encoding="utf-8",
        )
        copied = SelorynBridge(BINARY, cwd=external_copy).run(
            external_copy,
            input={"name": "fora-da-arvore"},
        )
        assert copied.value["ok"] is True, copied.value

    print("Seloryn Python bridge verification: external JSON/stdio success, typed failure, bounds and copied project passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
