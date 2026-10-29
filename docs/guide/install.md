# Installation and binary distributions

The language repository contains the Rust runtime source and reproducible
runtime packaging checks. The Windows installer, VS Code extension and font
family are distributed as separate GitHub Release assets; their builders and
sources are not part of this public repository tip.

## Runtime

The current verified-preview runtime release is listed in
[`packages/catalog.toml`](../../packages/catalog.toml). On Windows x64, obtain
the exact ZIP from the `seloryn-v0.12` release and verify its SHA-256 before
extracting it. Linux x64 and macOS arm64 do not have a published `0.12.0`
asset yet and are not claimed by this catalog entry.

The runtime remains a verified preview with an unstable ABI. A release asset
does not establish signing, a stable ABI, a clean-machine support promise or a
production SLA.

## Windows installer

The unsigned Windows x64 installer preview is published separately at
[`installer-v0.12.0`](https://github.com/marcosbuilds/Seloryn/releases/tag/installer-v0.12.0).
Use the manifest and `SHA256SUMS` asset from that release. The catalog records
the ZIP and headless executable hashes. Signing and clean-machine lifecycle
validation remain unclaimed.

## VS Code extension

Install the exact VSIX from
[`vscode-v0.12.0`](https://github.com/marcosbuilds/Seloryn/releases/tag/vscode-v0.12.0)
with VS Code's normal “Install from VSIX…” action, then verify the published
SHA-256. The public repository tests the Rust LSP protocol; extension-specific
build and packaging tests run in the private distribution pipeline.

A VSIX is executable extension content and is inspectable after download. The
repository boundary means its source is not tracked in the public language
repository; it does not make distributed extension code secret.

## Fonts

The experimental binary font distribution is published at
[`fonts-v2.0.0`](https://github.com/marcosbuilds/Seloryn/releases/tag/fonts-v2.0.0).
The release contains font binaries, metadata, CSS and licenses, but not its
generator source. Verify `SHA256SUMS` before use.

## What this repository does not provide

- a stable runtime or stable ABI;
- a package registry or automatic network installer;
- a signed installer;
- published `0.12.0` Linux/macOS runtime assets;
- a claim that the VSIX, installer or fonts are source-secret after download.
