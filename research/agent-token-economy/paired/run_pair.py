#!/usr/bin/env python3
"""Run TOKEN-PAIR-01 in Python and Seloryn, then count both transcripts.

The manifest, visible input and declared environment are shared.  The
Seloryn condition is intentionally allowed to expose unsupported metrics;
the runner records that capability gap instead of manufacturing BPE or byte
counts.  Token numbers are local ``tiktoken`` reconstructions of the visible
transcripts, never provider usage or billing.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.metadata
import importlib.util
import json
import platform
import subprocess
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[3]
PAIR_DIR = Path(__file__).resolve().parent
PYTHON_TASK = PAIR_DIR / "python_task.py"
SELY_TASK = PAIR_DIR / "seloryn_task.sely"
DEFAULT_MANIFEST = PAIR_DIR / "TOKEN-PAIR-01.manifest.json"
TRANSCRIPT_MODULE_PATH = ROOT / "research" / "agent-token-economy" / "transcript.py"
TOKEN_MODULE_PATH = ROOT / "research" / "agent-token-economy" / "token_count.py"
MANIFEST_SCHEMA = "seloryn.token-pair-task.v1"
TRANSCRIPT_SCHEMA = "seloryn.agent-visible-transcript.v1"
REPORT_SCHEMA = "seloryn.token-pair-report.v1"


def _load_module(name: str, path: Path) -> Any:
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise ImportError(f"could not load {name} from {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def canonical(value: Any) -> str:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


def load_manifest(path: Path) -> dict[str, Any]:
    document = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(document, dict) or document.get("schema") != MANIFEST_SCHEMA:
        raise ValueError(f"manifest schema must be {MANIFEST_SCHEMA}")
    if document.get("schema_version") != 1:
        raise ValueError("manifest schema_version must be 1")
    for field in ("run_id", "task_id", "prompt"):
        if not isinstance(document.get(field), str) or not document[field]:
            raise ValueError(f"manifest {field} is required")
    if not isinstance(document.get("inputs"), list) or not document["inputs"]:
        raise ValueError("manifest inputs must be a non-empty array")
    if not isinstance(document.get("environment", {}), dict):
        raise ValueError("manifest environment must be an object")
    return document


def visible_input(manifest: dict[str, Any]) -> str:
    task_input = {
        "environment": manifest.get("environment", {}),
        "inputs": manifest["inputs"],
    }
    return f'{manifest["prompt"]}\n\nInput JSON:\n{canonical(task_input)}'


def manifest_sha256(manifest: dict[str, Any]) -> str:
    return hashlib.sha256(canonical(manifest).encode("utf-8")).hexdigest()


def run_seloryn(binary: Path) -> str:
    command = [str(binary), "run", str(SELY_TASK), "--format=json"]
    completed = subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        encoding="utf-8",
        capture_output=True,
        check=False,
    )
    if completed.returncode != 0:
        raise RuntimeError(f"Seloryn condition failed with {completed.returncode}: {completed.stderr.strip()}")
    if completed.stderr:
        raise RuntimeError(f"Seloryn condition wrote stderr: {completed.stderr.strip()}")
    output = completed.stdout.strip()
    if not output:
        raise RuntimeError("Seloryn condition returned empty stdout")
    try:
        json.loads(output)
    except json.JSONDecodeError as error:
        raise RuntimeError(f"Seloryn condition did not return JSON: {output!r}") from error
    return output


def seloryn_transcript(manifest: dict[str, Any], visible_output: str) -> dict[str, Any]:
    return {
        "schema": TRANSCRIPT_SCHEMA,
        "run_id": manifest["run_id"],
        "task_id": manifest["task_id"],
        "condition": "seloryn",
        "capture_scope": "shared_manifest_prompt_and_seloryn_stdout_only",
        "manifest_sha256": manifest_sha256(manifest),
        "messages": [
            {"role": "user", "text": visible_input(manifest)},
            {"role": "assistant", "text": visible_output},
        ],
        "not_provider_billing": True,
        "limitations": [
            "The Seloryn condition currently supports Unicode codepoint length only for this task.",
            "Seloryn does not yet implement OpenAI-compatible BPE tokenization, provider billing or hidden-context telemetry.",
            "The transcript captures the declared prompt and process stdout, not tool serialization or provider usage.",
        ],
    }


def run_pair(manifest_path: Path, binary: Path, *, model: str | None, encoding: str | None) -> dict[str, Any]:
    manifest = load_manifest(manifest_path)
    python_task = _load_module("seloryn_python_pair_task", PYTHON_TASK)
    sys.modules["token_count"] = _load_module("token_count", TOKEN_MODULE_PATH)
    transcript = _load_module("seloryn_pair_transcript", TRANSCRIPT_MODULE_PATH)
    python_document = python_task.run(manifest)
    if python_document["messages"][0]["text"] != visible_input(manifest):
        raise ValueError("Python condition diverged from the shared visible input")

    seloryn_document = seloryn_transcript(manifest, run_seloryn(binary))
    if seloryn_document["messages"][0]["text"] != python_document["messages"][0]["text"]:
        raise ValueError("paired conditions do not share the same visible input")

    python_count = transcript.measure(python_document, encoding=encoding, model=model)
    seloryn_count = transcript.measure(seloryn_document, encoding=encoding, model=model)
    tokenizer = python_count["tokenizer"]
    return {
        "schema": REPORT_SCHEMA,
        "schema_version": 1,
        "run_id": manifest["run_id"],
        "task_id": manifest["task_id"],
        "evidence_class": "paired-visible-transcript-local-reconstruction",
        "not_provider_billing": True,
        "manifest_sha256": manifest_sha256(manifest),
        "declared_environment": manifest.get("environment", {}),
        "observed_environment": {
            "python": platform.python_version(),
            "platform": platform.platform(),
            "tiktoken": importlib.metadata.version("tiktoken"),
            "seloryn_binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        },
        "tokenizer": tokenizer,
        "conditions": {
            "python": {
                "transcript": python_document,
                "token_count": python_count,
            },
            "seloryn": {
                "transcript": seloryn_document,
                "token_count": seloryn_count,
            },
        },
        "parity": {
            "same_manifest": True,
            "same_prompt": True,
            "same_declared_environment": True,
            "same_tokenizer": python_count["tokenizer"] == seloryn_count["tokenizer"],
            "functionally_equivalent": False,
            "equivalence_boundary": "Seloryn reports codepoint support and explicit BPE/byte limitations; Python reports all declared local text metrics.",
        },
        "interpretation": {
            "token_economy_claim": "not_demonstrated",
            "provider_usage": "unknown",
            "cost": "unknown",
            "productivity": "unknown",
            "purpose": "prove shared-input measurement and expose the current Seloryn tokenization boundary",
        },
    }


def write_outputs(report: dict[str, Any], output_dir: Path) -> None:
    output_dir.mkdir(parents=True, exist_ok=True)
    base = report["run_id"]
    python_data = report["conditions"]["python"]
    seloryn_data = report["conditions"]["seloryn"]
    files = {
        f"{base}.python.transcript.json": python_data["transcript"],
        f"{base}.seloryn.transcript.json": seloryn_data["transcript"],
        f"{base}.python.token-count.json": python_data["token_count"],
        f"{base}.seloryn.token-count.json": seloryn_data["token_count"],
        f"{base}.pair-report.json": report,
    }
    for name, document in files.items():
        (output_dir / name).write_text(json.dumps(document, ensure_ascii=False, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--binary", type=Path, default=ROOT / "target" / "release" / ("seloryn.exe" if sys.platform == "win32" else "seloryn"))
    parser.add_argument("--model", default="gpt-4o")
    parser.add_argument("--encoding")
    parser.add_argument("--output-dir", type=Path)
    args = parser.parse_args(argv)
    try:
        if not args.binary.is_file():
            raise ValueError(f"release Seloryn binary is required at {args.binary}")
        if args.model and args.encoding:
            raise ValueError("choose --model or --encoding, not both")
        report = run_pair(args.manifest, args.binary, model=args.model, encoding=args.encoding)
        if args.output_dir:
            write_outputs(report, args.output_dir)
            print(f"wrote paired report to {args.output_dir}")
        else:
            print(json.dumps(report, ensure_ascii=False, indent=2, sort_keys=True))
    except (OSError, ValueError, RuntimeError, KeyError) as error:
        print(f"TOKEN-PAIR-01: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
