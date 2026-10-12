# Getting started

**Availability:** Seloryn 0.00 currently provides an experimental syntax and semantic checker plus a pure scalar interpreter in the development repository. There is no verified release, downloadable executable, package registry, hosted API or supported installation command.

The checker is the first real result of the project: it can reject malformed or semantically inconsistent source before execution exists. The attached `seloryn_e3_design_review.md` describes the broader design; [status](../project/status.md) and the [E3 checklist](../project/e3-checklist.md) distinguish that design from today's implementation.

## What you can do today (E3 checker and E4 interpreter)

When built from the repository, the native CLI accepts:

```text
seloryn --help
seloryn --version
seloryn check example.sely
seloryn run example.sely
```

`check` validates UTF-8, four-space layout, the implemented parser subset and selected semantic rules: local names and scopes, scalar types, function calls and returns, records, enums, `Result`, `Option`, `require`, postfix `?`, pipelines and exhaustive `match`. It emits a structured source diagnostic and nonzero exit code for invalid input. The command path is implemented in [`src/main.rs:82`](../../src/main.rs#L82) and exercised by [`tests/bootstrap_cli.rs:61`](../../tests/bootstrap_cli.rs#L61).

`run` executes the same bounded constructs after all front-end diagnostics
pass. It is pure: it has no filesystem, network, process, tensor or accelerator
effects. A valid program starts at `fn main()`, and its returned value is
rendered to stdout.

## What's not yet available

- No core IR, module system or standard library runtime.
- No tensor operations, allocated numeric buffers or shape execution.
- No HTTP, file, process or package runtime.
- No model loading, autodiff, optimizer or training.
- No GPU, distributed backend, installer or supported release target.

The exact gate-by-gate limits are recorded in the [E3 checklist](../project/e3-checklist.md).

## Development run

From a built development binary, the current evidence path is:

1. Download an operating-system-specific signed or checksum-verified release artifact from the project's public releases page.
2. Put `seloryn` on `PATH` using a documented per-platform method; do not require shell evaluation or untrusted installation scripts.
3. Verify `seloryn --version` and `seloryn doctor` show the precise release, build ID, capabilities and target architecture.
4. Save a UTF-8 `.sely` source file and use `seloryn check`, followed by `seloryn run` for the pure E4 subset.
5. Consult actual verified capability flags before attempting tensors, HTTP or model training.

The broader commands remain planned; the currently executable subset is:

```text
seloryn --version
seloryn doctor
seloryn check hello.sely
seloryn run hello.sely
```

## Runnable pure hello-world source

```seloryn
fn main() -> Text:
    return "Hello, Seloryn"
```

The E4 interpreter returns the text value; it does not provide a `print`
effect. Tensor, module and systems examples elsewhere remain design-only.

## File rules

Sources use `.sely`, UTF-8 and either LF or CRLF on input. The canonical formatter emits LF. Semantic indentation consists of four ASCII spaces; leading tab characters in blocks are rejected. Editors should display whitespace. The module resolver must never execute an arbitrary path just because a program imports a name.

See [CLI](cli.md), [platforms](platforms.md), [language specification](../reference/grammar.md), and [current status](../project/status.md). The broader design remains a proposal until each runtime gate has evidence.
