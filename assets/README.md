# Seloryn assets

Shared assets are grouped by their role. Package-specific copies remain inside
their package when the packaging tool requires a self-contained archive.

```text
assets/
├── brand/       canonical Seloryn identity vectors
├── diagrams/    public architecture and training diagrams
├── fonts/       project faces and the licensed Sora source family
└── installer/   native setup artwork
```

The two Seloryn font files have different responsibilities: `seloryn-display`
is the generated display face used by the font QA path, while
`seloryn-signature-regular` is the supplied installer face. They are not
merged because they have different glyph sets, metadata and runtime roles.
`sora-variable.ttf` is not a Seloryn font; it is the OFL-licensed source family
used as the scaffold for the generated display face. Its license is kept beside
the font in `fonts/sora-ofl.txt`. The vendored variable file matches upstream
commit `7f9a9c5d0ccd1c099cfac420aa27133df1c5fdc4`; its source and download URL
are recorded in `../packages/catalog.toml`.

The native setup embeds its font and artwork for the setup process. It does not
install a system-wide font or claim that a public font installer exists.
