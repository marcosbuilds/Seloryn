"""Bounded operational reliability evidence for the current release binary.

The probe is intentionally black-box and standard-library-only.  It creates a
fresh copy of one deterministic project for every child process, captures
stdout and JSON2 stderr to files instead of pipes, and checks the observable
result, receipt lifecycle, checkpoint and workspace boundary.
"""

from __future__ import annotations

import copy
import hashlib
import json
import re
import subprocess
import sys
import tempfile
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from typing import Iterable, NamedTuple


ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target" / "release" / ("seloryn.exe" if sys.platform == "win32" else "seloryn")

NORMAL_PROCESSES = 4
NORMAL_STEPS = 640
TERMINATION_STEPS = 700_000
PROCESS_TIMEOUT_SECONDS = 45
TERMINATION_OBSERVE_SECONDS = 15

ZERO_HASH = "sha256:" + "0" * 64
HASH_PATTERN = re.compile(r"^sha256:[0-9a-f]{64}$")
RUN_PATTERN = re.compile(r"^r_[0-9a-f]{64}$")
REQUIRED_RECEIPT_FIELDS = ("schema", "run", "seq", "kind", "phase", "data", "prev", "hash")

TRAIN_JSON = (
    '{"inputs":[[-2,-2],[-2,0],[-2,2],[0,-2],[0,0],[0,2],[2,-2],[2,0],[2,2]],'
    '"targets":[[-5.5],[-1.5],[2.5],[-3.5],[0.5],[4.5],[-1.5],[2.5],[6.5]]}\n'
)
TEST_JSON = (
    '{"inputs":[[-1,-1],[-1,1],[1,-1],[1,1],[3,0]],'
    '"targets":[[-2.5],[1.5],[-0.5],[3.5],[3.5]]}\n'
)


class RawNumber(str):
    """Preserve JSON number spelling for the JSON2 hash algorithm."""


class DuplicateKeyError(ValueError):
    pass


class ProcessResult(NamedTuple):
    returncode: int
    stdout: str
    stderr: str
    events: list[dict[str, object]]


