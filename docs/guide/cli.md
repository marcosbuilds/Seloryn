# Command-line interface

The native CLI exposes help, version metadata, the E3 source checker, bounded
CPU/GPU-linear training profiles and project/bundle gates. It is a verified
preview, not an installable stable release.

```text
seloryn --help
seloryn --version
seloryn devices [--format=human|json]
seloryn gpu-probe [--format=human|json]
seloryn doctor [project] [--format=human|json]
seloryn init <directory> [--template=minimal|tabular-mlp] [--name=<name>] [--format=human|json]
seloryn check <file.sely|project> [--format=human|json] [--quiet]
seloryn run <file.sely|project> [--format=human|json] [--log=off|human|json|json2|json2-signed]
seloryn build <project> --profile debug|release --output <directory>
seloryn exec <bundle.selyapp|directory>
seloryn job create <spec.json> [--id=<job-id>] [--root=<directory>] [--format=human|json]
seloryn job status <job-id> [--root=<directory>] [--format=human|json]
seloryn job list [--root=<directory>] [--format=human|json]
seloryn job transition <job-id> <status> [--root=<directory>] [--format=human|json]
seloryn job claim <job-id> [--owner=<owner>] [--lease-id=<id>] [--ttl-seconds=<n>] [--root=<directory>] [--format=human|json]
seloryn job heartbeat <job-id> --lease-id=<id> [--ttl-seconds=<n>] [--root=<directory>] [--format=human|json]
seloryn job release <job-id> --lease-id=<id> [--root=<directory>] [--format=human|json]
seloryn job recover <job-id> [--root=<directory>] [--format=human|json]
seloryn lsp --stdio
```

The local `job` contract canonicalizes a bounded JSON spec and stores an
event-authoritative state snapshot under `.seloryn/jobs/<job-id>`. Public
`transition` is administrative only: it can move `accepted` to `queued`, but
cannot manufacture `running`, `committing`, `succeeded` or another
executor-owned state. `claim` creates a single local reservation lease with a
bearer `lease_id`, an owner, an attempt identity and a bounded expiry. The
same lease ID can safely retry `claim`; `heartbeat` extends it, `release`
returns the job to `queued`, and `recover` moves an expired reservation to
terminal `lost` (repeating `recover` is idempotent). A lease ID coordinates
local writers; it is not a cryptographic credential, process-liveness proof or
worker fencing token. Seloryn does not yet spawn or supervise a job, verify a
live process, retry a lost job, commit artifacts or execute remotely. Jobs
with `lost` are terminal in this contract; create a new job for a new
execution attempt. Errors `SEL-J013` through `SEL-J020` identify invalid
operation/state, lease ownership, expiry, identity or TTL conditions.

The preview version reports `0.12`, `package=0.12.0` and
`build=local-development`; this is
not a release hash or a supported language release. `check` validates UTF-8,
layout, the E3 parser subset and the E3 semantic core: lexical scopes,
scalar types, records, enums, `Result`, `Option`, `require`, postfix `?`,
calls and exhaustive `match`. It reports `checked` only when all these
checks pass. Local project imports are resolved through `seloryn.toml`; external
packages and nullable `T?` syntax remain unavailable.

Project commands parse `seloryn.toml` as TOML before loading source. Malformed
manifests and a `[package] language` newer than the runtime fail with
`SEL-M001`; no source execution begins after either diagnostic.

