# Getting started

**Availability:** Seloryn 0.00 currently provides an experimental syntax and semantic checker, a pure interpreter and a bounded CPU tensor runtime in the development repository. There is no verified release, downloadable executable, package registry, hosted API or supported installation command.

The checker rejects malformed or semantically inconsistent source before execution. The [status page](../project/status.md) distinguishes implemented behavior from future design.

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
pass. It is pure: it has no filesystem, network, process or accelerator effects.
The N1 bridge can allocate bounded CPU tensors and run checked F32/I64 kernels.
A valid program starts at `fn main()`, and its returned value is rendered to
stdout.

## What's not yet available

- No core IR, module system or standard library runtime.
- No GPU, distributed or symbolic-shape execution; N1 supports only bounded CPU tensors.
- No HTTP, file, process or package runtime.
- No model loading, autodiff, optimizer or training.
- No GPU, distributed backend, installer or supported release target.

The current capability limits are recorded in the [status page](../project/status.md) and the [tensor reference](../compute/tensors.md).

## Development run

From a built development binary, the current evidence path is:

1. Install a Rust toolchain and clone the development repository.
2. Run `cargo test --all-targets --locked` from the checkout.
3. Save a UTF-8 `.sely` source file and use `cargo run -- check`, followed by `cargo run -- run`.
4. Use only the documented CPU tensor subset until a separately verified backend exists.

The broader commands remain planned; the currently executable subset is:

```text
seloryn --version
seloryn check hello.sely
seloryn run hello.sely
```

## Runnable pure hello-world source

```seloryn
fn main() -> Text:
    return "Hello, Seloryn"
```

The interpreter returns the text value; it does not provide a `print` effect.
[`examples/tensor-matmul.sely`](../../examples/tensor-matmul.sely) is a checked
CPU tensor example.

## File rules

Sources use `.sely`, UTF-8 and either LF or CRLF on input. The canonical formatter emits LF. Semantic indentation consists of four ASCII spaces; leading tab characters in blocks are rejected. Editors should display whitespace. The module resolver must never execute an arbitrary path just because a program imports a name.

See [CLI](cli.md), [platforms](platforms.md), [language specification](../reference/grammar.md), and [current status](../project/status.md). The broader design remains a proposal until each runtime gate has evidence.
