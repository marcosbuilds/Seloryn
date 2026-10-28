# Seloryn assets

Shared assets are grouped by their role. Package-specific copies remain inside
their package when the packaging tool requires a self-contained archive.

```text
assets/
├── brand/       canonical Seloryn wordmark and symbol vectors
├── diagrams/    public architecture and training diagrams
├── fonts/       project faces and the licensed Sora source family
└── installer/   native setup artwork
```

`brand/seloryn-logo.svg` is the complete identity lockup: official S plus the
Seloryn wordmark. Use it whenever the product name is being presented. The
isolated `brand/seloryn-symbol.svg` is reserved for app/extension marks, compact
avatars and short visual references.

`fonts/seloryn-font-light.ttf` and `fonts/seloryn-font-bold.ttf` are the v2
Seloryn Font supplied with the installer redesign. The WinUI frontend uses the
light face for readable copy and the bold face for the wordmark/display tier;
the headless/native fallback embeds the same pair. The old generated display
face and Sora remain vendored only for compatibility with unrelated existing
assets and are no longer the modern installer typography.

The native setup embeds its font and artwork for the setup process. It does not
install a system-wide font or claim that a public font installer exists.
