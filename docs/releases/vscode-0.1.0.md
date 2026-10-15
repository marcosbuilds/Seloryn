# Seloryn Language for VS Code 0.1.0

Experimental release of the Seloryn editor extension.

Included:

- `.sely` language association, indentation and expanded syntax highlighting;
- native completions for local symbols, types, implemented builtins and
  language keywords;
- saved-file diagnostics from the real Seloryn CLI structured JSON output;
- the Seloryn mark for the extension and the optional `.sely` file-icon theme;
- opt-in VS Code Language Model ghost text, disabled by default.

This release is experimental. It is not a full LSP, formatter, debugger or
runtime distribution. The extension's CLI diagnostics require a compatible
Seloryn executable configured through `seloryn.cliPath` or available as
`seloryn` on `PATH`.

The extension package is versioned independently from the language capability
version. This release corresponds to language `0.01` and the accepted
`N3B-TENSOR-AUTODIFF` gain; see [`VERSION.toml`](../../VERSION.toml).
