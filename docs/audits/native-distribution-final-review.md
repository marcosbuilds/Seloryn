# Native distribution implementation review

Review date: 2026-10-26

## Evidence identity

The artifact was built from the working tree based on
`cf3d9779c1229a8b412649b9619e057b7552be44` on branch
`feature/job-continuity-bridge`. This is a local preview candidate; it is not a
tagged release and it is not code-signed.

| Identity | Observed value |
|---|---|
| Language | `0.12` |
| Runtime package | `0.12.0` |
| Installer package | `0.12.0` |
| Platform | `windows-x64` |
| Installer | `dist/installer/SelorynSetup-0.12.0-windows-x64.exe` |
| Installer SHA-256 | `3ea51bf40e741b72301bb587cd3e0b115eecc4d2303b5463d92c9747ee132a3e` |
| Embedded runtime SHA-256 | `ef083b52d6e4d4eff0d5b52070ccc93071a56d7da75d219bbed463b1373114c2` |
| Embedded launcher SHA-256 | `77bc547aa63671ef940830263577d7c5ade354fd189db3d533dafa958b2caba4` |

## Implemented behavior

- `seloryn-launcher` is a native Rust dispatcher. It uses absolute validated
  paths and `std::process::Command`; it does not invoke a shell or contain a
  second Seloryn runtime.
- `seloryn-setup` is a native Win32 GUI executable. It embeds the runtime and
  launcher payloads and installs to a per-user directory by default.
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
| Installer build gate | `scripts/release/build-installer.ps1`: exit 0; embedded setup exit 0 |
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

The real native installer artifact and one installed runtime are proven. The
following are intentionally not claimed by this review: two distinct public
runtime artifacts coexisting, clean-machine validation, fresh CMD/PowerShell/
Windows Terminal/VS Code PATH sessions, code signing, public release, a REPL,
or the separate native terminal application. Those remain later gates and are
not hidden by this installer result.
