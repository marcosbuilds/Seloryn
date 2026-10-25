"""A bounded, dependency-free bridge to ``seloryn run --format=json``.

The bridge intentionally starts one Seloryn process per request. It does not
invoke a shell, keep a daemon, or claim to fence descendants. The transport
contract is the public CLI JSON result envelope: ``{"stdout", "value"}`` on
success and the structured diagnostic envelope on failure.
"""

from __future__ import annotations

from dataclasses import dataclass
import json
import os
from pathlib import Path
import subprocess
import threading
import time
from typing import Any, Mapping, Sequence


PROTOCOL_VERSION = "seloryn-python-bridge.v1"
DEFAULT_TIMEOUT_SECONDS = 30.0
DEFAULT_INPUT_LIMIT = 1_048_576
DEFAULT_OUTPUT_LIMIT = 4 * 1_048_576
VALID_LOGS = frozenset({"off", "human", "json", "json2", "json2-signed"})


@dataclass(frozen=True)
class BridgeResult:
    """Successful response from one Seloryn process."""

    value: Any
    stdout: str
    events: tuple[Mapping[str, Any], ...]
    protocol: str = PROTOCOL_VERSION


class BridgeError(RuntimeError):
    """A bounded transport, process, or Seloryn diagnostic failure."""

    def __init__(
        self,
        message: str,
        *,
        code: str,
        exit_code: int | None,
        stderr: str = "",
        stdout: str = "",
        timed_out: bool = False,
    ) -> None:
        super().__init__(message)
        self.code = code
        self.exit_code = exit_code
        self.stderr = stderr
        self.stdout = stdout
        self.timed_out = timed_out


class _Capture:
    def __init__(self, limit: int) -> None:
        self.limit = limit
        self.data = bytearray()
        self.overflow = False


def _read_limited(stream: Any, capture: _Capture) -> None:
    try:
        while True:
            chunk = stream.read(64 * 1024)
            if not chunk:
                return
            remaining = capture.limit - len(capture.data)
            if len(chunk) > remaining:
                if remaining > 0:
                    capture.data.extend(chunk[:remaining])
                capture.overflow = True
                return
            capture.data.extend(chunk)
    finally:
        stream.close()


def _write_input(stream: Any, payload: bytes, error: list[BaseException]) -> None:
    try:
        if payload:
            stream.write(payload)
            stream.flush()
    except BaseException as exc:  # the parent reports a typed process error
        error.append(exc)
    finally:
        stream.close()


def _diagnostic_code(stderr: str) -> str:
    try:
        document = json.loads(stderr)
    except (json.JSONDecodeError, TypeError):
        return "SEL-BRIDGE-PROCESS"
    if isinstance(document, dict):
        direct = document.get("code")
        if isinstance(direct, str):
            return direct
        diagnostics = document.get("diagnostics")
        if isinstance(diagnostics, list) and diagnostics:
            first = diagnostics[0]
            if isinstance(first, dict) and isinstance(first.get("code"), str):
                return first["code"]
    return "SEL-BRIDGE-PROCESS"


def _json_events(stderr: str) -> tuple[Mapping[str, Any], ...]:
    events: list[Mapping[str, Any]] = []
    for line in stderr.splitlines():
        if not line.strip():
            continue
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(event, dict):
            events.append(event)
    return tuple(events)


