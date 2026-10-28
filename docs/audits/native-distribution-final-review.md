# Native distribution implementation review

Review date: 2026-10-26

## Evidence identity

The artifact was built from the working tree based on
`578983bbc094fa989305a265b99c7581186f480b` on branch
`feature/job-continuity-bridge`. This is a local preview candidate; it is not a
tagged release and it is not code-signed.

| Identity | Observed value |
|---|---|
| Language | `0.12` |
| Runtime package | `0.12.0` |
| Installer package | `0.12.0` |
| Platform | `windows-x64` |
| Installer | `dist/installer-v9/SelorynSetup-0.12.0-windows-x64.exe` |
| Installer SHA-256 | `fba1702f579ab23e079e3d7aa220d0e2d028e511a27f3d3842ae5d556fe0f02f` |
| Embedded runtime SHA-256 | `6b705db42f28aa37a247560dbdd06072d53650f68e6b021a7240cacc7709eb97` |
| Embedded launcher SHA-256 | `4be5b2dc2265f499894f0c9f796b15f93f27c7753288c64352168c1300f05277` |

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
- The private display font is validated independently: all required UI
  codepoints have outlines, the generated face is deterministic, and the
  uppercase `S` retains two contours derived from the two centerlines in
  `assets/brand/seloryn-symbol.svg` with cap-height/advance checks.
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
| Display font QA | `scripts/assets/test-seloryn-display-font.py assets/fonts/seloryn-display.ttf`: passed; 378 mapped codepoints; S=792×769; contours=2 |
| Installer build gate | `scripts/release/build-installer.ps1 -OutputDirectory dist/installer-v9`: exit 0; embedded setup exit 0 |
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
