# Scripts

Scripts are grouped by their operational purpose:

- `ci/` runs the local format, Rust, probe and example checks used by the
  development workflow. `scripts/ci/conformance.py` validates the public
  capability matrix and selects the existing probes by `core`, `editor`,
  `hardware`, `release` or `benchmark` suite.
- `install/` contains experimental scripts that install an already-built
  binary into a caller-selected directory. They do not download releases or
  establish platform support.
- `release/prepare-release.py` stages one coordinated runtime plus VSIX
  candidate with a compatibility manifest and external SHA-256 sums. It does
  not create tags, GitHub releases or publish anything.

Run scripts from any working directory; each script resolves the repository
root from its own location. Before committing or pushing a language change,
run `scripts/ci/verify-public.ps1` on Windows or `scripts/ci/verify-public.sh` on POSIX.
That gate includes formatting, all-target Rust tests, Clippy with warnings as
errors, release checks, independent numeric probes, and the exact public
project/bundle business-result probe. CI repeats the same classes of checks on
Ubuntu, macOS and Windows.

For a claim-to-evidence view, run `python scripts/ci/conformance.py --list`.
Use `--validate` to check every implementation and documentation path, or
`--run --suite core` to execute the portable public conformance proofs.

The Rust test commands intentionally pass `--test-threads=1`. Tensor tests
exercise a process-wide resident-buffer budget; serial scheduling keeps the
limit assertion independent of unrelated test allocations and prevents the
platform-specific ordering failures that previously made CI intermittent.
