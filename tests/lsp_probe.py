"""Black-box LSP v1 probe for unsaved diagnostics and parser-backed queries."""

from __future__ import annotations

import json
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target" / "release" / ("seloryn.exe" if __import__("sys").platform == "win32" else "seloryn")


def send(process: subprocess.Popen[bytes], value: dict[str, object]) -> None:
    payload = json.dumps(value, separators=(",", ":")).encode("utf-8")
    process.stdin.write(f"Content-Length: {len(payload)}\r\n\r\n".encode("ascii") + payload)
    process.stdin.flush()


def receive(process: subprocess.Popen[bytes]) -> dict[str, object]:
    headers: dict[str, str] = {}
    while True:
        line = process.stdout.readline()
        if not line:
            raise AssertionError(f"LSP closed unexpectedly; stderr={process.stderr.read()!r}")
        decoded = line.decode("ascii").strip()
        if not decoded:
            break
        key, value = decoded.split(":", 1)
        headers[key.lower()] = value.strip()
    length = int(headers["content-length"])
    return json.loads(process.stdout.read(length).decode("utf-8"))


def main() -> int:
    if not BINARY.is_file():
        raise SystemExit(f"release CLI is required at {BINARY}")
    process = subprocess.Popen(
        [str(BINARY), "lsp", "--stdio"],
        cwd=ROOT,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    uri = "file:///probe.sely"
    invalid = "fn main() -> Unit:\n    if 1:\n        return\n"
    valid = "fn helper() -> Unit:\n    return\n\nfn main() -> Unit:\n    helper()\n"
    try:
        send(process, {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}})
        initialized = receive(process)
        assert initialized["id"] == 1
        capabilities = initialized["result"]["capabilities"]
        assert capabilities["completionProvider"] == {}
        assert capabilities["definitionProvider"] is True
        assert capabilities["documentSymbolProvider"] is True

        send(process, {"jsonrpc": "2.0", "method": "initialized", "params": {}})
        send(process, {
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {"textDocument": {"uri": uri, "version": 1, "text": invalid}},
        })
        invalid_notification = receive(process)
        assert invalid_notification["method"] == "textDocument/publishDiagnostics"
        assert invalid_notification["params"]["diagnostics"][0]["code"] == "SEL-T004"

        send(process, {
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": {"uri": uri, "version": 2},
                "contentChanges": [{"text": valid}],
            },
        })
        valid_notification = receive(process)
        assert valid_notification["params"]["diagnostics"] == []

        send(process, {
            "jsonrpc": "2.0",
            "id": 2,
            "method": "textDocument/completion",
            "params": {"textDocument": {"uri": uri}, "position": {"line": 4, "character": 4}},
        })
        completion = receive(process)
        labels = {item["label"] for item in completion["result"]["items"]}
        assert {"helper", "train_mlp", "fn"}.issubset(labels)

        send(process, {
            "jsonrpc": "2.0",
            "id": 3,
            "method": "textDocument/documentSymbol",
            "params": {"textDocument": {"uri": uri}},
        })
        symbols = receive(process)
        assert [item["name"] for item in symbols["result"]] == ["helper", "main"]

        send(process, {
            "jsonrpc": "2.0",
            "id": 4,
            "method": "textDocument/definition",
            "params": {"textDocument": {"uri": uri}, "position": {"line": 4, "character": 5}},
        })
        definition = receive(process)
        assert definition["result"][0]["range"]["start"] == {"line": 0, "character": 3}

        send(process, {"jsonrpc": "2.0", "id": 5, "method": "shutdown", "params": None})
        assert receive(process)["result"] is None
        send(process, {"jsonrpc": "2.0", "method": "exit"})
        assert process.wait(timeout=5) == 0
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
    print("LSP probe passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
