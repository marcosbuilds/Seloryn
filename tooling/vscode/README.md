# Seloryn for VS Code

This directory contains the verified-preview Seloryn editor extension. It is
public evidence of editor support, not a claim that Seloryn has a complete
LSP, formatter or stable runtime release.

The package version is controlled by the root `VERSION.toml`, not by an
independent hand-edited number. Before tagging a release, run
`python scripts/release/verify-version.py` from the repository root. The
verifier compares the previous and current SemVer values, requires the
append-only extension gain ledger and checks that the manifest and lockfile
agree. Keep the extension version unchanged for documentation-only changes.

The diagnostic bridge also exposes strict project-manifest failures from the
CLI, including malformed TOML and a project language newer than the runtime;
the public CLI integration test covers these errors without reimplementing
manifest semantics in the extension.

It currently provides:

- `.sely` file association, comments, brackets, indentation and TextMate
  syntax highlighting;
- a Seloryn extension mark and an optional `Seloryn File Icons` theme that
  maps both the `seloryn` language id and `.sely` files to the Seloryn mark;
- native VS Code completion items for local declarations, parameters, types,
  implemented builtins including multi-input `autograd.gradients`, bounded
  `data.range` and `training.train_mlp`, real training snippets,
  keywords and reserved syntax;
- a `Seloryn: Run CPU Training in Terminal` command that opens the configured
  CLI with an argument vector and `--log=human` for live progress;
- `Seloryn: Inspect Hardware Devices` and `Seloryn: Run GPU Compute Probe`
  commands that expose adapter evidence and the bounded compute/readback gate;
- saved-file diagnostics from the configured Seloryn CLI, using its structured
  JSON output without reimplementing parser or type semantics;
- opt-in inline ghost text through the VS Code Language Model API when a
  provider is available.

AI ghost text is disabled by default. Enable `seloryn.aiGhostText.enabled`
only when sending the active document context to the configured model provider
is acceptable. If the API is unavailable, no model is selected, a request is
cancelled or a provider rejects a request, the extension returns no suggestion
and does not pretend that AI generation occurred.

Diagnostics run only after a `.sely` file is saved. The extension invokes one
configured executable (`seloryn.cliPath`, default `seloryn`) with separate
arguments: `check <file-or-project> --format=json --color=never`. It enforces a
timeout and combined output limit, cancels a previous check when a newer save
starts, and clears diagnostics if the CLI is unavailable. It does not parse
shell commands, inspect unsaved buffers or turn a provider failure into a fake
language error.

## Development validation

The tests do not require VS Code or an installed model provider. They inject a
small API double and exercise completion filtering, local symbol discovery,
prompt bounds, generated-text sanitization and the empty-provider fallback:

```text
npm test --prefix tooling/vscode
```

After a release CLI build, the real CLI bridge can be checked with:

```text
npm run test:cli --prefix tooling/vscode
```

This fixture covers a saved path containing spaces and Unicode, CRLF input,
structured JSON failure output, source positions and the CLI's data-error exit
code.

The fixture [`examples/extension-evidence.sely`](examples/extension-evidence.sely)
is a real `.sely` source file used by the extension test. With a local Seloryn
development binary it can also be checked with:

```text
seloryn check tooling/vscode/examples/extension-evidence.sely
```

## Local VSIX

From this directory, install the pinned development dependency and build the
package with:

```text
npm ci
npm run package
```

The result is a local `seloryn-language-0.7.0.vsix`. The package contains the
extension runtime, grammar, language configuration, README, logo and file-icon
theme; tests, fixtures and the lockfile stay out of the VSIX.
The packaging step fixes ZIP entry ordering and timestamps, so repeated builds
from the same locked dependency tree produce the same local artifact.

The optional file-icon theme uses the canonical public Seloryn symbol for
`.sely` files. VS Code Marketplace packaging rejects SVG files as the extension
manifest icon, so the manifest uses a 128px PNG render of that same mark. The
package test fails if the packaged SVG source diverges from
`assets/seloryn-symbol.svg`.
To show the Seloryn mark beside `.sely` files in the Explorer, select `Seloryn File Icons`
from `Preferences: File Icon Theme`. The extension does not change the user's
global icon-theme setting automatically.

For an interactive VS Code check, open the repository as an extension
development host with `tooling/vscode` as the extension directory. A real
VS Code installation and a configured Language Model provider are required for
visual highlighting and live ghost text; the repository does not claim that
those external conditions were available in headless tests.
