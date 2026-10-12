# Getting started

**Availability:** Seloryn is a documented language design, not yet installable. There is currently no verified release, downloadable executable, package registry, hosted API or supported installation command.

## Intended first-run experience

After an actual release ships and its binaries pass operating-system checks, the intended user path will be:

1. Download an operating-system-specific signed or checksum-verified release artifact from the project's public releases page.
2. Put `seloryn` on `PATH` using a documented per-platform method; do not require shell evaluation or untrusted installation scripts.
3. Verify `seloryn --version` and `seloryn doctor` show the precise release, build ID, capabilities and target architecture.
4. Save a UTF-8 `.sely` source file and use `seloryn check`, followed by `seloryn run`.
5. Consult actual verified capability flags before attempting tensors, HTTP or model training.

Planned, **not executable**, command flow:

```text
seloryn --version
seloryn doctor
seloryn check hello.sely
seloryn run hello.sely
```

## Design-only hello-world source

```seloryn
fn main() -> Unit:
    print("Hello, Seloryn")
```

## File rules

Sources use `.sely`, UTF-8 and either LF or CRLF on input. The canonical formatter emits LF. Semantic indentation consists of four ASCII spaces; leading tab characters in blocks are rejected. Editors should display whitespace. The module resolver must never execute an arbitrary path just because a program imports a name.

See [CLI](cli.md), [platforms](platforms.md), [language specification](../reference/grammar.md), and [current status](../project/status.md).
