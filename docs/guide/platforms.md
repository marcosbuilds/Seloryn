# Platform support

**Currently verified:** none. The rows below are **test targets**, not support badges.

| Platform | Planned target | Validation |
|---|---|---|
| Windows 10/11 x64 | Priority | `cmd.exe`, PowerShell 5.1, PowerShell 7+, Windows Terminal |
| Linux x64 | Priority | Native shell, libc/runtime linkage, paths/signals |
| macOS arm64 / x64 | Later | Gatekeeper, architecture, filesystem case rules |
| Accelerators | Separate qualification | Driver, runtime, device families and numeric tolerance |

A platform is supported only after real executable artifacts pass smoke, conformance, networking and telemetry/redaction tests. Windows support requires passing arguments accurately under both CMD and PowerShell semantics, including quoting and Unicode. Portable UTF-8 paths and CRLF line endings must not be treated as cosmetic issues.
