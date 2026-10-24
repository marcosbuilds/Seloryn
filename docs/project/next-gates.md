# Next executable gates

Seloryn is currently `0.12`: a bounded verified-preview language and runtime for
AI development and integration. The next steps are executable gates, not a
promise that every design proposal already exists.

## Completed baseline

- The public repository has a deterministic front end, pure bounded execution,
  local project/bundle support and bounded N1/N2 CPU tensor behavior.
- Tensor[F32] reverse-mode autodiff is executable for one input and a rank-zero
  CPU objective, with an independent CLI oracle.
- A bounded two-layer CPU MLP now trains on real JSON data with deterministic
  mini-batches, holdout evaluation, checkpoint/resume and live terminal events;
  a general model API, optimizer family and accelerator path remain outside
  the gate.
- A Seloryn-authored project now defines a tensor objective, calls gradients
  for multiple parameters, iterates with `range` and performs SGD updates in
  `.sely`; the native backend supplies the bounded tensor/autodiff kernels.
- MLP resume now detects tampering in sampler state as well as weights and enforces a
  total sample-update budget. `devices` and `gpu-probe` provide a real
  capability-aware hardware gate with synchronized WGSL readback. The closed
  scalar `train_linear_gpu` profile now performs a real device-resident update;
  MLP, Tensor and autodiff training remain CPU-only.
- Public CI exercises Linux, Windows and macOS, while independent CLI probes
  compare selected tensor results with external references.
- Project manifests now use a real TOML parser and reject malformed or
  future-language declarations before source loading; the independent
  `project_manifest_probe.py` and editor CLI fixture cover both paths.
- The VS Code package provides `.sely` association, syntax highlighting,
  native completion and opt-in model ghost text. Its installable preview is
  validated separately from the language runtime.
- `--log=json2` now emits a real hash-chained run envelope for the two bounded
  CPU training profiles. An independent Python verifier and black-box probe
  detect changed, missing, reordered, truncated and duplicate-key records;
  schema-v1 JSON remains available for compatibility. JSON2 also carries
  program/manifest/dataset/checkpoint/result identity; the public replay helper
  re-executes successful receipts in an isolated copy, signed receipts use
  keyed HMAC-SHA256 and structured path fields are redacted.
- `train_linear_gpu` is an explicit scalar F32 GPU profile: the shader updates
  device-resident parameters and performs one final readback. It fails without
  a physical adapter and never silently falls back to CPU.
- Bounded JSON numeric arrays now convert to typed lists through
  `json.as_i64_list` and `json.as_f64_list`, with positive and negative
  black-box evidence in `json_array_probe.py`.
- A bounded CPU MLP checkpoint can now be consumed by `training.predict_mlp`;
  the independent inference probe checks deterministic predictions, checkpoint
  and input hashes, JSON2 receipt verification and invalid dimensions/hash
  rejection.
- `doctor` now performs read-only project and semantic validation, while
  `init` creates validated minimal or tabular-MLP projects transactionally and
  refuses overwrite; both have stable JSON contracts and independent probes.
- Runtime staging now embeds `seloryn.provenance.v1` with source-tree,
  toolchain, lockfile, binary and artifact hashes; the release probe verifies
  the envelope before accepting a candidate.
- Built bundles now use the explicit `seloryn.bundle.v1` format and emit
  `seloryn-build.json`; directory execution verifies the metadata hash before
  materialization. The LSP tracks document versions and closes diagnostics
  deterministically.
- A black-box operational probe now runs four fresh CPU training processes
  concurrently, verifies deterministic semantic receipts/checkpoints and exact
  workspace confinement, and records forced Windows termination behavior without
  claiming graceful cancellation.
- The `self-hosted-mlp` proof now owns a complete custom ReLU network in `.sely`,
  including its dataset, objective, updates, holdout evaluation, persisted
  JSON checkpoint, reload and inference, with source-owned progress events.
- The `self-hosted-language-model` proof now owns a complete runtime-sized
  bigram next-token workflow in modular `.sely` files, including vocabulary and
  textual dataset ingestion, tokenizer, fused cross-entropy, reusable parameter
  and SGD modules, held-out evaluation, checkpoint hash validation, reload,
  JSON2 progress and a separate inference request.
- The `auditable-local-inference` proof now owns a complete small ReLU MLP
  workflow in `.sely`, including explicit dataset/model/run contracts, its own
  data ingestion, source-owned updates, holdout evaluation, hash-bound JSON
  checkpoint, separate inference, structured audit event and six negative
  boundary cases. A pure-Python standard-library implementation is used only
  as an independent numerical oracle.
