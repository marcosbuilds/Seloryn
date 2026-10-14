# Seloryn for VS Code

This directory contains the experimental Seloryn editor extension. It is
public evidence of editor support, not a claim that Seloryn has a complete
LSP, formatter or stable release.

It currently provides:

- `.sely` file association, comments, brackets, indentation and TextMate
  syntax highlighting;
- a Seloryn extension mark and an optional `Seloryn File Icons` theme that
  maps both the `seloryn` language id and `.sely` files to the Seloryn mark;
- native VS Code completion items for local declarations, parameters, types,
  implemented builtins, keywords and reserved syntax;
- opt-in inline ghost text through the VS Code Language Model API when a
  provider is available.

AI ghost text is disabled by default. Enable `seloryn.aiGhostText.enabled`
only when sending the active document context to the configured model provider
is acceptable. If the API is unavailable, no model is selected, a request is
cancelled or a provider rejects a request, the extension returns no suggestion
and does not pretend that AI generation occurred.

## Development validation

The tests do not require VS Code or an installed model provider. They inject a
small API double and exercise completion filtering, local symbol discovery,
prompt bounds, generated-text sanitization and the empty-provider fallback:

```text
npm test --prefix tooling/vscode
```

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

The result is a local `seloryn-language-0.0.1.vsix`. The package contains the
extension runtime, grammar, language configuration, README, logo and file-icon
theme; tests, fixtures and the lockfile stay out of the VSIX.
The packaging step fixes ZIP entry ordering and timestamps, so repeated builds
from the same locked dependency tree produce the same local artifact.

The extension manifest icon identifies the extension itself. To show the
Seloryn mark beside `.sely` files in the Explorer, select `Seloryn File Icons`
from `Preferences: File Icon Theme`. The extension does not change the user's
global icon-theme setting automatically.

For an interactive VS Code check, open the repository as an extension
development host with `tooling/vscode` as the extension directory. A real
VS Code installation and a configured Language Model provider are required for
visual highlighting and live ghost text; the repository does not claim that
those external conditions were available in headless tests.
