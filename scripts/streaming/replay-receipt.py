#!/usr/bin/env python3
"""Re-execute a successful CPU training receipt and compare its identity/result."""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
VERIFIER = Path(__file__).with_name("verify-event-chain.py")


def load_verifier():
    spec = importlib.util.spec_from_file_location("seloryn_event_chain", VERIFIER)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load verifier: {VERIFIER}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def read_receipt(
    path: Path, verifier, hmac_key: bytes | None
) -> tuple[list[str], list[dict], dict | None]:
    lines = [line for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]
    valid, message, _ = verifier.verify(lines, hmac_key)
    if not valid:
        raise ValueError(f"receipt is invalid: {message}")
    events = [json.loads(line) for line in lines]
    if events[0]["kind"] != "run.start" or events[-1]["kind"] != "run.end":
        raise ValueError("receipt does not contain a complete run envelope")
    if events[-1]["data"].get("status") != "ok":
        raise ValueError("replay currently accepts successful receipts only")
    identity = events[0]["data"].get("identity")
    if not isinstance(identity, dict) or identity.get("schema") != "seloryn.run-identity.v1":
        raise ValueError("receipt has no supported run identity")
    for field in ("program_sha256", "train_sha256", "test_sha256"):
        if not isinstance(identity.get(field), str) or len(identity[field]) != 64:
            raise ValueError(f"receipt identity is missing {field}")
    operations = [
        event["data"].get("operation")
        for event in events
        if event.get("kind") == "training.start"
    ]
    request = None
    if operations:
        if operations[0] not in {"train_linear", "train_mlp"}:
            raise ValueError("replay is limited to the bounded CPU training profiles")
    else:
        contracts = [
            event
            for event in events
            if event.get("kind") == "audit.record"
            and event["data"].get("audit_kind") == "training.contract"
        ]
        if len(contracts) != 1:
            raise ValueError(
                "replay requires a native training.start or one source-owned training.contract"
            )
        payload = contracts[0]["data"].get("payload")
        if not isinstance(payload, dict):
            raise ValueError("source-owned training contract has no object payload")
        request = payload.get("request")
        if not isinstance(request, dict) or request.get("mode") != "train":
            raise ValueError("source-owned replay requires a recorded train request")
        completions = [
            event
            for event in events
            if event.get("kind") == "audit.record"
            and event["data"].get("audit_kind") == "training.complete"
        ]
        if len(completions) != 1:
            raise ValueError("source-owned replay requires one training.complete event")
        completion_payload = completions[0]["data"].get("payload")
        if not isinstance(completion_payload, dict) or not isinstance(
            completion_payload.get("checkpoint_sha256"), str
        ):
            raise ValueError("source-owned replay requires a checkpoint completion hash")
        checkpoint_after = events[-1]["data"].get("checkpoint_after_sha256")
        if not isinstance(checkpoint_after, str) or checkpoint_after != completion_payload["checkpoint_sha256"]:
            raise ValueError("source-owned replay requires checkpoint identity in run.end")
    if not isinstance(events[-1]["data"].get("result_sha256"), str):
        raise ValueError("receipt has no result identity")
    return lines, events, request


def semantic_projection(events: list[dict]) -> list[tuple[str, dict]]:
    """Compare source event meaning while ignoring per-attempt chain metadata."""
    return [
        (event["kind"], event["data"])
        for event in events
        if event["kind"] not in {"run.start", "run.end"}
    ]


def remove_initial_checkpoints(project: Path) -> None:
    for directory in project.rglob("checkpoints"):
        if directory.is_dir():
            for candidate in directory.glob("*.json"):
                candidate.unlink()
    for candidate in project.rglob("checkpoint.json"):
        if candidate.is_file():
            candidate.unlink()


def checkpoint_hashes(project: Path) -> set[str]:
    hashes: set[str] = set()
    for candidate in project.rglob("*.json"):
        if candidate.name == "checkpoint.json" or candidate.parent.name == "checkpoints":
            hashes.add(hashlib.sha256(candidate.read_bytes()).hexdigest())
    return hashes


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--receipt", type=Path, required=True)
    parser.add_argument("--project", type=Path, required=True)
    parser.add_argument(
        "--binary",
        type=Path,
        default=ROOT / "target" / "release" / ("seloryn.exe" if sys.platform == "win32" else "seloryn"),
    )
    parser.add_argument("--hmac-key")
    args = parser.parse_args(argv)
    verifier = load_verifier()
    hmac_key = args.hmac_key.encode("utf-8") if args.hmac_key is not None else None
    try:
        _, original_events, request = read_receipt(args.receipt, verifier, hmac_key)
        original_start = original_events[0]["data"]
        original_identity = original_start["identity"]
        original_end = original_events[-1]["data"]
        if not args.binary.is_file():
            raise ValueError(f"binary not found: {args.binary}")
        if not args.project.is_dir():
            raise ValueError(f"project not found: {args.project}")

        with tempfile.TemporaryDirectory(prefix="seloryn-replay-") as directory:
            replay_project = Path(directory) / args.project.name
            shutil.copytree(args.project, replay_project)
            checkpoint_before = original_identity.get("checkpoint_before_sha256")
            if checkpoint_before:
                if checkpoint_before not in checkpoint_hashes(args.project):
                    raise ValueError(
                        "project does not contain the checkpoint snapshot recorded before the run"
                    )
            else:
                remove_initial_checkpoints(replay_project)
            environment = None
            if args.hmac_key is not None:
                environment = {"SELORYN_RECEIPT_KEY": args.hmac_key}
            process = subprocess.run(
                [str(args.binary), "run", str(replay_project), "--log=json2-signed" if args.hmac_key else "--log=json2"],
                cwd=ROOT,
                input="" if request is None else json.dumps(request, sort_keys=True),
                text=True,
                encoding="utf-8",
                errors="replace",
                capture_output=True,
                check=False,
                env=None if environment is None else {**__import__("os").environ, **environment},
            )
            if process.returncode != 0:
                raise ValueError(f"replay process failed with {process.returncode}: {process.stderr}")
            replay_lines = [line for line in process.stderr.splitlines() if line.strip()]
            valid, message, _ = verifier.verify(replay_lines, hmac_key)
            if not valid:
                raise ValueError(f"replay receipt is invalid: {message}")
            replay_events = [json.loads(line) for line in replay_lines]
            replay_identity = replay_events[0]["data"].get("identity")
            replay_end = replay_events[-1]["data"]
            if replay_identity != original_identity:
                raise ValueError("replay identity differs from the original receipt")
            for field in ("result_sha256", "checkpoint_after_sha256"):
                if original_end.get(field) != replay_end.get(field):
                    raise ValueError(f"replay {field} differs from the original receipt")
            if semantic_projection(original_events) != semantic_projection(replay_events):
                raise ValueError("replay source events differ from the original receipt")
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"Seloryn replay FAILED: {error}", file=sys.stderr)
        return 1
    print("Seloryn replay verified: program, datasets, result and checkpoint identity match")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
