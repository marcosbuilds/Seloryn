# Getting started

**Availability:** Seloryn 0.00 currently provides an experimental syntax and semantic checker in the development repository. There is no verified release, downloadable executable, package registry, hosted API or supported installation command.

The checker is the first real result of the project: it can reject malformed or semantically inconsistent source before execution exists. The attached `seloryn_e3_design_review.md` describes the broader design; [status](../project/status.md) and the [E3 checklist](../project/e3-checklist.md) distinguish that design from today's implementation.

## What you can do today (E3 checker)

When built from the repository, the native CLI accepts:

```text
seloryn --help
seloryn --version
seloryn check example.sely
```

`check` validates UTF-8, four-space layout, the implemented parser subset and selected semantic rules: local names and scopes, scalar types, function calls and returns, records, enums, `Result`, `Option`, `require`, postfix `?`, pipelines and exhaustive `match`. It emits a structured source diagnostic and nonzero exit code for invalid input. The command path is implemented in [`src/main.rs:82`](../../src/main.rs#L82) and exercised by [`tests/bootstrap_cli.rs:61`](../../tests/bootstrap_cli.rs#L61).

This is useful for evaluating the language design, writing negative examples and giving feedback on the front-end contracts. It does not make a `.sely` file executable.

## What's not yet available

- No interpreter, core IR or `seloryn run` command.
- No tensor operations, allocated numeric buffers or shape execution.
- No HTTP, file, process or package runtime.
- No model loading, autodiff, optimizer or training.
- No GPU, distributed backend, installer or supported release target.

The exact gate-by-gate limits are recorded in the [E3 checklist](../project/e3-checklist.md).

## Expected first run when the interpreter ships in E4 or later

After an actual release ships and its binaries pass operating-system checks, the intended user path will be:

1. Download an operating-system-specific signed or checksum-verified release artifact from the project's public releases page.
2. Put `seloryn` on `PATH` using a documented per-platform method; do not require shell evaluation or untrusted installation scripts.
3. Verify `seloryn --version` and `seloryn doctor` show the precise release, build ID, capabilities and target architecture.
4. Save a UTF-8 `.sely` source file and use `seloryn check`, followed by `seloryn run` once execution is implemented.
5. Consult actual verified capability flags before attempting tensors, HTTP or model training.

Planned, **not executable today**, command flow:

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

The snippet is a language concept. The current checker does not provide `print` or execute `main`.

## File rules

Sources use `.sely`, UTF-8 and either LF or CRLF on input. The canonical formatter emits LF. Semantic indentation consists of four ASCII spaces; leading tab characters in blocks are rejected. Editors should display whitespace. The module resolver must never execute an arbitrary path just because a program imports a name.

See [CLI](cli.md), [platforms](platforms.md), [language specification](../reference/grammar.md), and [current status](../project/status.md). The broader design remains a proposal until each runtime gate has evidence.
