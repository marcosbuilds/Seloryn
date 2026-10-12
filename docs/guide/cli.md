# Command-line design

**Proposed commands — not implemented.**

```text
seloryn --version                  # exactly M.mm + independent build hash
seloryn doctor                     # runtime, platform and capabilities
seloryn run app.sely               # execute a program
seloryn check app.sely             # parse + type checking
seloryn fmt app.sely               # canonical formatter
seloryn test                       # test runner, when implemented
seloryn add package                # package tool, when implemented
seloryn bench                      # benchmark harness, when implemented
seloryn devices                    # verified accelerator inventory
```

Formatting must be deterministic; machine-readable `--json` diagnostics may be added without corrupting standard output. Interactive/redirected/color output must be detected correctly. Command exit codes and shell quoting must behave consistently in CMD and PowerShell (including empty arguments, Unicode names and paths with spaces). `run` passes program arguments after `--`.

Every command advertises itself only when its implementation and tests are accepted. `seloryn doctor` must report `unsupported` instead of silently pretending a GPU backend works.
