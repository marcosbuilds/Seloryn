# Visual references

![Seloryn mark](../../assets/brand/seloryn-symbol.svg)

| Asset | Purpose | Origin |
|---|---|---|
| `assets/brand/seloryn-symbol.svg` | Canonical language, runtime, extension and `.sely` file identity | Original project asset, canonical vector |
| `assets/diagrams/architecture-flow.png` | Design pipeline from source to future runtime/backends; the bounded checker, interpreter and CPU tensor path exist today | Original project raster diagram |
| `assets/diagrams/training-flow.png` | Design-only forward pass, gradient and optimizer flow; only narrow scalar/autograd and linear fixtures exist today | Original project raster diagram |
| `assets/fonts/*` | Installer/display faces and their source/license metadata | Mixed project-owned and third-party assets; see [`assets/README.md`](../../assets/README.md) |
| `assets/installer/*` | Native setup artwork | Original project assets |
| `packages/fonts/seloryn-font/` | Experimental installable Seloryn Font family, Light 300 and Bold 700 | Supplied official candidate; see its [`package.toml`](../../packages/fonts/seloryn-font/package.toml) |
| `theSVG` links | Distinguish outside vendors/services visually | External brand assets; not bundled |

### External SVG sourcing rule

For vendor visuals, the project uses the catalog at https://thesvg.org/ as the lookup/download source, such as [GitHub](https://thesvg.org/icon/github), [Windows](https://thesvg.org/icon/windows) and [Linux](https://thesvg.org/icon/linux). Download only the exact variant listed in the theSVG metadata, check its **per-icon license** and register it in the asset inventory before inclusion. The official catalog points to a static `https://thesvg.org/icons/{slug}/{variant}.svg` endpoint. No icon found on that service should be misrepresented as the Seloryn logo.

### Identity

Mark: a continuous, compact split-path glyph suggesting structured flows converging into one execution. The public SVG is the source of truth; package-local raster/vector copies are derived assets and are checked against it. It must remain legible in single-color and 16×16 contexts; no glossy gradients or vendor lookalikes.
