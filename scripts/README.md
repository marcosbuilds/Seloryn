# Scripts

Scripts are grouped by their operational purpose:

- `ci/` runs the local format, Rust, probe and example checks used by the
  development workflow.
- `install/` contains experimental scripts that install an already-built
  binary into a caller-selected directory. They do not download releases or
  establish platform support.

Run scripts from any working directory; each script resolves the repository
root from its own location. Before committing or pushing a language change,
run `scripts/ci/check-n1.ps1` on Windows or `scripts/ci/check-n1.sh` on POSIX.
That gate includes formatting, all-target Rust tests, Clippy with warnings as
errors, release checks, independent numeric probes, and the exact public
project/bundle business-result probe. CI repeats the same classes of checks on
Ubuntu, macOS and Windows.