`run` performs the same full front-end validation before evaluating the pure E4
subset and the bounded N1/N2 CPU tensor subset. A successful run prints
explicit `from io import print` output and, when non-`Unit`, one deterministic
rendered final value to stdout. A typed `Err(E)` returned by `main` is promoted
to `SEL-R127` at the CLI boundary and returns exit code 65, so a failed result
cannot look like a successful process to automation. Source, runtime and
top-level result failures write stable `SEL-*` diagnostics to stderr; no runtime
evaluation starts after a lexical, parse or semantic diagnostic. For a
successful run, --format=json emits one JSON document on stdout with the
stable {stdout,value} envelope. stdout contains explicit io.print output and
value is the JSON representation of the final Seloryn value; tensor values
expose dtype, shape and element count rather than an unbounded render. For
failures, the same flag keeps one JSON diagnostic document on stderr, with
source byte spans, Unicode-aware locations, help and runtime stack.
`from autograd import gradients` returns one gradient tensor for each of
multiple Tensor[F32] inputs, while `from data import range` creates a bounded
I64 sequence for Seloryn `for` loops. `--quiet` suppresses the successful `check`
line. `--log=human`, `--log=json`, `--log=json2` or `--log=json2-signed` is opt-in and streams the
active training lifecycle to stderr; it never changes program stdout. Human
logs are concise, `json` is schema-v1 compatibility output, and `json2` is a
hash-chained schema-v2 run envelope with identity for the bounded CPU and
GPU-linear training profiles. `json2-signed` adds HMAC-SHA256 tags when
`SELORYN_RECEIPT_KEY` is set.
The MLP profile reports dataset paths, sample counts, effective seed,
batch/model sizes, checkpoint hash and measured train/evaluation losses.
`--log=off` is the default. A standalone source run performs no file, network, process or GPU
effects. A program may explicitly import read_stdin from json to consume one
bounded UTF-8 JSON document from standard input; it does not grant filesystem
or network access. A project run may perform only its explicitly bounded JSON reads. The
behavior is covered by the CLI tests in
[`tests/bootstrap_cli.rs`](../../tests/bootstrap_cli.rs) and
[`tests/tensor_cpu.rs`](../../tests/tensor_cpu.rs), with independent N2 numeric
references in [`tests/n2_probe.py`](../../tests/n2_probe.py). Public project
examples are grouped under [`examples/projects`](../../examples/projects/README.md).

`lsp --stdio` speaks a bounded JSON-RPC Language Server Protocol surface. It
supports full-text `didOpen`/`didChange`/`didSave`, parser-backed diagnostics,
completion, definition and document symbols. It advertises open/close
synchronization and full-text changes. Every opened document must carry an
integer version; a change is applied only when its version is strictly newer
than the stored version. Stale, malformed or ranged changes are ignored so
they cannot overwrite a newer buffer. `didClose` drops the document state and
publishes an empty diagnostic set. Header names are treated case-insensitively
as required by the protocol. The wire contract is tested by
[`tests/lsp_probe.py`](../../tests/lsp_probe.py) and the in-process Rust LSP
tests. It is the runtime foundation for the editor, not yet a claim of
incremental parsing, rename, formatting or debugging support.

`build` validates a local `seloryn.toml` project, packages its validated source
files and explicitly declared `[capabilities].read` and `[capabilities].write`
directories into a deterministic `.selyapp` bundle using the versioned
`seloryn.bundle.v1` format. It also writes `seloryn-build.json` with the
language/runtime versions, profile, byte length and SHA-256 of the artifact.
The runtime performs the same deterministic lowering at `exec` time; this
bundle is not native code or a public release. `exec` validates and materializes
that bundle in a temporary directory before running the same project contract.
When a build directory is supplied, the runtime verifies the metadata hash
before materialization; a bare `.selyapp` remains accepted for transport.
Diagnostics retain the stable bundle path rather than exposing the
materialization directory. A top-level `Err(E)` also points to the first
applicable control-flow source span in the entry function. Repeated execution
of the same failing bundle therefore produces the same diagnostic identity.
Only relative manifest, source and declared-read paths are permitted; a read
target must canonicalize successfully inside its declared root. These
capability checks are policy checks, not an OS sandbox, and this is experimental
bundle evidence rather than an installer or supported release.

`devices` reports the compiled hardware adapters as structured JSON or concise
human output. `gpu-probe` is a bounded hardware smoke test: it requires a
physical non-fallback adapter and validates a real compute dispatch/readback, but it
does not move general model training to the GPU. A `.sely` project can explicitly
import the canonical `gpu` profile for the closed scalar F32 proof (the older
`train_linear_gpu` name remains accepted); it never
silently falls back to CPU.

