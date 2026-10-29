# Downloads

This index was checked against the GitHub Releases API on 2026-10-29. The
repository currently has **no stable release**. The products below are
published previews, and the public Rust source remains the source of truth for
the language and runtime.

Use the [GitHub Releases page](https://github.com/marcosbuilds/Seloryn/releases)
as the primary release browser. The direct asset URLs are included only to
make the exact product/platform boundary and SHA-256 explicit; do not use the
Tags page or a `/releases/latest` alias as a substitute for this evidence.

## Stable

No stable runtime, installer, VS Code extension or font release is published.

## Preview

| Product | Verified release | Platform/channel | Exact asset | SHA-256 |
|---|---|---|---|---|
| Runtime 0.12.0 | [release](https://github.com/marcosbuilds/Seloryn/releases/tag/seloryn-v0.12) | Windows x64 / `verified-preview` | [ZIP](https://github.com/marcosbuilds/Seloryn/releases/download/seloryn-v0.12/seloryn-runtime-0.12.0-windows-x64.zip) | `3de3b6ff405d0b71801cccd45e99c1981ea421dd87b3ec9836341ce6ca290999` |
| VS Code extension 0.12.0 | [release](https://github.com/marcosbuilds/Seloryn/releases/tag/vscode-v0.12.0) | VS Code / `verified-preview` | [VSIX](https://github.com/marcosbuilds/Seloryn/releases/download/vscode-v0.12.0/seloryn-language-0.12.0.vsix) | `8be2dc5b87cf2b3171618ce73215148c6f181fbadb3b55e38bed9020848f7fdc` |
| Windows installer 0.12.0 | [release](https://github.com/marcosbuilds/Seloryn/releases/tag/installer-v0.12.0) | Windows x64 / unsigned `verified-preview` | [ZIP](https://github.com/marcosbuilds/Seloryn/releases/download/installer-v0.12.0/SelorynSetup-0.12.0-windows-x64.zip) | `b397415d7b3ad8918a5ed28a4a75ba6da6fd7578acc212c4c1d33a14c9775837` |
| Windows installer headless 0.12.0 | [release](https://github.com/marcosbuilds/Seloryn/releases/tag/installer-v0.12.0) | Windows x64 / unsigned `verified-preview` | [EXE](https://github.com/marcosbuilds/Seloryn/releases/download/installer-v0.12.0/SelorynSetup-0.12.0-windows-x64-headless.exe) | `90f0f6d294770c9f481d0dc03aad58844985e337e950716abc7acd97358b628e` |
| Font 2.0.0 | [release](https://github.com/marcosbuilds/Seloryn/releases/tag/fonts-v2.0.0) | Binary font distribution / experimental | [ZIP](https://github.com/marcosbuilds/Seloryn/releases/download/fonts-v2.0.0/seloryn-font-2.0.0.zip) | `3efcfff21b3fd204ee18378e1353d6b6c22d89dfea28a3734a7db5776981fb80` |

All rows above also have a `SHA256SUMS` asset on their release. The runtime
0.12.0 release has no Linux or macOS asset, so this repository makes no claim
for those platforms on the current line. The installer is unsigned; the VSIX
is inspectable executable extension content; the public/private boundary does
not imply secrecy of a distributed binary.

The public repository does not contain the installer UI/backend, VS Code
extension source, font source/generators or font files. Those inputs belong to
the separate private product package and are not required to compile or study
the language/runtime.
