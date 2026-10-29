# Seloryn distributions

This directory contains the public distribution catalog only. The runtime
language and Rust implementation remain in this repository; the VSIX,
Windows installer and font package are built outside the public source tree
and published as GitHub Release assets.

| Distribution | Current release | Download |
|---|---|---|
| Runtime, Windows x64 | `0.12.0`, verified-preview | [GitHub Release](https://github.com/marcosbuilds/Seloryn/releases/tag/seloryn-v0.12) |
| VS Code extension | `0.12.0`, verified-preview | [GitHub Release](https://github.com/marcosbuilds/Seloryn/releases/tag/vscode-v0.12.0) |
| Windows installer | `0.12.0`, unsigned verified-preview | [GitHub Release](https://github.com/marcosbuilds/Seloryn/releases/tag/installer-v0.12.0) |
| Seloryn Font | `2.0.0`, experimental binary distribution | [GitHub Release](https://github.com/marcosbuilds/Seloryn/releases/tag/fonts-v2.0.0) |

Use the exact asset URL and SHA-256 recorded in [`catalog.toml`](catalog.toml).
The catalog deliberately does not contain source paths, local build scripts or
claims of support for platforms whose assets are not published. A downloaded
VSIX is executable extension content and can be inspected by its consumer; the
boundary here is the public Git repository, not secrecy of a distributed
binary.
