# Visual references

![Seloryn mark](../../assets/seloryn-mark.svg)

| Asset | Purpose | Origin |
|---|---|---|
| `seloryn-mark.svg` | Language identity, future terminal/editor icon | Original project draft |
| `architecture-flow.svg` | Source → runtime → tensor backends | Original vector diagram |
| `training-flow.svg` | Forward pass → gradient → optimizer | Original vector diagram |
| `theSVG` links | Distinguish outside vendors/services visually | External brand assets; not bundled |

### External SVG sourcing rule

For vendor visuals, the project uses the catalog at https://thesvg.org/ as the lookup/download source, such as [GitHub](https://thesvg.org/icon/github), [Windows](https://thesvg.org/icon/windows) and [Linux](https://thesvg.org/icon/linux). Download only the exact variant listed in the theSVG metadata, check its **per-icon license** and register it in the asset inventory before inclusion. The official catalog points to a static `https://thesvg.org/icons/{slug}/{variant}.svg` endpoint. No icon found on that service should be misrepresented as the Seloryn logo.

### Identity

Mark: a continuous, compact split-path glyph suggesting structured flows converging into one execution. Proposed palette: near-black `#111920`, off-white `#F4FAF8` and mineral mint `#74D6B0`. Must remain legible in single-color and 16×16 contexts; no glossy gradients or vendor lookalikes.