- Cooperative cancellation is now a real, bounded slice: `SIGINT`/`SIGTERM`
  on Unix and `Ctrl+C`/`Ctrl+Break` on Windows produce a valid JSON2 `run.end`
  with `error_code=SEL-CANCELLED` and exit code `130`; the public black-box
  probe verifies that no success result is emitted. This does not claim
  preemption of every loop, cleanup after forced termination, or resumability.
- `source-contract-reuse` demonstrates one source-owned `Parameter`/`Sgd`
  contract consumed by an MLP and a language-model function. It is an
  experimental same-project proof, not a package, registry, resolver or ABI.
- The next continuation is also closed with evidence: the contiguous tensor
  access fast path reduced the recorded five-run median from `0.182768 s` to
  `0.157362 s` (`13.9%`) without changing the numerical result; both source-
  owned training projects now share the same `Parameter`/`Sgd` contract;
  multi-symbol local imports validate every requested export; and source-owned
  training receipts replay successfully in an isolated copy while rejecting a
  changed dataset.
- The first job-lifecycle contract is now executable locally: bounded JSON
  specs receive safe IDs, canonical hashes and an `accepted` state; state is
  checked against an append-only event log; public transitions cannot forge
  executor-owned states; and a bounded local reservation lease supports
  claim/heartbeat/release plus expiry-only recovery. Atomic writes,
  event-authoritative repair, 100-way claim races and lease expiry are covered
  by black-box probes. This does not execute a job, prove process liveness,
  fence a worker or retry a lost job.

## Next gates

1. **Editor distribution — completed locally:** the `0.12.0` package is on the
   same compatibility line as language `0.12` and runtime `0.12.0`. It contains
   a deterministic self-contained VSIX, the canonical Seloryn symbol and the
   optional `.sely` file-icon theme. A real VS Code host still needs an
   interactive rendering check; that is a validation task, not an unbuilt
   distribution feature.
2. **Editor/runtime integration — lifecycle-safe full-document LSP slice
   completed:** the editor now uses `seloryn lsp --stdio` by default for
   unsaved diagnostics, completion and definition. The server advertises
   open/close plus full-text synchronization, requires integer document
   versions, rejects stale or ranged changes, and clears state and diagnostics
   on `didClose`; the black-box probe covers these transitions.
   `seloryn.lsp.enabled=false` preserves the saved-file CLI bridge with its
   timeout, output bound and cancellation fixtures. Incremental/indexed project
   analysis, range-change support and rename remain the next editor gates.
3. **AI workflow evidence — first slice completed:** the auditable local-
   inference proof compares numerical results with a pure-Python oracle and
   measures contract rejection, checkpoint tampering, receipt validity and
   source ownership. The next study must compare development/production
   adaptations and diagnostic effort against a real MLflow/DVC-style workflow
   without claiming superiority from the fixture alone.
4. **Numerical continuation — first contract slice completed:** `Parameter`,
   `replace`, `Sgd`, `sgd(Tensor[F32])`, `sgd_rate(F64)` and `step` are now a
   shared source-level contract exercised by the MLP and language-model
   projects. The next gate is explicit parameter collections, optimizer state
   and checkpoint compatibility; a public `Module`/`Dataset` framework is not
   claimed yet.
5. **Runtime release:** the local 0.12.0 candidate now has supported Windows
   x64 staging, checksums, provenance, installer failure-path evidence and the
   bounded operational reliability evidence. Public binaries, cross-platform
   installer execution, upgrade/rollback behavior and a release remain pending
   explicit publication authorization and corresponding platform evidence.
6. **Receipt continuation — source-owned training slice completed:** identity,
   successful isolated replay for native and source-owned CPU training, and
   cooperative cancellation with an explicit `SEL-CANCELLED` end event,
   semantic-event comparison, structured path redaction and keyed HMAC
   authentication are now tested. Remaining gates are pre-run failure receipts,
   initial checkpoint snapshots for resume replay, filesystem-wide output
   snapshots, public-key signatures, async backpressure and
   slow/disconnected consumer evidence.
7. **GPU continuation — first slice completed:** the closed `train_linear_gpu`
   proof is real on a declared physical adapter. The next gate is a reusable
   device-resident Tensor/F32 abstraction, then GPU autodiff and MLP training;
   no fallback or distributed claim is implied.
8. **Job coordination continuation — reservation lease slice completed:**
   `seloryn job` now provides a durable local spec/state/event boundary and a
   bounded reservation lease. The next gate must add a single-process worker
   only after defining process identity, fencing, preflight/failure receipts,
   process-group cancellation, artifact commit markers and idempotent recovery.
   It must prove that an expired lease cannot abandon a live worker before any
   queue, retry policy, remote worker or serving API is added.

## Boundaries

No current gate implies a package registry, unrestricted effects, GPU tensor
training, distributed execution, general tensor autodiff, model serving, a
full LSP or a stable language release. See [current status](status.md) for the
observed implementation boundary.
