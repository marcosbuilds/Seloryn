"""Black-box LSP probe for lifecycle, version ordering and parser-backed queries."""

from __future__ import annotations

import json
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target" / "release" / ("seloryn.exe" if __import__("sys").platform == "win32" else "seloryn")


def send(
    process: subprocess.Popen[bytes],
    value: dict[str, object],
    content_length_header: str = "Content-Length",
) -> None:
    payload = json.dumps(value, separators=(",", ":")).encode("utf-8")
    process.stdin.write(
        f"{content_length_header}: {len(payload)}\r\n"
        "Content-Type: application/vscode-jsonrpc; charset=utf-8\r\n\r\n".encode("ascii")
        + payload
    )
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
        send(
            process,
            {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}},
            content_length_header="content-length",
        )
        initialized = receive(process)
        assert initialized["id"] == 1
        capabilities = initialized["result"]["capabilities"]
        assert capabilities["textDocumentSync"] == {
            "openClose": True,
            "change": 1,
            "save": True,
        }
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

        latest = "fn helper() -> Unit:\n    return\n\nfn current() -> Unit:\n    helper()\n"
        send(process, {
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": {"uri": uri, "version": 4},
                "contentChanges": [{"text": latest}],
            },
        })
        latest_notification = receive(process)
        assert latest_notification["params"]["diagnostics"] == []

        # Version 3 is stale after version 4 and must not emit diagnostics or
        # replace the current document. The following response must arrive next.
        send(process, {
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": {"uri": uri, "version": 3},
                "contentChanges": [{"text": invalid}],
            },
        })
        send(process, {
            "jsonrpc": "2.0",
            "id": 2,
            "method": "textDocument/documentSymbol",
            "params": {"textDocument": {"uri": uri}},
        })
        current_symbols = receive(process)
        assert current_symbols["id"] == 2
        assert [item["name"] for item in current_symbols["result"]] == ["helper", "current"]

        # Full synchronization rejects ranged edits instead of treating their
        # replacement text as a complete document.
        send(process, {
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": {"uri": uri, "version": 5},
                "contentChanges": [{
                    "range": {
                        "start": {"line": 0, "character": 0},
                        "end": {"line": 0, "character": 1},
                    },
                    "text": invalid,
                }],
            },
        })
        send(process, {
            "jsonrpc": "2.0",
            "id": 3,
            "method": "textDocument/documentSymbol",
            "params": {"textDocument": {"uri": uri}},
        })
        after_partial = receive(process)
        assert after_partial["id"] == 3
        assert [item["name"] for item in after_partial["result"]] == ["helper", "current"]

        send(process, {
            "jsonrpc": "2.0",
            "id": 4,
            "method": "textDocument/completion",
            "params": {"textDocument": {"uri": uri}, "position": {"line": 4, "character": 4}},
        })
        completion = receive(process)
        labels = {item["label"] for item in completion["result"]["items"]}
        assert {"helper", "train_mlp", "fn"}.issubset(labels)

        send(process, {
            "jsonrpc": "2.0",
            "id": 5,
            "method": "textDocument/documentSymbol",
            "params": {"textDocument": {"uri": uri}},
        })
        symbols = receive(process)
        assert symbols["id"] == 5
        assert [item["name"] for item in symbols["result"]] == ["helper", "current"]

        send(process, {
            "jsonrpc": "2.0",
            "id": 6,
            "method": "textDocument/definition",
            "params": {"textDocument": {"uri": uri}, "position": {"line": 4, "character": 5}},
        })
        definition = receive(process)
        assert definition["result"][0]["range"]["start"] == {"line": 0, "character": 3}

        send(process, {
            "jsonrpc": "2.0",
            "method": "textDocument/didClose",
            "params": {"textDocument": {"uri": uri}},
        })
        cleared = receive(process)
        assert cleared["method"] == "textDocument/publishDiagnostics"
        assert cleared["params"] == {"uri": uri, "diagnostics": []}

        send(process, {
            "jsonrpc": "2.0",
            "id": 7,
            "method": "textDocument/documentSymbol",
            "params": {"textDocument": {"uri": uri}},
        })
        closed_symbols = receive(process)
        assert closed_symbols["id"] == 7
        assert closed_symbols["result"] == []

        reopened = "fn reopened() -> Unit:\n    return\n"
        send(process, {
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {"textDocument": {"uri": uri, "version": 1, "text": reopened}},
        })
        reopened_notification = receive(process)
        assert reopened_notification["params"]["diagnostics"] == []

        send(process, {
            "jsonrpc": "2.0",
            "id": 8,
            "method": "textDocument/documentSymbol",
            "params": {"textDocument": {"uri": uri}},
        })
        reopened_symbols = receive(process)
        assert reopened_symbols["id"] == 8
        assert [item["name"] for item in reopened_symbols["result"]] == ["reopened"]

        send(process, {"jsonrpc": "2.0", "id": 9, "method": "textDocument/rename", "params": {}})
        unsupported = receive(process)
        assert unsupported["id"] == 9
        assert unsupported["error"]["code"] == -32601

        send(process, {"jsonrpc": "2.0", "id": 10, "method": "shutdown", "params": None})
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