class SelorynBridge:
    """Invoke a Seloryn binary through its bounded JSON/stdio contract."""

    def __init__(
        self,
        binary: str | os.PathLike[str],
        *,
        cwd: str | os.PathLike[str] | None = None,
        env: Mapping[str, str] | None = None,
        timeout: float = DEFAULT_TIMEOUT_SECONDS,
        input_limit: int = DEFAULT_INPUT_LIMIT,
        output_limit: int = DEFAULT_OUTPUT_LIMIT,
    ) -> None:
        if timeout <= 0:
            raise ValueError("timeout must be positive")
        if input_limit <= 0 or output_limit <= 0:
            raise ValueError("input and output limits must be positive")
        self.binary = os.fspath(binary)
        self.cwd = os.fspath(cwd) if cwd is not None else None
        self.env = dict(env) if env is not None else None
        self.timeout = timeout
        self.input_limit = input_limit
        self.output_limit = output_limit

    def run(
        self,
        program: str | os.PathLike[str],
        *,
        input: Any = None,
        log: str = "off",
    ) -> BridgeResult:
        """Run one project/source and return its structured result.

        ``input`` is encoded as one UTF-8 JSON document. A missing input is an
        empty stdin stream, so programs that import ``json.read_stdin`` will
        receive Seloryn's normal bounded input diagnostic.
        """

        if log not in VALID_LOGS:
            raise ValueError(f"unsupported log format: {log}")
        payload = json.dumps(
            input,
            ensure_ascii=False,
            allow_nan=False,
            separators=(",", ":"),
        ).encode("utf-8") if input is not None else b""
        if len(payload) > self.input_limit:
            raise BridgeError(
                "JSON request exceeds the bridge input limit",
                code="SEL-BRIDGE-INPUT_LIMIT",
                exit_code=None,
            )

        completed = self._invoke(
            ["run", os.fspath(program), "--format=json", f"--log={log}"],
            payload,
        )
        if completed.returncode != 0:
            raise BridgeError(
                "Seloryn rejected the request",
                code=_diagnostic_code(completed.stderr),
                exit_code=completed.returncode,
                stderr=completed.stderr,
                stdout=completed.stdout,
            )
        try:
            document = json.loads(completed.stdout)
        except json.JSONDecodeError as exc:
            raise BridgeError(
                "Seloryn returned invalid JSON",
                code="SEL-BRIDGE-PROTOCOL",
                exit_code=completed.returncode,
                stderr=completed.stderr,
                stdout=completed.stdout,
            ) from exc
        if (
            not isinstance(document, dict)
            or not isinstance(document.get("stdout"), str)
            or "value" not in document
        ):
            raise BridgeError(
                "Seloryn JSON result is missing the {stdout,value} envelope",
                code="SEL-BRIDGE-PROTOCOL",
                exit_code=completed.returncode,
                stderr=completed.stderr,
                stdout=completed.stdout,
            )
        return BridgeResult(
            value=document["value"],
            stdout=document["stdout"],
            events=_json_events(completed.stderr),
        )

    def check(self, program: str | os.PathLike[str]) -> None:
        """Validate a project/source without executing its program."""

        completed = self._invoke(
            ["check", os.fspath(program), "--format=json", "--quiet"],
            b"",
        )
        if completed.returncode != 0:
            raise BridgeError(
                "Seloryn rejected the program during check",
                code=_diagnostic_code(completed.stderr),
                exit_code=completed.returncode,
                stderr=completed.stderr,
                stdout=completed.stdout,
            )

    def _invoke(self, arguments: Sequence[str], payload: bytes) -> subprocess.CompletedProcess[str]:
        environment = os.environ.copy()
        if self.env is not None:
            environment.update(self.env)
        try:
            process = subprocess.Popen(
                [self.binary, *arguments],
                cwd=self.cwd,
                env=environment,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                shell=False,
            )
        except OSError as exc:
            raise BridgeError(
                f"could not start Seloryn: {exc}",
                code="SEL-BRIDGE-START",
                exit_code=None,
            ) from exc

        stdout_capture = _Capture(self.output_limit)
        stderr_capture = _Capture(self.output_limit)
        writer_error: list[BaseException] = []
        threads = [
            threading.Thread(
                target=_read_limited,
                args=(process.stdout, stdout_capture),
                daemon=True,
            ),
            threading.Thread(
                target=_read_limited,
                args=(process.stderr, stderr_capture),
                daemon=True,
            ),
            threading.Thread(
                target=_write_input,
                args=(process.stdin, payload, writer_error),
                daemon=True,
            ),
        ]
        for thread in threads:
            thread.start()

        timed_out = False
        overflow = False
        deadline = time.monotonic() + self.timeout
        while process.poll() is None:
            overflow = stdout_capture.overflow or stderr_capture.overflow
            if overflow:
                process.kill()
                break
            if time.monotonic() >= deadline:
                timed_out = True
                process.kill()
                break
            time.sleep(0.005)
        returncode = process.wait()
        for thread in threads:
            thread.join(timeout=1.0)
        overflow = overflow or stdout_capture.overflow or stderr_capture.overflow
        stdout = bytes(stdout_capture.data).decode("utf-8", errors="replace")
        stderr = bytes(stderr_capture.data).decode("utf-8", errors="replace")
        if timed_out:
            raise BridgeError(
                "Seloryn process exceeded the bridge timeout",
                code="SEL-BRIDGE-TIMEOUT",
                exit_code=returncode,
                stderr=stderr,
                stdout=stdout,
                timed_out=True,
            )
        if overflow:
            raise BridgeError(
                "Seloryn output exceeds the bridge output limit",
                code="SEL-BRIDGE-OUTPUT_LIMIT",
                exit_code=returncode,
                stderr=stderr,
                stdout=stdout,
            )
        if writer_error and returncode == 0:
            raise BridgeError(
                f"could not write Seloryn stdin: {writer_error[0]}",
                code="SEL-BRIDGE-INPUT",
                exit_code=returncode,
                stderr=stderr,
                stdout=stdout,
            )
        return subprocess.CompletedProcess(
            args=[self.binary, *arguments],
            returncode=returncode,
            stdout=stdout,
            stderr=stderr,
        )
