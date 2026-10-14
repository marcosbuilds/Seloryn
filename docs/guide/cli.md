# Command-line interface

The native CLI exposes help, version metadata, the E3 source checker and an
experimental E4 interpreter and the narrow project/bundle gates. It is not an installable release.

```text
seloryn --help
seloryn --version
seloryn check <file.sely> [--format=human|json] [--quiet]
seloryn run <file.sely> [--format=human|json]
seloryn build <project> --profile debug|release --output <directory>
seloryn exec <bundle.selyapp|directory>
```

The development version reports `0.00` and `build=local-development`; this is
not a release hash or a supported language release. `check` validates UTF-8,
layout, the E3 parser subset and the E3 semantic core: lexical scopes,
scalar types, records, enums, `Result`, `Option`, `require`, postfix `?`,
calls and exhaustive `match`. It reports `checked` only when all these
checks pass. Local project imports are resolved through `seloryn.toml`; external
packages and nullable `T?` syntax remain unavailable.

`run` performs the same full front-end validation before evaluating the pure E4
subset and the bounded N1/N2 CPU tensor subset. A successful run prints
explicit `from io import print` output and, when non-`Unit`, one deterministic
rendered final value to stdout. A typed `Err(E)` returned by `main` is promoted
to `SEL-R127` at the CLI boundary and returns exit code 65, so a failed result
cannot look like a successful process to automation. Source, runtime and
top-level result failures write stable `SEL-*` diagnostics to stderr; no runtime
evaluation starts after a lexical, parse or semantic diagnostic. `--format=json` emits one
JSON document containing the diagnostics, source byte spans, Unicode-aware
locations, help and runtime stack. `--quiet` suppresses the successful `check`
line. A standalone source run performs no file, network, process or GPU
effects; a project run may perform only its explicitly bounded JSON reads. The
behavior is covered by the CLI tests in
[`tests/bootstrap_cli.rs`](../../tests/bootstrap_cli.rs) and
[`tests/tensor_cpu.rs`](../../tests/tensor_cpu.rs), with independent N2 numeric
references in [`tests/n2_probe.py`](../../tests/n2_probe.py). Public project
examples are grouped under [`examples/projects`](../../examples/projects/README.md).

`build` validates a local `seloryn.toml` project, packages its validated source
files and explicitly declared `[capabilities].read` directories into a
deterministic experimental `.selyapp` bundle and writes JSON build metadata.
The runtime performs the same deterministic lowering at `exec` time; this B1
bundle is not native code or a public release. `exec` validates and materializes
that bundle in a temporary directory before running the same project contract,
but diagnostics retain the stable bundle path rather than exposing the
materialization directory. A top-level `Err(E)` also points to the first
applicable control-flow source span in the entry function. Repeated execution
of the same failing bundle therefore produces the same diagnostic identity.
Only relative manifest, source and declared-read paths are permitted; this is
B1 evidence, not an installer or supported release.

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

Formatting is deterministic and JSON diagnostics never mix with program stdout.
Interactive/redirected/color output is reserved for the terminal gate; the
current renderer emits no ANSI escapes. Command exit codes and shell quoting
must behave consistently in CMD and PowerShell (including empty arguments,
Unicode names and paths with spaces). Passing program arguments after `--`
remains outside E4.

Every command advertises itself only when its implementation and tests are accepted. `seloryn doctor` must report `unsupported` instead of silently pretending a GPU backend works.
