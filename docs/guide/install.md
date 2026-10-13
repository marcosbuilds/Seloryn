# Installation — experimental local tooling

There is no supported public release or download yet. The repository includes
experimental local installers for an already-built binary. They verify
SHA-256 before copying, write provenance metadata, preserve one rollback copy,
support uninstall and report whether the install directory is on `PATH`.
They do not download code, modify `PATH` automatically or create a release.

On the validated Windows x64 host:

```powershell
$hash = (Get-FileHash .\target\release\seloryn.exe -Algorithm SHA256).Hash
.\scripts\install-seloryn-windows.ps1 -Action Install `
  -BinaryPath .\target\release\seloryn.exe `
  -InstallRoot .\local-install `
  -ExpectedSha256 $hash
.\scripts\install-seloryn-windows.ps1 -Action DiagnosePath -InstallRoot .\local-install
.\scripts\install-seloryn-windows.ps1 -Action Rollback -InstallRoot .\local-install
.\scripts\install-seloryn-windows.ps1 -Action Uninstall -InstallRoot .\local-install
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
