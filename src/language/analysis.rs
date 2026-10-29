use crate::diagnostic::Span;
use crate::language::parser::{Directive, Item, Program};
use std::path::Path;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
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

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
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

/// Stable schema identifier for the bounded context package.
pub const CONTEXT_SCHEMA: &str = "seloryn.context.v1";

/// Explicit bounds for [`build_context`]. A zero bound is valid and means
/// that the corresponding collection is omitted. The package builder only
/// consumes `FileContext` metadata; it never opens or reads any path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContextLimits {
    pub max_files: usize,
    pub max_symbols: usize,
    pub max_imports: usize,
    pub max_path_bytes: usize,
    pub max_label_bytes: usize,
}

impl ContextLimits {
    pub const fn new(
        max_files: usize,
        max_symbols: usize,
        max_imports: usize,
        max_path_bytes: usize,
        max_label_bytes: usize,
    ) -> Self {
        Self {
            max_files,
            max_symbols,
            max_imports,
            max_path_bytes,
            max_label_bytes,
        }
    }
}

impl Default for ContextLimits {
    fn default() -> Self {
        Self::new(64, 512, 256, 256, 128)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub public: bool,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ContextImport {
    pub module: String,
    pub symbols: Vec<String>,
    pub alias: Option<String>,
    pub kind: ImportKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextFile {
    pub path: String,
    pub symbols: Vec<ContextSymbol>,
    pub imports: Vec<ContextImport>,
    pub total_symbols: usize,
    pub total_imports: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextPack {
    pub schema: &'static str,
    pub files: Vec<ContextFile>,
    pub total_files: usize,
    pub total_symbols: usize,
    pub total_imports: usize,
    pub omitted_files: usize,
    pub omitted_symbols: usize,
    pub omitted_imports: usize,
    pub truncated: bool,
}

/// Build a deterministic, compact context package from already-collected
/// file metadata.
///
/// The function does not resolve paths, inspect source files, execute code,
/// or include diagnostics, source text, tool arguments, or other unbounded
/// data. Paths and labels are normalized/capped and credential-shaped labels
/// are replaced with a fixed redaction marker.
pub fn build_context(files: &[FileContext], limits: ContextLimits) -> ContextPack {
    let mut entries = files
        .iter()
        .map(|file| compact_file(file, limits))
        .collect::<Vec<_>>();

    let total_files = entries.len();
    let total_symbols: usize = entries.iter().map(|file| file.total_symbols).sum();
    let total_imports: usize = entries.iter().map(|file| file.total_imports).sum();

    entries.sort_by(context_file_cmp);

    let omitted_files = total_files.saturating_sub(limits.max_files);
    let mut kept = entries;
    kept.truncate(limits.max_files);

    let mut remaining_symbols = limits.max_symbols;
    let mut remaining_imports = limits.max_imports;
    for file in &mut kept {
        if file.symbols.len() > remaining_symbols {
            file.symbols.truncate(remaining_symbols);
        }
        remaining_symbols = remaining_symbols.saturating_sub(file.symbols.len());

        if file.imports.len() > remaining_imports {
            file.imports.truncate(remaining_imports);
        }
        remaining_imports = remaining_imports.saturating_sub(file.imports.len());
    }

    let kept_symbols = kept.iter().map(|file| file.symbols.len()).sum::<usize>();
    let kept_imports = kept.iter().map(|file| file.imports.len()).sum::<usize>();
    let omitted_symbols = total_symbols.saturating_sub(kept_symbols);
    let omitted_imports = total_imports.saturating_sub(kept_imports);
    let truncated = omitted_files > 0 || omitted_symbols > 0 || omitted_imports > 0;

    ContextPack {
        schema: CONTEXT_SCHEMA,
        files: kept,
        total_files,
        total_symbols,
        total_imports,
        omitted_files,
        omitted_symbols,
        omitted_imports,
        truncated,
    }
}

fn compact_file(file: &FileContext, limits: ContextLimits) -> ContextFile {
    let mut symbols = file
        .symbols
        .iter()
        .map(|symbol| ContextSymbol {
            name: safe_label(&symbol.name, limits.max_label_bytes),
            kind: symbol.kind,
            public: symbol.public,
            span: symbol.span,
        })
        .collect::<Vec<_>>();
    symbols.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.kind.as_str().cmp(right.kind.as_str()))
            .then_with(|| left.public.cmp(&right.public))
            .then_with(|| left.span.start.cmp(&right.span.start))
            .then_with(|| left.span.end.cmp(&right.span.end))
    });

    let mut imports = file
        .imports
        .iter()
        .map(|import| ContextImport {
            module: safe_label(&import.module, limits.max_label_bytes),
            symbols: import
                .symbols
                .iter()
                .map(|symbol| safe_label(symbol, limits.max_label_bytes))
                .collect(),
            alias: import
                .alias
                .as_ref()
                .map(|alias| safe_label(alias, limits.max_label_bytes)),
            kind: import.kind,
        })
        .collect::<Vec<_>>();
    for import in &mut imports {
        import.symbols.sort();
    }
    imports.sort_by(|left, right| {
        left.module
            .cmp(&right.module)
            .then_with(|| left.kind.as_str().cmp(right.kind.as_str()))
            .then_with(|| left.alias.cmp(&right.alias))
            .then_with(|| left.symbols.cmp(&right.symbols))
    });

    ContextFile {
        path: normalize_context_path(&file.path, limits.max_path_bytes),
        total_symbols: symbols.len(),
        total_imports: imports.len(),
        symbols,
        imports,
    }
}

fn context_file_cmp(left: &ContextFile, right: &ContextFile) -> std::cmp::Ordering {
    left.path
        .cmp(&right.path)
        .then_with(|| left.total_symbols.cmp(&right.total_symbols))
        .then_with(|| left.total_imports.cmp(&right.total_imports))
        .then_with(|| compare_symbols(&left.symbols, &right.symbols))
        .then_with(|| left.imports.cmp(&right.imports))
}

fn compare_symbols(left: &[ContextSymbol], right: &[ContextSymbol]) -> std::cmp::Ordering {
    left.iter()
        .zip(right)
        .map(|(left, right)| {
            left.name
                .cmp(&right.name)
                .then_with(|| left.kind.as_str().cmp(right.kind.as_str()))
                .then_with(|| left.public.cmp(&right.public))
                .then_with(|| left.span.start.cmp(&right.span.start))
                .then_with(|| left.span.end.cmp(&right.span.end))
        })
        .find(|ordering| *ordering != std::cmp::Ordering::Equal)
        .unwrap_or_else(|| left.len().cmp(&right.len()))
}

fn normalize_context_path(path: &str, max_bytes: usize) -> String {
    let normalized = path.replace('\\', "/");
    if normalized.starts_with('/') {
        return cap_text("<external>", max_bytes);
    }
    let mut parts = Vec::new();
    for part in normalized.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." || (parts.is_empty() && part.ends_with(':')) {
            return cap_text("<external>", max_bytes);
        }
        parts.push(part);
    }
    cap_text(&parts.join("/"), max_bytes)
}