def reject_duplicate_keys(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        if key in result:
            raise DuplicateKeyError(f"duplicate key {key!r}")
        result[key] = value
    return result


def canonical(value: object) -> bytes:
    def render(item: object) -> str:
        if isinstance(item, RawNumber):
            return item
        if item is None:
            return "null"
        if item is True:
            return "true"
        if item is False:
            return "false"
        if isinstance(item, str):
            return json.dumps(item, ensure_ascii=False, allow_nan=False, separators=(",", ":"))
        if isinstance(item, list):
            return "[" + ",".join(render(child) for child in item) + "]"
        if isinstance(item, dict):
            return "{" + ",".join(
                render(str(key)) + ":" + render(child)
                for key, child in sorted(item.items())
            ) + "}"
        raise TypeError(f"unsupported JSON value: {type(item).__name__}")

    return render(value).encode("utf-8")


def digest(value: object) -> str:
    return "sha256:" + hashlib.sha256(canonical(value)).hexdigest()


def parse_json_object(line: str) -> dict[str, object]:
    value = json.loads(
        line,
        object_pairs_hook=reject_duplicate_keys,
        parse_int=RawNumber,
        parse_float=RawNumber,
    )
    if not isinstance(value, dict):
        raise AssertionError("receipt line is not a JSON object")
    return value


def verify_receipt(lines: Iterable[str], require_end: bool) -> list[dict[str, object]]:
    """Verify JSON2 structure and hashes, optionally accepting a killed prefix."""

    previous = ZERO_HASH
    run_id: str | None = None
    events: list[dict[str, object]] = []
    for line_number, raw in enumerate(lines, 1):
        if not raw.strip():
            continue
        try:
            event = parse_json_object(raw)
        except (DuplicateKeyError, json.JSONDecodeError) as error:
            raise AssertionError(f"receipt line {line_number} is invalid: {error}") from error
        missing = [key for key in REQUIRED_RECEIPT_FIELDS if key not in event]
        if missing:
            raise AssertionError(f"receipt line {line_number} is missing {missing}")
        if event["schema"] != RawNumber("2"):
            raise AssertionError(f"receipt line {line_number} has schema {event['schema']!r}")
        if not isinstance(event["run"], str) or not RUN_PATTERN.fullmatch(event["run"]):
            raise AssertionError(f"receipt line {line_number} has an invalid run identity")
        if run_id is None:
            run_id = event["run"]
        if event["run"] != run_id:
            raise AssertionError(f"receipt line {line_number} changes run identity")
        expected_sequence = RawNumber(str(len(events)))
        if event["seq"] != expected_sequence:
            raise AssertionError(
                f"receipt line {line_number} has seq {event['seq']!r}; expected {expected_sequence!r}"
            )
        if not isinstance(event["kind"], str) or not event["kind"]:
            raise AssertionError(f"receipt line {line_number} has an invalid kind")
        if event["phase"] != "run" or not isinstance(event["data"], dict):
            raise AssertionError(f"receipt line {line_number} has invalid phase/data")
        if event["prev"] != previous or not isinstance(event["prev"], str) or not HASH_PATTERN.fullmatch(event["prev"]):
            raise AssertionError(f"receipt line {line_number} has a broken prev chain")
        claimed_hash = event["hash"]
        if not isinstance(claimed_hash, str) or not HASH_PATTERN.fullmatch(claimed_hash):
            raise AssertionError(f"receipt line {line_number} has an invalid hash")
        content = copy.deepcopy(event)
        del content["hash"]
        signature = content.pop("signature", None)
        if signature is not None:
            raise AssertionError("unexpected signed receipt in the unsigned probe")
        if digest(content) != claimed_hash:
            raise AssertionError(f"receipt line {line_number} hash does not verify")
        if events and events[-1]["kind"] == "run.end":
            raise AssertionError("receipt contains an event after run.end")
        if event["kind"] == "run.end":
            data = event["data"]
            if data.get("status") not in ("ok", "error"):
                raise AssertionError("run.end has an invalid status")
            if data.get("events_before_end") != RawNumber(str(len(events))):
                raise AssertionError("run.end event count is inconsistent")
            if data.get("last_event_hash") != previous:
                raise AssertionError("run.end last_event_hash is inconsistent")
        events.append(event)
        previous = claimed_hash

    if not events or events[0]["kind"] != "run.start":
        raise AssertionError("receipt does not start with run.start")
    if require_end:
        if events[-1]["kind"] != "run.end":
            raise AssertionError("receipt does not end with run.end")
        if len(events) < 3:
            raise AssertionError("receipt has no source lifecycle event")
    elif events[-1]["kind"] == "run.end":
        raise AssertionError("terminated process unexpectedly emitted run.end")
    return events


def create_project(root: Path, steps: int) -> None:
    (root / "src").mkdir(parents=True)
    (root / "data").mkdir()
    (root / "checkpoints").mkdir()
    (root / "seloryn.toml").write_text(
        """[package]
name = "operational-reliability-probe"
language = "0.02"
source_root = "src"
entry = "main.sely"

[capabilities]
read = ["data"]
write = ["checkpoints"]

[limits]
json_max_bytes = 1048576
json_max_depth = 32
""",
        encoding="utf-8",
        newline="\n",
    )
    (root / "data" / "train.json").write_text(TRAIN_JSON, encoding="utf-8", newline="\n")
    (root / "data" / "test.json").write_text(TEST_JSON, encoding="utf-8", newline="\n")
    (root / "src" / "main.sely").write_text(
        "from training import train_mlp\n\n"
        "fn main() -> Json:\n"
        f'    return train_mlp("data/train.json", "data/test.json", '
        f'"checkpoints/model.json", learning_rate=0.02, hidden_size=8, '
        f'batch_size=3, steps={steps}, seed=17)\n',
        encoding="utf-8",
        newline="\n",
    )


def capture_process(workspace: Path, timeout: float = PROCESS_TIMEOUT_SECONDS) -> ProcessResult:
    stdout_path = workspace / "probe.stdout"
    receipt_path = workspace / "probe.receipt.ndjson"
    stdout_handle = stdout_path.open("wb")
    receipt_handle = receipt_path.open("wb")
    try:
        process = subprocess.Popen(
            [str(BINARY), "run", str(workspace), "--format=json", "--log=json2"],
            cwd=workspace,
            stdin=subprocess.DEVNULL,
            stdout=stdout_handle,
            stderr=receipt_handle,
            close_fds=False,
        )
    finally:
        stdout_handle.close()
        receipt_handle.close()
    try:
        returncode = process.wait(timeout=timeout)
    except subprocess.TimeoutExpired as error:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)
        raise AssertionError(f"normal run exceeded {timeout:g}s in {workspace}") from error
    stdout = stdout_path.read_text(encoding="utf-8")
    stderr = receipt_path.read_text(encoding="utf-8")
    events = verify_receipt(stderr.splitlines(), require_end=True)
    return ProcessResult(returncode, stdout, stderr, events)


