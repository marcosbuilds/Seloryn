# Installation — experimental local tooling

There is no supported public runtime release or download yet. The repository
includes local installers for an already-built binary. The validated Windows
installer verifies SHA-256 and the binary's canonical language/runtime version
before copying, can write supplied provenance metadata, preserves one rollback
copy, supports uninstall and reports whether the install directory is on
`PATH`. The POSIX script has lifecycle commands,
but its native binary and provenance path are not yet verified. Neither script
downloads code, modifies `PATH` automatically or creates a release.

On the validated Windows x64 host:

```powershell
$hash = (Get-FileHash .\target\release\seloryn.exe -Algorithm SHA256).Hash
.\scripts\install\install-seloryn-windows.ps1 -Action Install `
  -BinaryPath .\target\release\seloryn.exe `
  -InstallRoot .\local-install `
  -ExpectedSha256 $hash `
  -ExpectedLanguageVersion 0.07 -ExpectedRuntimeVersion 0.7.0
.\scripts\install\install-seloryn-windows.ps1 -Action DiagnosePath -InstallRoot .\local-install
.\scripts\install\install-seloryn-windows.ps1 -Action Rollback -InstallRoot .\local-install
.\scripts\install\install-seloryn-windows.ps1 -Action Uninstall -InstallRoot .\local-install
```

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

Downloaded releases must publish `SHA-256`, provenance/build ID, target ABI, install/uninstall steps, `PATH` diagnosis and rollback instructions. Do not mark a target supported until actual binaries and tests exist. No release date is promised.
