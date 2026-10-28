# Seloryn installer visual and interaction design

The user-facing Windows setup is a WinUI 3/XAML application built with the
Windows App SDK and C#/.NET. It is deliberately a compact, fixed-size wizard
instead of a custom-painted Win32 form. The renderer is the Windows compositor,
so text, SVG images, controls, focus states and antialiasing remain crisp at
100%, 125% and 150% scale.

## Runtime boundary

The frontend lives in `installer-ui/`. It owns the visual system, navigation,
folder picker, keyboard-accessible controls and progress presentation. The
Rust `seloryn-setup` binary remains the transactional installer backend. The
release build embeds that backend as `Assets/seloryn-setup-backend.exe` and
invokes it with `--headless-progress`; the frontend parses real progress events
instead of simulating a timer.

The shipped modern build is an unpackaged, self-contained WinUI app folder
distributed as a ZIP. This keeps the Windows App SDK native runtime beside the
entrypoint and avoids a fragile single-file binary that would be hundreds of
megabytes after native self-extraction. The release also emits a separate
headless executable for CI and recovery workflows.

## Visual system

The canvas is absolute black `#000000`, with near-black green surfaces,
white/mint text and a restrained Seloryn green accent. The actual
`assets/brand/seloryn-logo.svg` is used wherever the product name is presented;
the isolated `assets/brand/seloryn-symbol.svg` is kept for compact app/mark
contexts. No logo is redrawn by hand. The v2 `Seloryn Font` files supplied in
`assets/fonts/seloryn-font-light.ttf` and `assets/fonts/seloryn-font-bold.ttf`
drive the frontend's copy and display tiers. Nothing is installed system-wide.

Three hairline Bézier flows sit behind the content at very low contrast. They
reuse the same split-flow vocabulary as the mark. Their eased dash-offset
animations continuously travel forward and back at different durations, while
the complete wordmark keeps a short entry reveal. The motion is decorative,
pointer-transparent and deliberately quiet behind the actual content.

The layout has one controlled scroll region: the installation log. Welcome,
location and done stages fit their fixed client area, so content is not hidden
below an unscrollable card. Checkbox controls are native WinUI controls with
real toggle state and keyboard focus. Progress is driven by backend events and
shows the percentage beside the live status.

## Staged flow

1. **Welcome** — identifies the verified Seloryn runtime and install model.
2. **Location** — chooses a per-user root and exposes PATH, default-runtime and
   cleanup options. Cleanup only removes registered Seloryn versions; projects
   are preserved.
3. **Install** — streams the Rust transaction into a readable monospace log.
4. **Done** — confirms the final path and lets the user explicitly open a new
   terminal. No terminal is forced open after installation.

## Build and validation

`scripts/release/build-installer.ps1` builds the Rust payloads, publishes the
WinUI frontend with the .NET SDK on `D:\Toolchains\dotnet`, creates the
Windows-only `SelorynSetup-experimental-beta-windows-x64.zip`, and validates the
separate headless backend. The output manifest labels this as an experimental
beta without public installer versioning, identifies `SelorynSetup.exe` as the
modern entry point and records the ZIP hash. The embedded runtime keeps its
internal compatibility package metadata for the install transaction.