`doctor [project]` performs read-only manifest, source graph, capability and
semantic validation without executing project code. It emits the stable
`seloryn.doctor.v1` JSON envelope and explicitly reports the current
`verified-preview` channel, unstable ABI and `production_ready=false`.
`init <directory>` creates a validated project atomically and refuses to
overwrite an existing path. The default `minimal` template and the
`tabular-mlp` starter both emit a stable `seloryn.init.v1` report.

`job` exposes the first local lifecycle contract. `create` canonicalizes a
bounded JSON specification and stores it with an initial `accepted` state;
`status` and `list` validate the state snapshot against the append-only event
log; `transition` is limited to the administrative `accepted` to `queued`
edge. The store is
written under `<root>/.seloryn/jobs/<job-id>` using same-directory temporary
files, flushes and atomic replacement. Repeating `create` with the same ID and
specification is idempotent; a different specification is rejected. IDs are
ASCII path-safe and valid job operations use versioned JSON envelopes when
`--format=json` is selected; malformed CLI syntax still uses the global usage
diagnostic.

This is a contract, persistence and reservation-coordination slice, not a
worker or scheduler. It does not execute a spec, claim crash recovery of a
running process, provide process fencing, retry, quotas, remote execution or a
public registry. `run` remains the
backward-compatible direct execution command. These limits are intentional
until a separate worker/supervisor gate proves them with process, concurrency
and interruption tests.

## Future commands

The following commands remain proposed and must not be treated as available:

```text
seloryn fmt app.sely               # canonical formatter
seloryn test                       # test runner, when implemented
seloryn add package                # package tool, when implemented
seloryn bench                      # benchmark harness, when implemented
```

Formatting is deterministic and JSON diagnostics never mix with program stdout.
Interactive/redirected/color output is reserved for the terminal gate; the
current renderer emits no ANSI escapes. Command exit codes and shell quoting
must behave consistently in CMD and PowerShell (including empty arguments,
Unicode names and paths with spaces). Passing program arguments after `--`
remains outside E4.

Every command advertises itself only when its implementation and tests are accepted. `doctor` is intentionally read-only and does not probe or pretend that a GPU backend, registry, ABI or serving system is production-ready.

### Training event stream v1

The narrow `training.train_linear` stream is flushed after every event. JSON
records use `schema_version: 1`: `training.start` identifies the operation,
model, CPU device, dtype, optimizer, learning rate, parameter count and dataset
scope; sampled `training.step` records report completed updates, measured loss
and change from the initial loss; `training.complete` reports the loss change,
improvement flag, holdout MAE, total updates and learned linear parameters.
This contract applies only to the bounded fixture, not to general telemetry.

### Training event stream v2

`--log=json2` begins with `run.start`, emits the v1 training events as chained
records and closes with `run.end`. The start identity covers program, manifest,
datasets and the checkpoint before training; the end identity covers the result
and checkpoint after training. Structured path fields are redacted. The helper
[`scripts/streaming/replay-receipt.py`](../../scripts/streaming/replay-receipt.py)
re-executes successful bounded native or source-owned training receipts in an isolated copy. The external verifier at
[`scripts/streaming/verify-event-chain.py`](../../scripts/streaming/verify-event-chain.py)
detects changed fields, missing or reordered records, truncation, append-after-
completion and duplicate JSON keys. `json2-signed` authenticates the receipt
with a shared HMAC key, but is not a public-key signature. Source-owned training
replay is accepted only when the receipt contains its explicit dataset hashes,
recorded `{"mode":"train"}` request and checkpoint completion event. The stream does not
replay arbitrary executions or imply GPU,
distributed, serving or general runtime telemetry support. Preflight failures
before `run.start` remain normal CLI diagnostics and do not claim to have a
JSON2 receipt.
