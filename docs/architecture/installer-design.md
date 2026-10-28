# Native installer visual and interaction design

The Windows setup is a fixed-size native wizard for the current `0.12.0`
verified preview. Its client area is `880 × 560` pixels and its window style
does not expose a resize frame or maximize button. The fixed geometry is
intentional: it prevents the old absolute-positioned controls from shrinking
below their content and makes the visual panel and operational panel stable.

## Visual system

The left side uses the Seloryn palette: `#06110E` background, `#0E2520`
surfaces, `#34CCA7` primary accent, `#148874` secondary accent, `#F1F7F4`
primary text and `#B1C9C1` secondary text. The right side is a fixed `300px`
brand panel with a white surface and the official two-strand symbol geometry
from `assets/brand/seloryn-symbol.svg`, drawn directly by the native UI. The panel
uses only restrained linework, a rounded plate and the logo; there is no
external background artwork to stretch or crop. The left side contains only
setup controls and state; the panel carries the identity and runtime metadata.

`assets/fonts/seloryn-signature-regular.ttf` is the display face supplied for the
installer. It is loaded with `AddFontMemResourceEx` and removed when the setup
exits; it is never installed into Windows. Body copy and logs remain on system
UI/monospace faces for readability.

`assets/installer/seloryn-folder.svg` defines the folder icon geometry. The Win32 setup
draws the same small vector shape in the directory button because stock Win32
controls do not render SVG directly.

## Staged flow

1. **Welcome** — shows runtime, language, target and preview channel.
2. **Location** — shows the preselected per-user path, a real folder chooser,
   PATH/default options and an explicit checkbox to clean previous registered
   runtime versions or preserve them side by side. Cleanup never touches user
   projects.
3. **Install** — runs the existing Rust transaction on a worker thread and
   streams typed progress events into a read-only log. The UI thread stays on
   the message loop.
4. **Done** — is a failure/retry fallback. On successful verification the setup
   closes and starts a new `cmd.exe` with `seloryn --version` on the freshly
   installed `bin` path.

Progress values advance only after real payload, manifest, post-install and
PATH operations report completion. A failed worker event stays visible in the
log and exposes retry without hiding the error.

## Scope and limits

This is an initial app-only typeface and a one-runtime setup flow. It does not
claim a complete Seloryn brand family, system-wide font installation,
multiversion selection UI or clean-machine compatibility. The next visual gate
is to exercise the built executable at 100%, 125% and 150% Windows scale and
capture the real wizard states before calling the UI polished.
