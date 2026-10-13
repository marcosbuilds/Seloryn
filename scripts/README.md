# Scripts

Scripts are grouped by their operational purpose:

- `ci/` runs the local format, Rust, probe and example checks used by the
  development workflow.
- `install/` contains experimental scripts that install an already-built
  binary into a caller-selected directory. They do not download releases or
  establish platform support.

Run scripts from any working directory; each script resolves the repository
root from its own location.
