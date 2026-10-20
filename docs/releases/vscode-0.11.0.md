# Seloryn VS Code extension 0.11.0 / language 0.11

This local extension candidate follows the `N13-SELF-HOSTED-LANGUAGE-MODEL`
language line and records the editor-specific `EXT-N14-SELF-HOSTED-LANGUAGE-MODEL`
gain. Completions now expose `json.as_text_list` and a source-owned modular
language-model workflow snippet, so the editor surface points at the verified
project without inventing a native trainer API.

The package is locally testable and remains a verified preview. It is not a
Marketplace publication and does not change the runtime ABI.
