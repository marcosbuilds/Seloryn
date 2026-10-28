# Native distribution baseline audit

Audit date: 2026-10-26

## Scope and evidence identity

This audit is the M0 baseline for native distribution and terminal work. It
describes the checkout that was actually inspected; it does not promote the
remote default branch or any unreproduced release claim.

| Field | Observed value |
|---|---|
| Repository | `https://github.com/marcosbuilds/Seloryn` |
| Checkout | `D:\\Project\\Seloryn\\seloryn` |
| Branch | `feature/job-continuity-bridge` |
| HEAD | `cf3d9779c1229a8b412649b9619e057b7552be44` |
| HEAD summary | `test: cover embedding checkpoint tamper and resume data guards` |
| `origin/master` | `061890d7b86ec77dd86c20b3fd52238df2a308fc` |
| Language | `0.12` |
| Runtime package | `0.12.0` |
| Channel | `verified-preview` |
| ABI | `unstable` |

The requested `master` target was not the checked-out revision. No branch was
switched during this audit.

## Executed baseline commands

The following commands were run against the checkout above.

| Command | Result |
|---|---|
| `cargo test --locked --all-targets -- --test-threads=1` | exit 0; 101 unit tests, 37 CLI tests and 15 tensor integration tests passed |
| `cargo build --release --locked` | exit 0 |
| `target/release/seloryn.exe --version` | exit 0; `seloryn 0.12 package=0.12.0 build=local-development status=verified-preview` |
| `python tests/release_probe.py` | exit 0; public project and bundle checks passed |
| `python tests/workspace_probe.py` | exit 0; doctor/init/static validation passed |
| `powershell.exe -NoProfile -ExecutionPolicy Bypass -File tests/installer_probe.ps1` | exit 0; the probe passed, including an expected rejected invalid provenance case |

These results prove the current source and preview binary behavior only. They
do not prove a clean-machine installation, a graphical installer, a signed
artifact, or compatibility with every Windows shell.

## Capability classification

The labels below are deliberately narrower than a product announcement:

- **implemented and proven** means code and a relevant observed test exist;
- **implemented, insufficiently tested** means code or a probe exists but the
  required environment or negative coverage is incomplete;
- **partially implemented** means only a bounded subset exists;
- **documented but not implemented** means the public text describes a future
  capability without executable support;
- **not implemented** means no relevant implementation was found;
- **blocked externally** means proof requires an unavailable external system,
  hardware, signing identity, or clean machine.

| Mission question | Baseline answer | Evidence and confidence |
|---|---|---|
| Is the CLI implemented in Rust? | Implemented and proven for the current bounded CLI. | `src/main.rs` owns command parsing and dispatch; `Cargo.toml` defines the Rust binary; the baseline Rust suite passed. High confidence. |
| Which runtime paths execute `.sely`? | Implemented and proven for bounded scalar/list/record/enum/Result programs, CPU tensors, autodiff, selected training workflows, project manifests and `.selyapp` bundles. | `src/runtime.rs`, `src/project.rs`, `src/main.rs`; `tests/bootstrap_cli.rs`, `tests/tensor_cpu.rs`, `tests/release_probe.py`. High confidence for the declared bounded paths. |
| Does the runtime invoke Python? | No Python path was found in `src/` or the Rust manifest. | `Cargo.toml` has Rust dependencies only; source execution uses Rust I/O and process APIs. High confidence. |
| Which Python files are external tools? | Probes, release staging, conformance, streaming verification, benchmark or SDK files are external tooling. | `tests/*.py`, `scripts/release/*.py`, `scripts/ci/*.py`, `scripts/streaming/*.py`, `sdk/python`. High confidence. |
| Is Python needed by a distributed runtime user? | No for the current Rust executable; yes for several developer/release probes and the optional external Python bridge. | `sdk/python/seloryn_bridge`, test matrix and release scripts. High confidence. |
| What does the current Windows installer do? | A PowerShell install script validates a binary/version/provenance, copies it to `InstallRoot\\bin\\seloryn.exe`, writes one manifest, optionally adds one user PATH entry, supports rollback and selective removal of that one installation. | `scripts/install/install-seloryn-windows.ps1`, `tests/installer_probe.ps1`. High confidence. |
| How does it change PATH? | It reads and rewrites the user `Path` environment variable, de-duplicates a normalized entry, and removes it only when requested. | `User-PathEntries`, `Ensure-User-PathEntry`, `Remove-User-PathEntry` in the installer script. The probe validates the persisted user entry, but not a complete fresh CMD/PowerShell matrix. High confidence for implementation; medium for shell behavior. |
| Is the installer safe for duplicate versions and partial failure? | No. It has a single destination and overwrites it; rollback is a copied previous binary, not an isolated version transaction. Manifest and PATH writes are not a coordinated atomic installation journal. | `destinationRelative = 'bin/seloryn.exe'`, `Copy-Item -Force`, single `seloryn-install.json`. High confidence. |
| Are multiple runtimes supported? | Not implemented. | No native registry, version-specific runtime directories, versioned commands, or persistent default selector were found. High confidence. |
| Are supported operating systems proven? | Windows host behavior is partially exercised. Linux/macOS builds are described in release workflows, but installed-user behavior was not proven here. | `.github/workflows/release-runtime.yml`, local Windows commands and probes. Medium confidence. |
| How does VS Code find Seloryn? | Implemented as a parser/LSP and CLI integration surface, but runtime discovery and a real interactive terminal session are insufficiently tested. | `packages/vscode/extension.js`, `packages/vscode/lsp-client.js`, `tests/lsp_probe.py`, extension tests, `docs/guide/editor.md`. Medium confidence. |
| Does the CLI accept interactive commands? | Not implemented. | `src/main.rs` has no REPL action; `read_stdin` is a bounded one-shot JSON input path. `docs/guide/cli.md` explicitly reserves interactive output for a future terminal gate. High confidence. |
| Which components can be reused by a REPL? | The lexer, parser, semantic checker, runtime value/type machinery, diagnostics, JSON renderer and cancellation hooks are reusable; the current runtime entrypoint is still program-oriented. | `src/lexer.rs`, `src/parser.rs`, `src/semantic.rs`, `src/runtime.rs`, `src/diagnostic.rs`, `src/main.rs`. Medium confidence until a session contract is written. |
| What is the current version contract? | `VERSION.toml` is the earned language/package source of truth and `scripts/release/verify-version.py` cross-checks evidence paths and package compatibility. | `VERSION.toml`, `Cargo.toml`, `src/main.rs`, `scripts/release/verify-version.py`, `docs/project/versions.md`. High confidence. |
| What do current versions mean? | Language `0.12` records the accepted capability ledger; runtime and extension are `0.12.0`; the binary is a verified preview with unstable ABI, not a stable installer release. | `VERSION.toml`, `docs/project/status.md`, `src/main.rs`. High confidence. |
| Which tests use real binaries? | Rust integration tests, release probes, workspace probes, LSP probes and the PowerShell installer probe launch the built binary. | `tests/bootstrap_cli.rs`, `tests/tensor_cpu.rs`, `tests/release_probe.py`, `tests/workspace_probe.py`, `tests/installer_probe.ps1`. High confidence for the executed commands listed above. |
| Which tests require artificial/developer environments? | Python probes, release scripts, Node/npm extension packaging and GPU probes require developer tooling or specific hardware. | `tests/conformance_matrix.json`, `scripts/release`, `packages/vscode`, GPU probe requirements. High confidence. |
| Which capabilities are experimental? | General package resolution, stable ABI, general GPU training, production serving, distributed execution, REPL, terminal application and supported native installer remain outside the verified preview. | `docs/project/status.md`, `docs/guide/cli.md`, `docs/guide/install.md`, runtime help text and current contracts. High confidence. |
| Are docs, code, releases and artifacts synchronized? | The earned version metadata is internally checked, but the distribution story is not synchronized: docs describe a preview/runtime archive and PowerShell probe, while no GUI installer or multiversion artifact exists. | `VERSION.toml`, release scripts/workflows, install script and docs. High confidence. |
| Is there compatibility risk in changing installation? | Yes. Existing users may rely on `LOCALAPPDATA\\Seloryn\\bin\\seloryn.exe`, the single manifest, user PATH entry and rollback behavior. Migration must preserve or explicitly migrate these paths. | Current installer and `tests/installer_probe.ps1`. High confidence. |

