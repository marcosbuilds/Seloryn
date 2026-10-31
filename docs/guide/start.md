# Getting started

**Availability:** Seloryn 0.12 currently provides a verified-preview CPU MLP profile, a complete source-owned `.sely` runtime-sized contextual language-model workflow with configurable one-hot and embedding/ReLU windows, whitespace text inference, perplexity and hash-bound checkpoints, reusable parameters/SGD, fused cross-entropy, hash-validated inference, read-only `doctor` project diagnostics, transactional `init` templates including `source-mlp` and the modular `trainable-mlp` workflow, reproducible local release provenance, identity-bearing replayable training receipts, typed JSON number-array and text-array ingestion, a Seloryn-authored tensor objective, multi-input gradients, bounded `range` and SGD loop. Project manifests are parsed strictly and future language versions are rejected before execution. It also includes a real hardware inventory, bounded GPU compute/readback probe and an explicit scalar F32 `from training import gpu` profile; general GPU tensor/MLP or language-model training is not enabled. There is no distributed backend, package registry, hosted API or stable ABI.

The checker rejects malformed or semantically inconsistent source before execution. The [status page](../project/status.md) distinguishes implemented behavior from future design.

## One reproducible first run

Use one of the scripts below from the repository root. Each run checks the
required tools, builds `target/release`, runs the read-only project doctor,
checks and runs a tracked CPU tensor example, and finishes with the portable
core conformance suite.

```sh
bash scripts/quickstart/first-run-posix.sh
```

```powershell
pwsh -NoProfile -File .\scripts\quickstart\first-run-powershell.ps1
```

The expected tensor line is:

```text
Tensor[F32, shape=[2, 2], values=[19.0, 22.0, 43.0, 50.0]]
```

The project doctor reports a warning and exits successfully because the local
runtime is `verified-preview` with an unstable ABI; that warning is expected
and is not a production-readiness claim. The scripts are safe to repeat: they
reuse the tracked examples and the release build directory, and do not create
or overwrite a user project.

Required tools are the Rust toolchain pinned in `rust-toolchain.toml` and
Python 3.10+ for the conformance runner. Node.js/npm are not required for this
CLI path; install them only for the separate VS Code/editor checks. If a
required tool is missing, the prerequisite doctor stops before build and names
the command to verify after installation. The dedicated regression probe is:

```text
python tests/first_run_probe.py
```

It covers the successful path, a repeat run, the missing-Python diagnostic and
the existing nonzero `run` failure contract.

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

`check` validates UTF-8, four-space layout, the implemented parser subset and selected semantic rules: local names and scopes, scalar types, typed list indexing and iteration, mutable `var` assignment, function calls and returns, records, enums, `Result`, `Option`, `require`, postfix `?`, pipelines and exhaustive `match`. It emits a structured source diagnostic and nonzero exit code for invalid input. The command path is implemented in [`src/cli/mod.rs`](../../src/cli/mod.rs) and exercised by [`tests/bootstrap_cli.rs`](../../tests/bootstrap_cli.rs).

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
- No public `Module`/`Parameter`/`Dataset` framework, general model registry or production model serving; the custom `.sely` checkpoint proof remains a bounded CPU contract.
- No distributed backend, supported installer or supported release target; local installer scripts are not a release. `gpu-probe` is hardware evidence, while `from training import gpu` is the separate closed scalar F32 training proof; neither implies general GPU tensor/MLP training.

The current capability limits are recorded in the [status page](../project/status.md) and the [tensor reference](../compute/tensors.md).

Use `doctor` before execution when integrating a project into automation. It
performs no project execution, network access or GPU dispatch. Use `init` to
create a minimal, tabular-MLP, source-owned `source-mlp` or modular
`trainable-mlp` layout that already
follows the manifest, capability and source-root rules; it refuses to overwrite
an existing path. The `source-mlp` template keeps the model, objective, SGD
loop, checkpoint and reload code in `.sely`.

## Development run

From a built development binary, the current evidence path is the quickstart
above or the equivalent commands below. The commands intentionally use the
release binary produced by the local build:

```text
cargo build --release --locked
./target/release/seloryn doctor examples/projects/json-config --format=json
./target/release/seloryn check examples/tensors/tensor-matmul.sely
./target/release/seloryn run examples/tensors/tensor-matmul.sely
python3 scripts/ci/conformance.py --run --suite core
```

On Windows PowerShell, use `target\release\seloryn.exe` and `python` in the
same sequence. The quickstart scripts select the platform binary and preserve
the native exit code of every step.

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