def relative_files(root: Path) -> list[str]:
    root_resolved = root.resolve()
    found: list[str] = []
    for path in root.rglob("*"):
        resolved = path.resolve(strict=False)
        try:
            resolved.relative_to(root_resolved)
        except ValueError as error:
            raise AssertionError(f"workspace entry escapes workspace: {path} -> {resolved}") from error
        if path.is_file() or path.is_symlink():
            found.append(path.relative_to(root).as_posix())
    return sorted(found)


def stable_receipt(events: list[dict[str, object]]) -> list[dict[str, object]]:
    """Drop only per-attempt and hash-chain fields before cross-process comparison."""

    stable = copy.deepcopy(events)
    for event in stable:
        event.pop("hash", None)
        event.pop("prev", None)
        event.pop("signature", None)
        data = event.get("data")
        if isinstance(data, dict):
            if event.get("kind") == "run.start":
                data.pop("attempt_id", None)
            if event.get("kind") == "run.end":
                data.pop("last_event_hash", None)
    return stable


def assert_normal_result(workspace: Path, result: ProcessResult) -> None:
    if result.returncode != 0:
        raise AssertionError(f"normal process exited {result.returncode}: {result.stderr}")
    envelope = json.loads(result.stdout)
    if not isinstance(envelope, dict) or set(envelope) != {"stdout", "value"}:
        raise AssertionError("run JSON envelope is not exactly {stdout,value}")
    if envelope["stdout"] != "":
        raise AssertionError("probe project unexpectedly wrote program stdout")
    start = result.events[0]
    if start["kind"] != "run.start" or start["data"].get("attempt_id") is None:
        raise AssertionError("receipt run.start has no attempt_id")
    training_start = next(event for event in result.events if event["kind"] == "training.start")
    if training_start["data"].get("resumed_from") != RawNumber("0"):
        raise AssertionError("normal run was not fresh")
    for event in result.events:
        data = event.get("data")
        if isinstance(data, dict):
            for key in ("train_path", "test_path", "checkpoint_path"):
                if key in data and not str(data[key]).startswith("path_sha256:"):
                    raise AssertionError(f"receipt path field {key} was not redacted")
    if str(workspace) in result.stdout or str(workspace) in result.stderr:
        raise AssertionError("absolute workspace path leaked into process output")
    if not (workspace / "checkpoints" / "model.json").is_file():
        raise AssertionError("successful run did not produce its workspace checkpoint")


def run_termination_case(workspace: Path) -> dict[str, object]:
    if sys.platform != "win32":
        return {
            "status": "skipped",
            "reason": "forced process termination is only classified as reliable by this probe on Windows",
        }

    stdout_path = workspace / "probe.stdout"
    receipt_path = workspace / "probe.receipt.ndjson"
    stdout_handle = stdout_path.open("wb")
    receipt_handle = receipt_path.open("wb")
    try:
        process = subprocess.Popen(
            [str(BINARY), "run", str(workspace), "--format=json", "--log=json2"],
            cwd=workspace,
            stdin=subprocess.DEVNULL,
            stdout=stdout_handle,
            stderr=receipt_handle,
            close_fds=False,
        )
    finally:
        stdout_handle.close()
        receipt_handle.close()

    observed = False
    deadline = time.monotonic() + TERMINATION_OBSERVE_SECONDS
    while time.monotonic() < deadline:
        if process.poll() is not None:
            break
        try:
            receipt = receipt_path.read_text(encoding="utf-8")
        except PermissionError:
            receipt = ""
        if '"kind":"training.step"' in receipt:
            observed = True
            break
        time.sleep(0.01)

    if not observed or process.poll() is not None:
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=10)
        raise AssertionError("termination case did not observe a live training step")

    termination_started = time.monotonic()
    process.terminate()
    try:
        returncode = process.wait(timeout=10)
    except subprocess.TimeoutExpired as error:
        process.kill()
        process.wait(timeout=10)
        raise AssertionError("Windows process termination did not complete within 10s") from error
    termination_elapsed = time.monotonic() - termination_started
    stdout = stdout_path.read_text(encoding="utf-8")
    stderr = receipt_path.read_text(encoding="utf-8")
    lines = [line for line in stderr.splitlines() if line.strip()]
    events = verify_receipt(lines, require_end=False)
    if returncode == 0:
        raise AssertionError("terminated process returned success")
    if '"kind":"run.end"' in stderr:
        raise AssertionError("terminated process emitted run.end; termination was not bounded")
    if stdout:
        raise AssertionError("terminated process emitted a successful JSON result")
    temporary_files = [
        name
        for name in relative_files(workspace)
        if ".tmp-" in name or name.endswith(".tmp")
    ]
    return {
        "status": "terminated",
        "returncode": returncode,
        "termination_seconds": round(termination_elapsed, 3),
        "receipt_prefix_events": len(events),
        "run_end_emitted": False,
        "stdout_empty": True,
        "temporary_files_after_termination": temporary_files,
        "graceful_cancellation_proven": False,
    }


