# Seloryn packages

This directory contains installable package sources and the public package
catalog. Generated binaries and VSIX files are build outputs; they belong in a
release archive or a GitHub Release, not in the source tree.

| Package | Source | Current local line | Latest published preview |
|---|---|---:|---|
| Runtime | [`Cargo.toml`](../Cargo.toml) and [`scripts/release`](../scripts/release) | `0.12.0` | [`seloryn-v0.07`](https://github.com/marcosbuilds/Seloryn/releases/tag/seloryn-v0.07) |
| VS Code extension | [`vscode/`](vscode) | `0.12.0` local candidate | [`vscode-v0.4.0`](https://github.com/marcosbuilds/Seloryn/releases/tag/vscode-v0.4.0) |
| Windows setup | [`scripts/release/build-installer.ps1`](../scripts/release/build-installer.ps1) | `0.12.0` local candidate | Not published |
| Seloryn Font | [`fonts/seloryn-font/`](fonts/seloryn-font) | `2.0.0` experimental candidate | Not published |
| Bundled app faces | [`assets/fonts/`](../assets/fonts) | embedded project assets | Not published as a standalone installer |

The exact status, official download URLs and compatibility lines are recorded
in [`catalog.toml`](catalog.toml). A local candidate is never described as an
official release. The latest published preview is the source of truth for
downloads until a newer GitHub Release is actually created.