## Current installer security findings

Positive controls already present:

- SHA-256 is computed and an optional expected hash is checked.
- The binary is executed with `--version` before registration.
- Provenance schema, language version, runtime version and binary hash can be
  checked.
- Relative manifest paths reject rooted paths and `..` components.
- User PATH entries are compared case-insensitively after trimming a trailing
  separator.
- The invalid-provenance negative case is exercised by the installer probe.

Gaps that block a native distribution claim:

- The destination is a single mutable runtime file, so installing a second
  version replaces the first one.
- The manifest is one mutable JSON file with no journal, lock, recovery state
  or atomic same-directory replacement.
- The rollback copy is timestamped but not an immutable per-version registry.
- A hash alone does not authenticate provenance; no signature or trusted
  release channel is verified by the installer.
- Archive extraction and GUI privilege/consent behavior do not exist yet.
- PATH persistence was observed, but fresh processes in CMD, Windows PowerShell
  5.1, PowerShell 7+, Windows Terminal and VS Code were not all executed.

## Architecture decision inputs

The implementation gate should use one shared Rust distribution module for the
launcher and installer. It should own canonical version parsing, validated
installation manifests, a per-user root, a persistent default selector,
same-directory atomic writes and a lock. Runtime execution must use absolute
paths and `std::process::Command` argument vectors so the launcher never parses
or invokes a shell.

The first Windows installer can be a small native Win32 Rust executable with an
embedded runtime and launcher payload. It must perform actual preflight,
extraction, manifest registration, optional user PATH update and post-install
hash/version checks. It must install per-user by default and leave each version
under an isolated directory. A full MSI/Burn pipeline remains a follow-up
packaging option after the native operation contract is proven.

This choice is consistent with the primary references consulted for this gate:

- Rust `std::process::Command` documents literal argument passing, inherited
  standard handles and the risk of shell-specific parsing:
  <https://doc.rust-lang.org/std/process/struct.Command.html>
- Microsoft documents persistent per-user environment variables and PATH
  scope:
  <https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_environment_variables>
- Microsoft documents process creation and the security implications of handle
  inheritance:
  <https://learn.microsoft.com/en-us/windows/win32/procthread/creating-processes>
- WiX/Burn is a credible future packaging alternative for a standard Windows
  bundle and per-user scope, but it adds an external build/payload toolchain:
  <https://docs.firegiant.com/wix/tools/burn/>

## M0 gate status

**M0 baseline audit: complete for the current checkout.**

The major pre-implementation assumptions are now separated from observed
behavior. The next gate is the native multiversion distribution implementation;
its acceptance must include two installed runtime directories, two exact
versioned commands, default selection persistence, selective removal, hash
stability of the first runtime, and a real GUI installer artifact. No stable
release, language-version credit or signing claim is implied by this audit.
