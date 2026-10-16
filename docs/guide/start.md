# Getting started

**Availability:** Seloryn 0.09 currently provides a verified-preview CPU MLP profile, hash-validated batch inference, read-only `doctor` project diagnostics, transactional `init` templates, reproducible local release provenance, identity-bearing replayable training receipts, typed JSON number-array ingestion, a Seloryn-authored tensor objective, multi-input gradients, bounded `range` and SGD loop. Project manifests are parsed strictly and future language versions are rejected before execution. It also includes a real hardware inventory, bounded GPU compute/readback probe and an explicit scalar F32 `train_linear_gpu` profile; general GPU tensor/MLP training is not enabled. There is no distributed backend, package registry, hosted API, stable ABI or published runtime installer; the preview candidate is staged locally and the public runtime remains 0.07.

The checker rejects malformed or semantically inconsistent source before execution. The [status page](../project/status.md) distinguishes implemented behavior from future design.

## What you can do today (E3 checker and E4 interpreter)

When built from the repository, the native CLI accepts:

```text
seloryn --help
seloryn --version
seloryn devices --format=json
seloryn gpu-probe --format=json
seloryn doctor project-directory --format=json
seloryn init new-project --template=minimal
seloryn check example.sely
seloryn run example.sely
seloryn check project-directory
seloryn run project-directory
seloryn build project-directory --profile debug --output artifact-directory
seloryn exec artifact-directory
```

`check` validates UTF-8, four-space layout, the implemented parser subset and selected semantic rules: local names and scopes, scalar types, typed list indexing and iteration, mutable `var` assignment, function calls and returns, records, enums, `Result`, `Option`, `require`, postfix `?`, pipelines and exhaustive `match`. It emits a structured source diagnostic and nonzero exit code for invalid input. The command path is implemented in [`src/main.rs`](../../src/main.rs) and exercised by [`tests/bootstrap_cli.rs`](../../tests/bootstrap_cli.rs).

`run` executes the same bounded constructs after all front-end diagnostics
pass. A standalone source run has no filesystem, network, process or
accelerator effects; a program may explicitly consume one bounded JSON
document from standard input, while a project run may additionally perform the
explicitly bounded JSON reads granted by its project root. See the [JSON
project example](../../examples/projects/json-config/README.md) for a complete
multi-file invocation.
The N1/N2 bridge can allocate bounded CPU tensors and run checked F32/I64
kernels plus the tested N2 F32 numeric primitives.
A valid program starts at `fn main()`, and its returned value is rendered to
stdout.

## What's not yet available

- No package registry, external dependency resolver or broad standard library runtime.
- No GPU tensor/autodiff training, distributed or symbolic-shape execution; N1/N2 and MLP training remain bounded CPU contracts.
- No HTTP, process or unrestricted file/package runtime; only bounded standard-input JSON and root-confined project JSON reads are available.
- No public `Module`/`Parameter`/`Dataset` framework, model loading or production model serving; the verified training profiles remain bounded CPU contracts.
- No distributed backend, supported installer or supported release target; local installer scripts are not a release. `gpu-probe` is hardware evidence, while `train_linear_gpu` is the separate closed scalar F32 training proof; neither implies general GPU tensor/MLP training.

The current capability limits are recorded in the [status page](../project/status.md) and the [tensor reference](../compute/tensors.md).

Use `doctor` before execution when integrating a project into automation. It
performs no project execution, network access or GPU dispatch. Use `init` to
create a minimal or tabular-MLP layout that already follows the manifest,
capability and source-root rules; it refuses to overwrite an existing path.

## Development run

From a built development binary, the current evidence path is:

1. Clone the development repository; its `rust-toolchain.toml` pins the
   development Rust toolchain and the required `rustfmt`/`clippy` components.
2. Run `cargo test --all-targets --locked` from the checkout.
3. Save a UTF-8 `.sely` source file and use `cargo run -- check`, followed by `cargo run -- run`.
4. Use the documented CPU tensor/training subset; run `devices` and `gpu-probe` to inspect hardware evidence without assuming GPU training support.

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
