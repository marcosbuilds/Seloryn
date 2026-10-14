# Getting started

**Availability:** Seloryn 0.00 currently provides an experimental syntax and semantic checker, deterministic core IR, pure interpreter, local project modules, bounded JSON capability, narrow training fixtures and an experimental `.selyapp` bundle path in the development repository. There is no verified release, package registry, hosted API or supported installation command; local installer scripts are development tooling only.

The checker rejects malformed or semantically inconsistent source before execution. The [status page](../project/status.md) distinguishes implemented behavior from future design.

## What you can do today (E3 checker and E4 interpreter)

When built from the repository, the native CLI accepts:

```text
seloryn --help
seloryn --version
seloryn check example.sely
seloryn run example.sely
seloryn check project-directory
seloryn run project-directory
seloryn build project-directory --profile debug --output artifact-directory
seloryn exec artifact-directory
```

`check` validates UTF-8, four-space layout, the implemented parser subset and selected semantic rules: local names and scopes, scalar types, typed list indexing and iteration, mutable `var` assignment, function calls and returns, records, enums, `Result`, `Option`, `require`, postfix `?`, pipelines and exhaustive `match`. It emits a structured source diagnostic and nonzero exit code for invalid input. The command path is implemented in [`src/main.rs:82`](../../src/main.rs#L82) and exercised by [`tests/bootstrap_cli.rs:61`](../../tests/bootstrap_cli.rs#L61).

`run` executes the same bounded constructs after all front-end diagnostics
pass. A standalone source run has no filesystem, network, process or
accelerator effects; a project run may perform the explicitly bounded JSON
reads granted by its project root. See the [JSON project example](../../examples/projects/json-config/README.md) for a complete multi-file invocation.
The N1/N2 bridge can allocate bounded CPU tensors and run checked F32/I64
kernels plus the tested N2 F32 numeric primitives.
A valid program starts at `fn main()`, and its returned value is rendered to
stdout.

## What's not yet available

- No package registry, external dependency resolver or broad standard library runtime.
- No GPU, distributed or symbolic-shape execution; N1/N2 support only bounded CPU tensors.
- No HTTP, process or unrestricted file/package runtime; only root-confined JSON reads are available in projects.
- No general model loading, optimizer, MLP training or production model serving; only the narrow fixture contracts are implemented.
- No GPU, distributed backend, supported installer or supported release target; local installer scripts are not a release.

The current capability limits are recorded in the [status page](../project/status.md) and the [tensor reference](../compute/tensors.md).

## Development run

From a built development binary, the current evidence path is:

1. Clone the development repository; its `rust-toolchain.toml` pins the
   development Rust toolchain and the required `rustfmt`/`clippy` components.
2. Run `cargo test --all-targets --locked` from the checkout.
3. Save a UTF-8 `.sely` source file and use `cargo run -- check`, followed by `cargo run -- run`.
4. Use only the documented CPU tensor subset until a separately verified backend exists.

The broader commands remain planned; the currently executable subset is:

```text
seloryn --version
seloryn check hello.sely
seloryn run hello.sely
seloryn build project-directory --profile debug --output artifact-directory
seloryn exec artifact-directory
```

## Runnable pure hello-world source

```seloryn
fn main() -> Text:
    return "Hello, Seloryn"
```

The interpreter returns the text value; it does not provide a `print` effect.
[`examples/tensors/tensor-matmul.sely`](../../examples/tensors/tensor-matmul.sely) and
[`examples/tensors/tensor-numeric.sely`](../../examples/tensors/tensor-numeric.sely) are checked
CPU tensor examples. Project examples are listed in
[`examples/README.md`](../../examples/README.md).

## File rules

Sources use `.sely`, UTF-8 and either LF or CRLF on input. No canonical
formatter is available yet; semantic indentation consists of four ASCII spaces
and leading tab characters in blocks are rejected. Editors should display
whitespace. The module resolver must never execute an arbitrary path just
because a program imports a name.

See [CLI](cli.md), [platforms](platforms.md), [language specification](../reference/grammar.md), and [current status](../project/status.md). The broader design remains a proposal until each runtime gate has evidence.
