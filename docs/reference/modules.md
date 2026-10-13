# Modules and packages

```seloryn
module app.routes
import seloryn.http as http
from app.validation import validate_request
```

- Every `.sely` file maps to at most one declared module namespace; package manifests declare the root namespace.
- Resolution order is deterministic: current package modules → standard library → locked external package graph, with ambiguity rejected. No imports by untrusted runtime URLs or unchecked filesystem paths.
- `import path as alias` binds a namespace; `from path import Item` binds an exported item by name. Hidden imports cannot shadow local declarations without explicit diagnostic.
- `pub` marks externally visible declarations; private is default. Cyclic type references may be supported, cyclic initialization is rejected with exact cycle diagnostics.
- Package metadata must include license, target language capability range, platform targets, integrity hashes, lockfile schema and reproducible installation behavior.
- A local `seloryn.toml` project manifest and deterministic source-root resolver are implemented experimentally for the F3/B1 subset. A package registry, lockfile resolver and `seloryn add` do **not** exist in this archive.

### Future package separation

Core: math/scalars, files, text, time, paths, JSON, Result, diagnostics. Standard modular extras: networking, servers, tensors, autodiff, neural networks, tokenizers, datasets, distributed execution and checkpoint I/O. Heavy backends must not be imported or initialized for unrelated command-line scripts.
