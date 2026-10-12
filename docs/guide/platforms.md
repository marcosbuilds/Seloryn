# Platform support

**Development-host smoke verified:** Windows `cmd.exe`, PowerShell 5.1 and
PowerShell 7.6 executed the release binary against the runnable N1 CPU tensor
example. This is command evidence, not an installable-platform support badge.

| Platform | Planned target | Validation |
|---|---|---|
| Windows 10/11 x64 | Development host smoke observed | `cmd.exe`, PowerShell 5.1, PowerShell 7.6; Windows Terminal and installer remain unverified |
| Linux x64 | Priority | Native shell, libc/runtime linkage, paths/signals |
| macOS arm64 / x64 | Later | Gatekeeper, architecture, filesystem case rules |
| Accelerators | Separate qualification | Driver, runtime, device families and numeric tolerance |

A platform is supported only after real executable artifacts pass smoke, conformance, networking and telemetry/redaction tests. Windows support requires passing arguments accurately under both CMD and PowerShell semantics, including quoting and Unicode. Portable UTF-8 paths and CRLF line endings must not be treated as cosmetic issues.
