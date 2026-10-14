# Editor support

The public repository contains an experimental VS Code extension at
[`tooling/vscode`](../../tooling/vscode). It is evidence of editor support for
the current bounded language surface, not a complete Language Server Protocol
implementation or a supported release.

It provides:

- `.sely` association, comments, brackets, indentation and TextMate syntax
  highlighting;
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

They cover local-symbol extraction, prefix filtering, completion candidates,
prompt bounds, generated-text sanitization and the empty-provider path. A real
VS Code installation is still required to validate host rendering and live
model-provider behavior.
