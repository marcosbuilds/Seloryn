# Installation — not yet available

There are **no valid installation instructions at present**. Installing a made-up package name or using copied unreviewed shell scripts is not supported. The public release page will be the source of truth after the first independently verified build.

## Planned distribution requirements

| Target | Artifact | Minimum evidence |
|---|---|---|
| Windows x64 | Native binary + optional installer | CMD, PowerShell 5.1/7+, Unicode/space paths, exit codes |
| Linux x64 | Static/dynamic package with declared dependencies | Dynamic linker baseline, sandboxed CI tests |
| macOS arm64/x64 | Signed/notarized packaging where applicable | Architecture and Gatekeeper checks |

Downloaded releases must publish `SHA-256`, provenance/build ID, target ABI, install/uninstall steps, `PATH` diagnosis and rollback instructions. Do not mark a target supported until actual binaries and tests exist. No release date is promised.