fn safe_label(value: &str, max_bytes: usize) -> String {
    if looks_like_credential(value) {
        cap_text("<redacted>", max_bytes)
    } else {
        cap_text(value, max_bytes)
    }
}

fn looks_like_credential(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    value.starts_with("sk-")
        || value.starts_with("ghp_")
        || value.starts_with("xoxb-")
        || lower.contains("begin private key")
        || lower.contains("api_key=")
        || lower.contains("password=")
        || lower.contains("secret=")
}

fn cap_text(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_owned();
    }
    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
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
    use super::{
        ContextLimits, FileContext, Import, ImportKind, Symbol, SymbolKind, build_context, imports,
        symbols,
    };
    use crate::diagnostic::Span;
    use crate::language::parser::parse;

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

    #[test]
    fn context_pack_is_empty_without_files() {
        let pack = build_context(&[], ContextLimits::default());

        assert_eq!(pack.schema, super::CONTEXT_SCHEMA);
        assert!(pack.files.is_empty());
        assert_eq!(pack.total_files, 0);
        assert_eq!(pack.total_symbols, 0);
        assert_eq!(pack.total_imports, 0);
        assert_eq!(pack.omitted_files, 0);
        assert!(!pack.truncated);
    }

    #[test]
    fn context_pack_normalizes_and_orders_metadata() {
        let files = vec![
            FileContext {
                path: "b\\second.sely".to_owned(),
                symbols: vec![
                    Symbol {
                        name: "zeta".to_owned(),
                        kind: SymbolKind::Function,
                        public: false,
                        span: Span::new(10, 12),
                    },
                    Symbol {
                        name: "alpha".to_owned(),
                        kind: SymbolKind::Binding,
                        public: false,
                        span: Span::new(1, 3),
                    },
                ],
                imports: vec![
                    Import {
                        module: "z.mod".to_owned(),
                        symbols: vec!["z".to_owned(), "a".to_owned()],
                        alias: None,
                        kind: ImportKind::Local,
                    },
                    Import {
                        module: "a.mod".to_owned(),
                        symbols: Vec::new(),
                        alias: None,
                        kind: ImportKind::Standard,
                    },
                ],
            },
            FileContext {
                path: ".\\a\\first.sely".to_owned(),
                symbols: Vec::new(),
                imports: Vec::new(),
            },
        ];

        let pack = build_context(&files, ContextLimits::default());

        assert_eq!(pack.files[0].path, "a/first.sely");
        assert_eq!(pack.files[1].path, "b/second.sely");
        assert_eq!(pack.files[1].symbols[0].name, "alpha");
        assert_eq!(pack.files[1].symbols[1].name, "zeta");
        assert_eq!(pack.files[1].imports[0].module, "a.mod");
        assert_eq!(pack.files[1].imports[1].module, "z.mod");
        assert_eq!(pack.files[1].imports[1].symbols, ["a", "z"]);
    }

    #[test]
    fn context_pack_reports_global_truncation() {
        let files = vec![
            FileContext {
                path: "a.sely".to_owned(),
                symbols: vec![
                    Symbol {
                        name: "a".to_owned(),
                        kind: SymbolKind::Binding,
                        public: false,
                        span: Span::new(0, 1),
                    },
                    Symbol {
                        name: "b".to_owned(),
                        kind: SymbolKind::Binding,
                        public: false,
                        span: Span::new(1, 2),
                    },
                ],
                imports: vec![
                    Import {
                        module: "a".to_owned(),
                        symbols: Vec::new(),
                        alias: None,
                        kind: ImportKind::Local,
                    },
                    Import {
                        module: "b".to_owned(),
                        symbols: Vec::new(),
                        alias: None,
                        kind: ImportKind::Local,
                    },
                ],
            },
            FileContext {
                path: "b.sely".to_owned(),
                symbols: Vec::new(),
                imports: Vec::new(),
            },
        ];

        let limits = ContextLimits::new(1, 1, 1, 64, 64);
        let pack = build_context(&files, limits);

        assert_eq!(pack.files.len(), 1);
        assert_eq!(pack.files[0].symbols.len(), 1);
        assert_eq!(pack.files[0].imports.len(), 1);
        assert_eq!(pack.omitted_files, 1);
        assert_eq!(pack.omitted_symbols, 1);
        assert_eq!(pack.omitted_imports, 1);
        assert!(pack.truncated);
    }

    #[test]
    fn context_pack_caps_labels_and_redacts_credential_shaped_metadata() {
        let files = [FileContext {
            path: "C:\\private\\main.sely".to_owned(),
            symbols: vec![Symbol {
                name: "sk-test-secret".to_owned(),
                kind: SymbolKind::Function,
                public: false,
                span: Span::new(0, 1),
            }],
            imports: vec![Import {
                module: "module_name_that_is_long".to_owned(),
                symbols: vec!["password=hidden".to_owned()],
                alias: None,
                kind: ImportKind::Local,
            }],
        }];

        let pack = build_context(&files, ContextLimits::new(4, 4, 4, 32, 8));

        assert_eq!(pack.files[0].path, "<external>");
        assert_eq!(pack.files[0].symbols[0].name, "<redacte");
        assert_eq!(pack.files[0].imports[0].module, "module_n");
        assert_eq!(pack.files[0].imports[0].symbols[0], "<redacte");
    }

    #[test]
    fn context_pack_never_reads_paths() {
        let files = [FileContext {
            path: "does-not-exist.sely".to_owned(),
            symbols: Vec::new(),
            imports: Vec::new(),
        }];

        let pack = build_context(&files, ContextLimits::default());

        assert_eq!(pack.files[0].path, "does-not-exist.sely");
    }
}
