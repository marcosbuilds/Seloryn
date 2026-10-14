# Editor support

The public repository contains a VS Code editor preview at
[`tooling/vscode`](../../tooling/vscode). It supports the current bounded
language surface and can be packaged as a local VSIX. It is not a complete
Language Server Protocol implementation and is not a release of the Seloryn
runtime.

It provides:

- `.sely` association, comments, brackets, indentation and TextMate syntax
  highlighting;
- an optional `Seloryn File Icons` theme that maps `.sely` files to Seloryn's
  own mark;
- native completion items for local functions, parameters, bindings, current
  types, implemented builtins, keywords and reserved syntax;
- opt-in inline ghost text through the VS Code Language Model API.

AI ghost text is disabled by default. Enabling it can send bounded active-file
context to the configured VS Code model provider. When no provider exists, the
request is cancelled or the provider fails, the extension returns no
suggestion. It never presents a local fallback as AI-generated text.

The extension's independent Node tests run without VS Code:

```text
npm test --prefix tooling/vscode
```

Build and install the local editor preview with:

```text
npm ci --prefix tooling/vscode
npm run package --prefix tooling/vscode
code --install-extension tooling/vscode/seloryn-language-0.0.1.vsix
```

In VS Code, choose `Seloryn File Icons` from `Preferences: File Icon Theme` to
show the mark beside `.sely` files. The package is reproducible from its lock
file and excludes tests, fixtures and development dependencies.

They cover local-symbol extraction, prefix filtering, completion candidates,
prompt bounds, generated-text sanitization, package metadata and the
empty-provider path. A real VS Code installation can validate package
installation, but an interactive window is still required to validate Explorer
rendering and live model-provider behavior; those checks are not claimed here.
