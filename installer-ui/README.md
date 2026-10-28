# Seloryn Setup UI

This is the modern Windows frontend for Seloryn Setup: WinUI 3 + Windows App
SDK + C#/.NET, with the Rust installer kept as a headless backend.

The project expects the .NET SDK on `D:\Toolchains\dotnet`. Build the UI from
this directory with:

```powershell
$env:DOTNET_ROOT = 'D:\Toolchains\dotnet'
$env:DOTNET_CLI_HOME = 'D:\Toolchains\dotnet-home'
& "$env:DOTNET_ROOT\dotnet.exe" build .\SelorynInstaller.csproj -c Debug -p:Platform=x64
```

Use `scripts/release/build-installer.ps1` for a complete release. It copies the
real Rust backend and payloads, publishes the self-contained WinUI folder and
creates the Windows-only `SelorynSetup-experimental-beta-windows-x64.zip`.
The package intentionally has no public installer version yet; `0.12.0`
remains internal runtime compatibility metadata for the embedded transaction.
