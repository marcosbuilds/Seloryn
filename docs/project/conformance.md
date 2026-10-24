# Public conformance and evidence matrix

Seloryn's public capability claims are mapped in the source-owned
[`conformance_matrix.json`](../../tests/conformance_matrix.json). The matrix is
deliberately executable: every implemented row names its implementation paths,
positive probes, negative probes, platform requirements, documentation and
limits. The probes are the existing independent scripts and tests; the runner
only validates coverage and orchestrates them.

## Run it

From the repository root, after building the release binary:

```text
python scripts/ci/conformance.py --validate
python scripts/ci/conformance.py --list
python scripts/ci/conformance.py --run --suite core
```

The runner returns a non-zero exit code for an invalid matrix, a missing
source/documentation link, or a failed executable probe. A `documented-boundary`
result is intentional evidence that a capability is explicitly outside the
current implementation; it is not a simulated test pass.

Suites are scoped so that platform and tool requirements stay visible:

| Suite | Scope | Typical requirement |
|---|---|---|
| `core` | language, runtime, CPU training, receipts, jobs and operational probes | Rust release binary and Python 3.12 |
| `editor` | LSP and VS Code package/integration probes | Node.js, npm dependencies and release binary |
| `hardware` | physical GPU inventory and scalar GPU-linear proof | compatible physical adapter |
| `release` | staging provenance and Windows installer proof | release dependencies; Windows for installer |
| `benchmark` | repeated timing evidence | controlled environment for promotion |

Use `--capability <id>` to run the probes attached to one row. For example:

```text
python scripts/ci/conformance.py --run --capability source-owned-language-model
```

## Status policy

The matrix uses only the public contract labels:
`design-only`, `reserved`, `experimental-implemented`, `verified-preview`,
`supported-stable`, `deprecated` and `unsupported`.

`verified-preview` means the named bounded behavior was reproduced by the
listed evidence. It does not promote neighboring or more general behavior.
Rows marked `experimental-implemented` have executable implementation and
tests but do not yet satisfy the project's stronger preview boundary. Rows
marked `unsupported` have no positive probe and must carry a documented
boundary probe. A probe may appear in both positive and negative columns when
the existing black-box script asserts both a successful path and rejection or
tamper cases; the runner executes that command once and preserves both roles in
the matrix.

This matrix is not a benchmark leaderboard. The benchmark suite records timing
for the declared fixture, while promotion or a performance claim still needs a
controlled comparison and independent review. Hardware-aware rows remain
capability-aware on hosts without the declared adapter.

The matrix is checked in CI through the portable core suite and is also
available for local editor, release and hardware verification. It does not
change `VERSION.toml`, create a release, or make a stable ABI claim.
