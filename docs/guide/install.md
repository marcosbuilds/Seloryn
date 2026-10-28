# Installation — Windows preview distribution

There is no public stable runtime or installer version yet. The latest
published runtime preview is [`0.7.0`](https://github.com/marcosbuilds/Seloryn/releases/tag/seloryn-v0.07);
the complete package map and official download URLs are in
[`packages/catalog.toml`](../../packages/catalog.toml). The repository also
contains an experimental Windows-only beta setup package for the current local
runtime preview. The setup embeds the Rust runtime and native launcher,
installs them into isolated per-version directories, writes a hash-bound
manifest, supports an explicit default-version choice, and can update only the
current user's `PATH`. It does not require Python, Rust, Cargo, Git, or a
network connection after the executable has been built.

The experimental beta artifact is produced by
[`scripts/release/build-installer.ps1`](../../scripts/release/build-installer.ps1)
as `SelorynSetup-experimental-beta-windows-x64.zip`. There is intentionally no
public installer version number yet. Extract the ZIP and run `SelorynSetup.exe`;
the build gate executes the separate headless backend
against a fresh install root and verifies the installed runtime, manifest hash,
versioned launcher, default launcher dispatch, and `seloryn versions`. This is
local evidence for a preview artifact, not a public release or a code-signed
distribution.

The WinUI setup UI defaults to the per-user install directory, adds the
per-user `PATH` only when selected, and makes `0.12.0` the default only when
selected. Existing version directories are never overwritten by a different
payload. The native launcher validates the manifest and runtime hash before
starting the selected executable. The UI is a fixed-size four-stage wizard:
welcome, location, installation and completion. Installation runs off the UI
thread and streams real stage messages into its log.

The older PowerShell lifecycle script remains useful for development-only
installation of an already-built binary. It is not the native setup artifact
and does not create a release. It refuses install, uninstall and rollback
operations when the target already contains native versioned-install state;
use the native setup or the versioned `seloryn` commands for that root.

## Experimental font package

The supplied Seloryn Font v2.0 candidate is available at
[`packages/fonts/seloryn-font/`](../../packages/fonts/seloryn-font/). Install the
TTF files manually on a development machine; use the WOFF/WOFF2 files and
`seloryn-font.css` for web previews. The package is proportional, covers
English and Brazilian Portuguese, and is not a stable or published release.

## Native setup build and verification

Run this on Windows with the Rust development toolchain:

```powershell
.\scripts\release\build-installer.ps1
```

The generated package is a self-contained Windows x64 beta ZIP. Extract it,
run `SelorynSetup.exe` from Explorer, choose the installation directory, select PATH and
default-version behavior explicitly, and press **Install**. Open a new shell
after a PATH change. The installed commands include:

```powershell
seloryn versions
seloryn use 0.12.0
seloryn --version
seloryn0.12.0 --version
```

The current artifact installs one runtime version. The launcher and state
layout are designed for side-by-side package versions, but two distinct public
runtime artifacts and a clean-machine gate are still required before claiming
full multiversion or supported-release status.

## Development-only PowerShell lifecycle

On the validated Windows x64 host:

```powershell
$hash = (Get-FileHash .\target\release\seloryn.exe -Algorithm SHA256).Hash
.\scripts\install\install-seloryn-windows.ps1 -Action Install `
  -BinaryPath .\target\release\seloryn.exe `
  -InstallRoot .\local-install `
  -ExpectedSha256 $hash `
  -ExpectedLanguageVersion 0.12 -ExpectedRuntimeVersion 0.12.0 `
  -AddToPath
.\scripts\install\install-seloryn-windows.ps1 -Action DiagnosePath -InstallRoot .\local-install
.\scripts\install\install-seloryn-windows.ps1 -Action Rollback -InstallRoot .\local-install
.\scripts\install\install-seloryn-windows.ps1 -Action Uninstall -InstallRoot .\local-install -RemoveFromPath
```

`-AddToPath` updates the per-user environment value; open a new PowerShell or
`cmd.exe` process to inherit it. `DiagnosePath` reports both the current
process and the persisted user value. After installation, the terminal
supports real `seloryn check`, `seloryn run`, `seloryn devices` and structured
JSON/JSON2 streaming in both shells; the Windows CI probe executes those
entrypoints against the installed binary.

The POSIX script has the equivalent `install`, `rollback`, `uninstall` and
`diagnose-path` actions. Its syntax and lifecycle were inspected with Git Bash
on the Windows host, but native Linux and macOS binaries were not available
for execution here; those targets remain unsupported.

## Planned distribution requirements

| Target | Artifact | Minimum evidence |
|---|---|---|
| Windows x64 | Local PowerShell installer for an existing native binary | Hash, install, run, PATH diagnosis, rollback and uninstall executed on this host |
| Linux x64 | POSIX installer script only | Native binary and dynamic-linker checks not executed |
| macOS arm64/x64 | POSIX installer script only | Native binary, signing and Gatekeeper checks not executed |

Downloaded releases must publish `SHA-256`, the `seloryn.provenance.v1`
record, target ABI, install/uninstall steps, `PATH` diagnosis and rollback
instructions. The current local staging gate now checks source-tree,
toolchain, lockfile, binary and artifact hashes. Do not mark a target supported
until actual binaries and tests exist. No release date is promised.
