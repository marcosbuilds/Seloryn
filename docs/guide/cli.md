# Command-line checker

The native CLI exposes help, version metadata, the E3 source checker and an
experimental E4 interpreter. It is not an installable release.

```text
seloryn --help
seloryn --version
seloryn check <file.sely>
seloryn run <file.sely>
```

The development version reports `0.00` and `build=local-development`; this is
not a release hash or a supported language release. `check` validates UTF-8,
layout, the E3 parser subset and the E3 semantic core: lexical scopes,
scalar types, records, enums, `Result`, `Option`, `require`, postfix `?`,
calls and exhaustive `match`. It reports `checked` only when all these
checks pass. Imports are diagnosed as unavailable, and nullable `T?` syntax
remains reserved.

`run` performs the same full front-end validation before evaluating the pure E4
subset and the bounded N1/N2 CPU tensor subset. A successful run prints one
deterministic rendered value to stdout. A source or runtime failure writes a
stable `SEL-*` diagnostic to stderr and returns exit code 65; no runtime
evaluation starts after a lexical, parse or semantic diagnostic. The
interpreter performs no file, network, process or GPU effects. The behavior is
covered by the CLI tests in [`tests/bootstrap_cli.rs`](../../tests/bootstrap_cli.rs)
and [`tests/tensor_cpu.rs`](../../tests/tensor_cpu.rs), with independent N2
numeric references in [`tests/n2_probe.py`](../../tests/n2_probe.py).

## Future commands

The following commands remain proposed and must not be treated as available:

```text
seloryn doctor                     # runtime, platform and capabilities
seloryn fmt app.sely               # canonical formatter
seloryn test                       # test runner, when implemented
seloryn add package                # package tool, when implemented
seloryn bench                      # benchmark harness, when implemented
seloryn devices                    # verified accelerator inventory
```

Formatting must be deterministic; machine-readable `--json` diagnostics may be added without corrupting standard output. Interactive/redirected/color output must be detected correctly. Command exit codes and shell quoting must behave consistently in CMD and PowerShell (including empty arguments, Unicode names and paths with spaces). Passing program arguments after `--` remains outside E4.

Every command advertises itself only when its implementation and tests are accepted. `seloryn doctor` must report `unsupported` instead of silently pretending a GPU backend works.
