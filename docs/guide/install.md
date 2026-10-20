# Installation — experimental local tooling

There is no supported public runtime release or download yet. The repository
includes local installers for an already-built binary. The validated Windows
installer verifies SHA-256, the binary's canonical language/runtime version and
supplied `seloryn.provenance.v1` metadata before copying, preserves one rollback
copy, supports uninstall and can persist the install directory in the current
user's `PATH`. It never changes the machine-wide PATH. The POSIX script has
lifecycle commands, but its native binary and provenance path are not yet
verified. Neither script downloads code or creates a release.

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
