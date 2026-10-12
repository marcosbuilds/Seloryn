# Command-line bootstrap

The native CLI bootstrap is implemented with help, version metadata and a
syntax-only source check. It is not an installable release and it cannot type
check, compile or run `.sely` programs.

```text
seloryn --help
seloryn --version
seloryn check <file.sely>
```

The development version reports `0.00` and `build=local-development`; this is
not a release hash or a supported language release. `check` validates UTF-8,
layout tokens and the implemented parser subset, then reports `syntax-only`.
The observed behavior is covered by [the CLI integration tests](../../tests/bootstrap_cli.rs).

## Future commands

The following commands remain proposed and must not be treated as available:

```text
seloryn doctor                     # runtime, platform and capabilities
seloryn run app.sely               # execute a program
seloryn fmt app.sely               # canonical formatter
seloryn test                       # test runner, when implemented
seloryn add package                # package tool, when implemented
seloryn bench                      # benchmark harness, when implemented
seloryn devices                    # verified accelerator inventory
```

Formatting must be deterministic; machine-readable `--json` diagnostics may be added without corrupting standard output. Interactive/redirected/color output must be detected correctly. Command exit codes and shell quoting must behave consistently in CMD and PowerShell (including empty arguments, Unicode names and paths with spaces). `run` passes program arguments after `--`.

Every command advertises itself only when its implementation and tests are accepted. `seloryn doctor` must report `unsupported` instead of silently pretending a GPU backend works.
