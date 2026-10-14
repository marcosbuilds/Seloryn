# Platform support

**Development-host smoke verified:** Windows `cmd.exe`, PowerShell 5.1 and
PowerShell 7.6 execute both successful N1/N2 CPU tensor examples and the
expected failure exit path. GitHub CI also runs the Rust and public probes on
Ubuntu 24.04 and macOS 15. This is execution evidence, not an
installable-platform support badge.

| Platform | Planned target | Validation |
|---|---|---|
| Windows 10/11 x64 | Development host smoke observed; local installer lifecycle verified | `cmd.exe`, PowerShell 5.1, PowerShell 7.6; Windows Terminal and public release remain unverified |
| Linux x64 | CI execution evidence; not supported | Native shell, libc/runtime linkage, paths/signals and an actual release artifact |
| macOS arm64 / x64 | CI execution evidence; not supported | Gatekeeper, architecture, filesystem case rules and an actual release artifact |
| Accelerators | Separate qualification | Driver, runtime, device families and numeric tolerance |

A platform is supported only after real executable artifacts pass smoke, conformance, networking and telemetry/redaction tests. Windows support requires passing arguments accurately under both CMD and PowerShell semantics, including quoting and Unicode. Portable UTF-8 paths and CRLF line endings must not be treated as cosmetic issues.