def main() -> int:
    if not BINARY.is_file():
        raise AssertionError(f"release binary is missing: {BINARY}")

    with tempfile.TemporaryDirectory(prefix="seloryn-operational-reliability-") as temporary:
        root = Path(temporary)
        normal_workspaces = [root / f"normal-{index}" for index in range(NORMAL_PROCESSES)]
        for workspace in normal_workspaces:
            create_project(workspace, NORMAL_STEPS)

        with ThreadPoolExecutor(max_workers=NORMAL_PROCESSES) as executor:
            results = list(
                executor.map(
                    capture_process,
                    normal_workspaces,
                )
            )
        for workspace, result in zip(normal_workspaces, results):
            assert_normal_result(workspace, result)

        stdout_values = [result.stdout for result in results]
        receipt_values = [result.stderr for result in results]
        stable_values = [stable_receipt(result.events) for result in results]
        checkpoint_values = [
            (workspace / "checkpoints" / "model.json").read_bytes()
            for workspace in normal_workspaces
        ]
        if len(set(stdout_values)) != 1:
            raise AssertionError("concurrent JSON outputs differ")
        if len({json.dumps(value, sort_keys=True, separators=(",", ":")) for value in stable_values}) != 1:
            raise AssertionError("deterministic JSON2 receipt projections differ")
        if len(set(checkpoint_values)) != 1:
            raise AssertionError("concurrent checkpoints differ")
        if any(len(value) == 0 for value in receipt_values):
            raise AssertionError("a concurrent process emitted an empty receipt")

        allowed = {
            "seloryn.toml",
            "src/main.sely",
            "data/train.json",
            "data/test.json",
            "checkpoints/model.json",
            "probe.stdout",
            "probe.receipt.ndjson",
        }
        for workspace in normal_workspaces:
            actual = set(relative_files(workspace))
            if actual != allowed:
                raise AssertionError(f"workspace file set differs: {workspace}: {sorted(actual)}")
        expected_root_entries = {workspace.name for workspace in normal_workspaces}
        actual_root_entries = {path.name for path in root.iterdir()}
        if actual_root_entries != expected_root_entries:
            raise AssertionError(
                f"processes created entries outside their workspaces: {sorted(actual_root_entries)}"
            )

        termination_workspace = root / "termination"
        create_project(termination_workspace, TERMINATION_STEPS)
        termination = run_termination_case(termination_workspace)
        expected_root_entries.add(termination_workspace.name)
        actual_root_entries = {path.name for path in root.iterdir()}
        if actual_root_entries != expected_root_entries:
            raise AssertionError(
                f"termination process created entries outside its workspace: {sorted(actual_root_entries)}"
            )
        all_workspaces_confined = all(
            set(relative_files(workspace)).issubset(allowed)
            for workspace in (*normal_workspaces, termination_workspace)
        )
        if not all_workspaces_confined:
            raise AssertionError("a probe process created an out-of-contract workspace entry")

        raw_receipts_identical = len(set(receipt_values)) == 1
        attempt_ids = [
            str(result.events[0]["data"].get("attempt_id"))
            for result in results
        ]
        summary = {
            "binary": str(BINARY),
            "normal_processes": NORMAL_PROCESSES,
            "normal_steps": NORMAL_STEPS,
            "normal_exit_codes": [result.returncode for result in results],
            "json_output_identical": len(set(stdout_values)) == 1,
            "json2_receipt_chains_valid": True,
            "json2_receipt_projection_identical": True,
            "raw_json2_receipts_identical": raw_receipts_identical,
            "attempt_ids_unique": len(set(attempt_ids)) == len(attempt_ids),
            "checkpoint_bytes_identical": len(set(checkpoint_values)) == 1,
            "workspace_confined": all_workspaces_confined,
            "termination": termination,
            "limitations": [
                "JSON2 raw receipt bytes include a per-attempt PID/time identifier and its dependent hash chain; only the verified semantic projection is deterministic.",
                "Forced Windows termination is not graceful cancellation and no cancellation cleanup guarantee is claimed.",
            ],
        }
        print(json.dumps(summary, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (AssertionError, OSError, subprocess.SubprocessError, UnicodeError, json.JSONDecodeError) as error:
        print(f"operational reliability probe FAILED: {error!r}", file=sys.stderr)
        raise SystemExit(1) from error
