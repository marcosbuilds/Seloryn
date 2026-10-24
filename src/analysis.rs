use crate::diagnostic::Span;
use crate::parser::{Directive, Item, Program};
use std::path::Path;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SymbolKind {
    Function,
    Binding,
    Record,
    Enum,
}

impl SymbolKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Binding => "binding",
            Self::Record => "record",
            Self::Enum => "enum",
        }
    }

    pub fn lsp_kind(self) -> i64 {
        match self {
            Self::Function => 12,
            Self::Binding => 13,
            Self::Record => 5,
            Self::Enum => 10,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub public: bool,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImportKind {
    Local,
    Standard,
}

impl ImportKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Standard => "standard",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Import {
    pub module: String,
    pub symbols: Vec<String>,
    pub alias: Option<String>,
    pub kind: ImportKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileContext {
    pub path: String,
    pub symbols: Vec<Symbol>,
    pub imports: Vec<Import>,
}

pub fn symbols(program: &Program) -> Vec<Symbol> {
    program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Function(value) => Some(Symbol {
                name: value.name.clone(),
                kind: SymbolKind::Function,
                public: value.public,
                span: value.span,
            }),
            Item::Binding(value) => Some(Symbol {
                name: value.name.clone(),
                kind: SymbolKind::Binding,
                public: false,
                span: value.span,
            }),
            Item::Record(value) => Some(Symbol {
                name: value.name.clone(),
                kind: SymbolKind::Record,
                public: value.public,
                span: value.span,
            }),
            Item::Enum(value) => Some(Symbol {
                name: value.name.clone(),
                kind: SymbolKind::Enum,
                public: value.public,
                span: value.span,
            }),
            _ => None,
        })
        .collect()
}

pub fn imports(program: &Program) -> Vec<Import> {
    program
        .items
        .iter()
        .filter_map(|item| {
            let Item::Directive(Directive { keyword, text, .. }) = item else {
                return None;
            };
            if keyword == "import" {
                let mut parts = text.split_whitespace();
                let module = parts.next()?.to_owned();
                let alias = parts
                    .next()
                    .filter(|part| *part == "as")
                    .and_then(|_| parts.next())
                    .map(str::to_owned);
                return Some(Import {
                    kind: if is_standard_module(&module) {
                        ImportKind::Standard
                    } else {
                        ImportKind::Local
                    },
                    module,
                    symbols: Vec::new(),
                    alias,
                });
            }
            if keyword == "from" {
                let mut parts = text.split_whitespace();
                let module = parts.next()?.to_owned();
                let _import = parts.next()?;
                let symbols = parts
                    .map(|part| part.trim_end_matches(',').to_owned())
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>();
                if symbols.is_empty() {
                    return None;
                }
                return Some(Import {
                    kind: if is_standard_module(&module) {
                        ImportKind::Standard
                    } else {
                        ImportKind::Local
                    },
                    module,
                    symbols,
                    alias: None,
                });
            }
            None
        })
        .collect()
}

pub fn file_context(path: &Path, root: &Path, program: &Program) -> FileContext {
    let relative = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    FileContext {
        path: relative,
        symbols: symbols(program),
        imports: imports(program),
    }
}

pub fn is_standard_module(module: &str) -> bool {
    matches!(
        module,
        "io" | "json" | "models" | "autograd" | "training" | "data" | "tensor" | "audit"
    )
}

#[cfg(test)]
mod tests {
    use super::{ImportKind, SymbolKind, imports, symbols};
    use crate::parser::parse;

    #[test]
    fn extracts_top_level_symbols_and_imports_from_one_program() {
        let source = "import data as source\nfrom local.contract import Contract, helper\npub record Contract:\n    value: I64\nfn main() -> Unit:\n    let count: I64 = 1\n    return\n";
        let program = parse(source).program.expect("fixture parses");
        let symbols = symbols(&program);
        assert_eq!(
            symbols
                .iter()
                .map(|symbol| (symbol.name.as_str(), symbol.kind, symbol.public))
                .collect::<Vec<_>>(),
            vec![
                ("Contract", SymbolKind::Record, true),
                ("main", SymbolKind::Function, false),
            ]
        );
        let imports = imports(&program);
        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].kind, ImportKind::Standard);
        assert_eq!(imports[0].alias.as_deref(), Some("source"));
        assert_eq!(imports[1].kind, ImportKind::Local);
        assert_eq!(imports[1].symbols, ["Contract", "helper"]);
    }
}
