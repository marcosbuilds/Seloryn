# Native distribution implementation review

Review date: 2026-10-26

## Evidence identity

The artifact was built from the working tree based on
`2f28013f078d98b4d3e09ccc00a67e4d9f1246a3` on branch
`feature/job-continuity-bridge`. This is a local preview candidate; it is not a
tagged release and it is not code-signed.

| Identity | Observed value |
|---|---|
| Language | `0.12` |
| Runtime package | `0.12.0` |
| Installer package | `0.12.0` |
| Platform | `windows-x64` |
| Installer | `dist/installer-v7/SelorynSetup-0.12.0-windows-x64.exe` |
| Installer SHA-256 | `f28de273b184f7556876091cf5fb150082f6d58488a98654bc64d2252cc9e950` |
| Embedded runtime SHA-256 | `6a0c55816b20b4561da3f85b05ee835981b44100f75507a818c5a4895086ad2d` |
| Embedded launcher SHA-256 | `6f3f8519c1e0b06c2b573138f83111b18241ebbc274aa4ea97a8b63f4da4b12c` |

## Implemented behavior

- `seloryn-launcher` is a native Rust dispatcher. It uses absolute validated
  paths and `std::process::Command`; it does not invoke a shell or contain a
  second Seloryn runtime.
- `seloryn-setup` is a native Win32 GUI executable. It embeds the runtime and
  launcher payloads, the app-only Seloryn Strand display face and the visual
  panel, then installs to a per-user directory by default. The fixed client
  area is `880×560`, with the visual panel at `280px`; the outer window size is
  calculated from the Win32 frame rather than treating client dimensions as
  window dimensions.
- The GUI is a fixed-size four-stage wizard: welcome, location, installation
  and completion fallback. Installation work runs on a worker thread and posts
  real stage events into a read-only log without blocking the window message
  loop. The successful path closes the setup and opens a verification terminal
  with `seloryn --version` on the freshly installed `bin` directory.
- The location stage offers an explicit cleanup choice. When selected, only
  previously registered runtime versions are removed; user projects are not
  traversed or deleted. When skipped, side-by-side versions are preserved.
- Runtime files live under `versions/<M.m.p>/bin/seloryn.exe` and are described
  by a hash-bound manifest. The root `bin` directory receives the default
  launcher and an exact versioned launcher such as `seloryn0.12.0.exe`.
- State writes use a same-directory temporary file and replacement. Installation
  operations use a create-new lock and stale-lock recovery.
- Existing bytes are verified on reinstall. A different payload for the same
  package version is rejected. The default version is changed only by explicit
  selection or the setup checkbox.
- Uninstall refuses to remove the active default and otherwise removes only the
  requested version.

## Commands and observed results

| Test | Command/result |
|---|---|
| Rust regression | `cargo test --locked --all-targets -- --test-threads=1`: 101 core + 8 launcher + 8 setup + 37 CLI + 15 tensor tests passed |
| Lint gate | `cargo clippy --locked --all-targets -- -D warnings`: passed |
| Installer build gate | `scripts/release/build-installer.ps1 -OutputDirectory dist/installer-v7`: exit 0; embedded setup exit 0 |
| Installed versioned launcher | `seloryn0.12.0 --version`: exit 0; reported `seloryn 0.12 package=0.12.0` |
| Installed default launcher | `seloryn --version`: exit 0; reported `seloryn 0.12 package=0.12.0` |
| Registry listing | `seloryn versions`: exit 0; listed `* 0.12.0` with the runtime hash |
| Invalid selection | `seloryn use 0.13.0`: exit 1 with `SEL-D006` |
| Active uninstall protection | `seloryn uninstall 0.12.0`: exit 1 with `SEL-D011` |
| Payload integrity | Installed runtime hash matched the manifest and embedded payload hash |

The build gate used a fresh install root under the Windows temporary directory
and did not depend on Python, Rust, Cargo or Git at install/run time. The test
host did have the Rust toolchain because it built the artifact.

## Root-cause correction

The first native setup execution failed with Windows status `0xC00000FD`
(stack overflow). The cause was a 1 MiB SHA-256 read buffer allocated on the
default Windows process stack. The buffer now lives on the heap. A small-payload
reproduction and the full embedded runtime both pass after the correction.

## Review verdict

**Approved with explicit limits for the one-version Windows preview gate.**

The real native installer artifact, its four-stage GUI implementation and one
installed runtime are proven at build/headless level. The following are
intentionally not claimed by this review: two distinct public
runtime artifacts coexisting, clean-machine validation, fresh CMD/PowerShell/
Windows Terminal/VS Code PATH sessions, code signing, public release, a REPL,
or the separate native terminal application. A screenshot-driven GUI run at
multiple Windows DPI scales remains a manual validation gate and is not hidden
by this installer result.
