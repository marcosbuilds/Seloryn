# Operational reliability evidence probe

This is a bounded black-box probe of the current Seloryn release binary. It
does not change the Rust runtime, does not modify an existing fixture, and does
not claim general production reliability. The probe creates one fresh temporary
workspace per child process, runs the same deterministic `.sely` MLP project in
four concurrent processes, and captures stdout and JSON2 stderr to files
instead of pipes. File-backed capture keeps the test independent of pipe
buffer size and avoids producer/consumer deadlocks.

## Reproduction

From the repository root, with the release binary already built:

```text
python tests/operational_reliability_probe.py
```

The binary path follows the existing probe convention:

```text
target/release/seloryn.exe   # Windows
target/release/seloryn      # non-Windows
```

The project fixture is generated inside a `TemporaryDirectory`. Each child
gets the same manifest, source, datasets, training seed, model dimensions and
640-step budget, but a different fresh workspace and no pre-existing
checkpoint. The successful command is:

```text
seloryn run <fresh-workspace> --format=json --log=json2
```

The probe independently verifies the JSON2 schema, duplicate-key rejection,
sequence, hash chain, lifecycle (`run.start` through `run.end`), redacted path
fields, exit code, JSON result envelope, deterministic checkpoint bytes and
the exact workspace file set. It also verifies that no captured output contains
the temporary absolute workspace path.

## Observed run

Executed on 2026-10-16 from `D:\Project\Seloryn\seloryn`:

```text
python --version
Python 3.10.0

target\release\seloryn.exe --version
seloryn 0.09 package=0.9.4 build=local-development status=verified-preview

SHA-256(target\release\seloryn.exe)
48EC29DFC766ACCFE9A1D5C971295C241D18529CA7AA74615A1E133BC82E1488

python tests/operational_reliability_probe.py
```

Observed result from the current Windows release binary:

```text
{"attempt_ids_unique":true,"binary":"D:\\Project\\Seloryn\\seloryn\\target\\release\\seloryn.exe","checkpoint_bytes_identical":true,"json2_receipt_chains_valid":true,"json2_receipt_projection_identical":true,"json_output_identical":true,"limitations":["JSON2 raw receipt bytes include a per-attempt PID/time identifier and its dependent hash chain; only the verified semantic projection is deterministic.","Forced Windows termination is not graceful cancellation and no cancellation cleanup guarantee is claimed."],"normal_exit_codes":[0,0,0,0],"normal_processes":4,"normal_steps":640,"raw_json2_receipts_identical":false,"termination":{"graceful_cancellation_proven":false,"receipt_prefix_events":3,"returncode":1,"run_end_emitted":false,"status":"terminated","stdout_empty":true,"temporary_files_after_termination":[],"termination_seconds":0.0},"workspace_confined":true}
```

The probe's JSON summary is the authoritative raw result for this run. Record
its reported binary path, exit-code list, receipt checks, checkpoint comparison,
workspace confinement and termination result together when reviewing the
evidence.

## Interpretation and limitations

- The normal case proves four concurrent fresh processes returned exit code
  `0`, emitted identical JSON result envelopes, produced valid JSON2 receipt
  chains with identical deterministic semantic content, and wrote identical
  checkpoints confined to their own workspaces.
- The runtime's JSON2 receipt includes `attempt_id`, derived from process ID and
  wall-clock time. That field changes between fresh processes and changes the
  dependent receipt hashes. Therefore the probe reports raw receipt byte
  equality separately and does not claim byte-for-byte JSON2 determinism. It
  compares the semantic projection after removing only `attempt_id`, the
  hash-chain fields and `run.end.data.last_event_hash`, while verifying the
  original chain in full for every process.
- On Windows, the interruption case starts the same project with a bounded
  `700000`-step budget, waits until a live `training.step` receipt is observed,
  then calls `Popen.terminate()` and waits at most ten seconds. This is forced
  process termination, not a runtime cancellation protocol. The probe requires
  a nonzero exit, an empty successful-result stream and no `run.end`; it does
  not claim graceful cancellation, resumability, or cleanup after termination.
- The interruption prefix is checked for valid JSON2 chaining, and any
  temporary checkpoint files are reported. A checkpoint observed after forced
  termination is not treated as a proof of recoverable interruption.
- Non-Windows hosts skip the forced-termination classification because this
  probe does not treat POSIX signal delivery as equivalent to a proven Seloryn
  cancellation contract.

This evidence covers one bounded CPU MLP profile and one release binary. It
does not cover arbitrary programs, long-duration soak behavior, cross-machine
reproducibility, graceful cancellation, distributed execution, GPU training,
installer behavior, stable ABI compatibility or a supported production SLA.
