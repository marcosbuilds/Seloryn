#![allow(clippy::result_large_err)]

use crate::diagnostic::{Diagnostic, Span};
use crate::parser::{Directive, IfBranch, MatchCase, MatchStatement, Pattern};
use crate::parser::{Expr, Item, Program, Statement};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

const BUNDLE_MAGIC: &[u8] = b"SELORYN-SELYAPP\0";
pub const BUNDLE_FORMAT: &str = "seloryn.bundle.v1";
const BUNDLE_FORMAT_VERSION: u32 = 1;
const MAX_BUNDLE_BYTES: usize = 64 * 1024 * 1024;
const MAX_BUNDLE_FILES: usize = 4096;

pub struct LoadedProgram {
    pub program: Program,
    pub source: String,
    pub label: String,
    pub base_dir: PathBuf,
    pub read_roots: Vec<PathBuf>,
    pub write_roots: Vec<PathBuf>,
    pub json_max_bytes: usize,
    pub json_max_depth: usize,
    pub training_json_max_bytes: usize,
    pub training_json_max_depth: usize,
    /// SHA-256 of the manifest bytes used to load this program.
    pub manifest_sha256: String,
    /// SHA-256 of the canonical manifest-plus-ordered-source identity.
    pub program_sha256: String,
}

#[derive(Debug)]
pub struct Bundle {
    pub format_version: u32,
    pub profile: String,
    pub files: Vec<BundleFile>,
}

#[derive(Debug)]
pub struct BundleFile {
    pub path: String,
    pub bytes: Vec<u8>,
}

#[allow(clippy::result_large_err)]
pub fn load(path: &Path) -> Result<LoadedProgram, Diagnostic> {
    if !path.is_dir() {
        return Err(Diagnostic::new(
            "SEL-C003",
            "project path is not a directory",
            Span::new(0, 0),
        ));
    }
    let project_root = fs::canonicalize(path).map_err(|_| {
        Diagnostic::new(
            "SEL-C003",
            "project path could not be canonicalized",
            Span::new(0, 0),
        )
    })?;
    let manifest = project_root.join("seloryn.toml");
    let manifest_text = fs::read_to_string(&manifest).map_err(|_| {
        Diagnostic::new(
            "SEL-M002",
            "project manifest `seloryn.toml` is missing or unreadable",
            Span::new(0, 0),
        )
    })?;
    let manifest_document = parse_manifest(&manifest_text)?;
    validate_language_version(&manifest_document)?;
    let source_root =
        value_for(&manifest_document, "source_root").unwrap_or_else(|| "src".to_owned());
    let source_root = manifest_relative_path(&source_root, "source_root")?;
    let entry = value_for(&manifest_document, "entry")
        .or_else(|| value_for(&manifest_document, "entrypoint"))
        .unwrap_or_else(|| "main.sely".to_owned());
    let entry = if Path::new(&entry).extension().is_none() {
        format!("{entry}.sely")
    } else {
        entry
    };
    let entry = manifest_relative_path(&entry, "entry")?;
    let source_root = project_root.join(source_root);
    let json_max_bytes = value_for(&manifest_document, "json_max_bytes")
        .and_then(|value| value.parse().ok())
        .unwrap_or(128);
    let json_max_depth = value_for(&manifest_document, "json_max_depth")
        .and_then(|value| value.parse().ok())
        .unwrap_or(4);
    let training_json_max_bytes = value_for(&manifest_document, "json_max_bytes")
        .and_then(|value| value.parse().ok())
        .unwrap_or(1_048_576);
    let training_json_max_depth = value_for(&manifest_document, "json_max_depth")
        .and_then(|value| value.parse().ok())
        .unwrap_or(32);
    let entry_path = source_root.join(entry);
    if !entry_path.starts_with(&source_root) || !entry_path.is_file() {
        return Err(Diagnostic::new(
            "SEL-M002",
            "project entry source is missing or escapes `source_root`",
            Span::new(0, 0),
        ));
    }

    let mut read_roots = declared_read_roots(&manifest_document)
        .into_iter()
        .map(|root| {
            let relative = manifest_relative_path(&root, "capabilities.read")?;
            let directory = project_root.join(relative);
            if !directory.is_dir() {
                return Err(Diagnostic::new(
                    "SEL-S004",
                    format!("declared read capability {root} is missing or not a directory"),
                    Span::new(0, 0),
                ));
            }
            fs::canonicalize(&directory).map_err(|_| {
                Diagnostic::new(
                    "SEL-S004",
                    format!("declared read capability {root} could not be canonicalized"),
                    Span::new(0, 0),
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    read_roots.sort();
    read_roots.dedup();

    let mut write_roots = declared_write_roots(&manifest_document)
        .into_iter()
        .map(|root| {
            let relative = manifest_relative_path(&root, "capabilities.write")?;
            let directory = project_root.join(relative);
            if !directory.is_dir() {
                return Err(Diagnostic::new(
                    "SEL-S004",
                    format!("declared write capability {root} is missing or not a directory"),
                    Span::new(0, 0),
                ));
            }
            fs::canonicalize(&directory).map_err(|_| {
                Diagnostic::new(
                    "SEL-S004",
                    format!("declared write capability {root} could not be canonicalized"),
                    Span::new(0, 0),
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    write_roots.sort();
    write_roots.dedup();

    let mut loader = Loader {
        source_root,
        visiting: HashSet::new(),
        visited: HashMap::new(),
        ordered: Vec::new(),
        aliases: HashMap::new(),
    };
    loader.visit(&entry_path, true)?;

    let entry_source = loader
        .visited
        .get(&entry_path)
        .map(|unit| unit.source.clone())
        .unwrap_or_default();
    let manifest_sha256 = sha256_hex(manifest_text.as_bytes());
    let mut program_hasher = Sha256::new();
    program_hasher.update(b"seloryn-program-identity-v1\0");
    program_hasher.update(manifest_text.as_bytes());
    for source_path in &loader.ordered {
        let relative = source_path
            .strip_prefix(&project_root)
            .unwrap_or(source_path)
            .to_string_lossy()
            .replace('\\', "/");
        program_hasher.update(relative.as_bytes());
        program_hasher.update([0]);
        if let Some(unit) = loader.visited.get(source_path) {
            program_hasher.update(unit.source.as_bytes());
        }
        program_hasher.update([0]);
    }
    let program_sha256 = format!("{:x}", program_hasher.finalize());
    let mut items = Vec::new();
    for path in &loader.ordered {
        let Some(unit) = loader.visited.get(path) else {
            continue;
        };
        for item in &unit.program.items {
            if is_local_directive(item, &loader.source_root) {
                continue;
            }
            let item = if path == &entry_path {
                rewrite_item(item.clone(), &loader.aliases)
            } else {
                item.clone()
            };
            items.push(item);
        }
    }
    Ok(LoadedProgram {
        program: Program {
            items,
            span: Span::new(0, entry_source.len()),
        },
        source: entry_source,
        label: format!("project {}", path.display()),
        base_dir: project_root,
        read_roots,
        write_roots,
        json_max_bytes,
        json_max_depth,
        training_json_max_bytes,
        training_json_max_depth,
        manifest_sha256,
        program_sha256,
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn encode_bundle(path: &Path, profile: &str) -> Result<Vec<u8>, Diagnostic> {
    if !matches!(profile, "debug" | "release") {
        return Err(Diagnostic::new(
            "SEL-B001",
            "build profile must be `debug` or `release`",
            Span::new(0, 0),
        ));
    }
    let _ = load(path)?;
    let manifest = path.join("seloryn.toml");
    let manifest_text = fs::read_to_string(&manifest).map_err(|_| {
        Diagnostic::new(
            "SEL-B001",
            "project manifest `seloryn.toml` is missing or unreadable",
            Span::new(0, 0),
        )
    })?;
    let manifest_document = parse_manifest(&manifest_text)
        .map_err(|diagnostic| Diagnostic::new("SEL-B001", diagnostic.message, diagnostic.span))?;
    validate_language_version(&manifest_document)
        .map_err(|diagnostic| Diagnostic::new("SEL-B001", diagnostic.message, diagnostic.span))?;
    let source_root =
        path.join(value_for(&manifest_document, "source_root").unwrap_or_else(|| "src".to_owned()));
    let mut files = vec![BundleFile {
        path: "seloryn.toml".to_owned(),
        bytes: fs::read(&manifest).map_err(|_| {
            Diagnostic::new(
                "SEL-B001",
                "project manifest could not be read",
                Span::new(0, 0),
            )
        })?,
    }];
    collect_bundle_files(path, &source_root, &mut files)?;
    for read_root in declared_read_roots(&manifest_document) {
        validate_bundle_path(&read_root.replace('\\', "/"))?;
        let directory = path.join(&read_root);
        if !directory.starts_with(path) || !directory.is_dir() {
            return Err(bundle_error(format!(
                "declared read capability `{read_root}` is missing or not a directory"
            )));
        }
        collect_bundle_files(path, &directory, &mut files)?;
    }
    for write_root in declared_write_roots(&manifest_document) {
        validate_bundle_path(&write_root.replace('\\', "/"))?;
        let directory = path.join(&write_root);
        if !directory.starts_with(path) || !directory.is_dir() {
            return Err(bundle_error(format!(
                "declared write capability `{write_root}` is missing or not a directory"
            )));
        }
        collect_bundle_files(path, &directory, &mut files)?;
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    let bundle = Bundle {
        format_version: BUNDLE_FORMAT_VERSION,
        profile: profile.to_owned(),
        files,
    };
    encode_bundle_data(&bundle)
}

pub fn decode_bundle(bytes: &[u8]) -> Result<Bundle, Diagnostic> {
    if bytes.len() > MAX_BUNDLE_BYTES {
        return Err(bundle_error("bundle exceeds the 64 MiB size limit"));
    }
    let mut reader = BundleReader { bytes, cursor: 0 };
    if reader.take(BUNDLE_MAGIC.len()) != Some(BUNDLE_MAGIC) {
        return Err(bundle_error("invalid `.selyapp` bundle header"));
    }
    let format_version = reader.u32()?;
    if format_version != BUNDLE_FORMAT_VERSION {
        return Err(bundle_error(format!(
            "unsupported `.selyapp` format version {format_version}"
        )));
    }
    let profile_length = reader.u32()?;
    let profile = reader.text(profile_length, "bundle profile")?;
    if !matches!(profile.as_str(), "debug" | "release") {
        return Err(bundle_error("bundle has an unsupported build profile"));
    }
    let count = reader.u32()? as usize;
    if count == 0 || count > MAX_BUNDLE_FILES {
        return Err(bundle_error(
            "bundle file count is outside the supported limit",
        ));
    }
    let mut files = Vec::with_capacity(count);
    let mut seen = HashSet::new();
    for _ in 0..count {
        let path_length = reader.u32()?;
        let path = reader.text(path_length, "bundle path")?;
        validate_bundle_path(&path)?;
        if !seen.insert(path.clone()) {
            return Err(bundle_error("bundle contains a duplicate file path"));
        }
        let length = usize::try_from(reader.u64()?)
            .map_err(|_| bundle_error("bundle file length does not fit this host"))?;
        if length > MAX_BUNDLE_BYTES || length > reader.remaining() {
            return Err(bundle_error(
                "bundle file length is outside the supported limit",
            ));
        }
        let bytes = reader
            .take(length)
            .ok_or_else(|| bundle_error("bundle ends before a file is complete"))?
            .to_vec();
        files.push(BundleFile { path, bytes });
    }
    if reader.remaining() != 0 {
        return Err(bundle_error("bundle has trailing bytes"));
    }
    if !files.iter().any(|file| file.path == "seloryn.toml") {
        return Err(bundle_error("bundle has no `seloryn.toml` manifest"));
    }
    Ok(Bundle {
        format_version,
        profile,
        files,
    })
}

fn encode_bundle_data(bundle: &Bundle) -> Result<Vec<u8>, Diagnostic> {
    if bundle.files.is_empty() || bundle.files.len() > MAX_BUNDLE_FILES {
        return Err(bundle_error(
            "project has an unsupported number of bundle files",
        ));
    }
    let mut output = Vec::new();
    output.extend_from_slice(BUNDLE_MAGIC);
    if bundle.format_version != BUNDLE_FORMAT_VERSION {
        return Err(bundle_error(format!(
            "unsupported `.selyapp` format version {}",
            bundle.format_version
        )));
    }
    output.extend_from_slice(&bundle.format_version.to_le_bytes());
    put_text(&mut output, &bundle.profile)?;
    output.extend_from_slice(&(bundle.files.len() as u32).to_le_bytes());
    for file in &bundle.files {
        validate_bundle_path(&file.path)?;
        put_text(&mut output, &file.path)?;
        output.extend_from_slice(&(file.bytes.len() as u64).to_le_bytes());
        output.extend_from_slice(&file.bytes);
    }
    if output.len() > MAX_BUNDLE_BYTES {
        return Err(bundle_error("project bundle exceeds the 64 MiB size limit"));
    }
    Ok(output)
}

fn collect_bundle_files(
    project_root: &Path,
    directory: &Path,
    files: &mut Vec<BundleFile>,
) -> Result<(), Diagnostic> {
    let entries = fs::read_dir(directory).map_err(|_| {
        Diagnostic::new(
            "SEL-B001",
            "project source root is missing or unreadable",
            Span::new(0, 0),
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|_| bundle_error("project source entry could not be read"))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|_| bundle_error("project source entry type could not be read"))?;
        if file_type.is_symlink() {
            return Err(bundle_error(
                "symbolic links are not allowed in a project bundle",
            ));
        }
        if file_type.is_dir() {
            collect_bundle_files(project_root, &path, files)?;
            continue;
        }
        if !file_type.is_file() {
            return Err(bundle_error(
                "project bundle contains a non-file source entry",
            ));
        }
        let relative = path
            .strip_prefix(project_root)
            .map_err(|_| bundle_error("project source escapes the project root"))?;
        let relative = relative
            .to_str()
            .ok_or_else(|| bundle_error("project source path is not valid UTF-8"))?
            .replace('\\', "/");
        validate_bundle_path(&relative)?;
        if files.iter().any(|file| file.path == relative) {
            return Err(bundle_error(format!(
                "project bundle contains duplicate path `{relative}`"
            )));
        }
        files.push(BundleFile {
            path: relative,
            bytes: fs::read(path).map_err(|_| bundle_error("project source could not be read"))?,
        });
        if files.len() > MAX_BUNDLE_FILES {
            return Err(bundle_error("project has too many source files"));
        }
    }
    Ok(())
}

fn put_text(output: &mut Vec<u8>, value: &str) -> Result<(), Diagnostic> {
    let bytes = value.as_bytes();
    let length =
        u32::try_from(bytes.len()).map_err(|_| bundle_error("bundle text field is too long"))?;
    output.extend_from_slice(&length.to_le_bytes());
    output.extend_from_slice(bytes);
    Ok(())
}

fn validate_bundle_path(path: &str) -> Result<(), Diagnostic> {
    let candidate = Path::new(path);
    if path.is_empty()
        || candidate.is_absolute()
        || candidate
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(bundle_error(
            "bundle file path must be relative and contained",
        ));
    }
    Ok(())
}

fn bundle_error(message: impl Into<String>) -> Diagnostic {
    Diagnostic::new("SEL-B001", message, Span::new(0, 0))
}

struct BundleReader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> BundleReader<'a> {
    fn take(&mut self, length: usize) -> Option<&'a [u8]> {
        let end = self.cursor.checked_add(length)?;
        let value = self.bytes.get(self.cursor..end)?;
        self.cursor = end;
        Some(value)
    }

    fn u32(&mut self) -> Result<u32, Diagnostic> {
        let bytes = self
            .take(4)
            .ok_or_else(|| bundle_error("bundle ends before a length field"))?;
        Ok(u32::from_le_bytes(
            bytes.try_into().expect("length is four bytes"),
        ))
    }

    fn u64(&mut self) -> Result<u64, Diagnostic> {
        let bytes = self
            .take(8)
            .ok_or_else(|| bundle_error("bundle ends before a file length"))?;
        Ok(u64::from_le_bytes(
            bytes.try_into().expect("length is eight bytes"),
        ))
    }

    fn text(&mut self, length: u32, field: &str) -> Result<String, Diagnostic> {
        let bytes = self
            .take(length as usize)
            .ok_or_else(|| bundle_error(format!("bundle ends before {field}")))?;
        String::from_utf8(bytes.to_vec())
            .map_err(|_| bundle_error(format!("bundle {field} is not UTF-8")))
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.cursor)
    }
}

struct Unit {
    program: Program,
    source: String,
}

struct Loader {
    source_root: PathBuf,
    visiting: HashSet<PathBuf>,
    visited: HashMap<PathBuf, Unit>,
    ordered: Vec<PathBuf>,
    aliases: HashMap<String, String>,
}

impl Loader {
    #[allow(clippy::result_large_err)]
    fn visit(&mut self, path: &Path, is_entry: bool) -> Result<(), Diagnostic> {
        let path = normalize_path(path);
        if self.visiting.contains(&path) {
            return Err(Diagnostic::new(
                "SEL-M005",
                format!("cyclic module initialization at `{}`", path.display()),
                Span::new(0, 0),
            ));
        }
        if self.visited.contains_key(&path) {
            return Ok(());
        }
        self.visiting.insert(path.clone());
        let bytes = fs::read(&path).map_err(|_| {
            Diagnostic::new(
                "SEL-M002",
                format!("module source is missing: `{}`", path.display()),
                Span::new(0, 0),
            )
        })?;
        let source = String::from_utf8(bytes).map_err(|_| {
            Diagnostic::new(
                "SEL-M002",
                format!("module source is not valid UTF-8: `{}`", path.display()),
                Span::new(0, 0),
            )
        })?;
        let parsed = crate::parser::parse(&source);
        if let Some(diagnostic) = parsed.diagnostics.first() {
            return Err(diagnostic.clone());
        }
        let Some(program) = parsed.program else {
            return Err(Diagnostic::new(
                "SEL-C004",
                "parser returned no project program",
                Span::new(0, 0),
            ));
        };
        let imports = imports(&program);
        self.visited.insert(path.clone(), Unit { program, source });
        for import in imports {
            match import {
                Import::Local {
                    module,
                    symbol,
                    alias,
                } => {
                    let module_path = self.resolve_module(&module, symbol.is_some())?;
                    self.visit(&module_path, false)?;
                    if let Some(symbol) = symbol
                        && !self.is_exported(&module_path, &symbol)
                    {
                        return Err(Diagnostic::new(
                            "SEL-M004",
                            format!("`{symbol}` is not exported by module `{module}`"),
                            Span::new(0, 0),
                        ));
                    }
                    if is_entry && let Some(alias) = alias {
                        self.aliases.insert(alias, module);
                    }
                }
                Import::Standard => {}
            }
        }
        self.visiting.remove(&path);
        self.ordered.push(path);
        Ok(())
    }

    #[allow(clippy::result_large_err)]
    fn resolve_module(&self, module: &str, _from_import: bool) -> Result<PathBuf, Diagnostic> {
        if module
            .split('.')
            .any(|part| part.is_empty() || part == ".." || part == ".")
            || module.contains('/')
        {
            return Err(Diagnostic::new(
                "SEL-M006",
                format!("module path `{module}` escapes the project root"),
                Span::new(0, 0),
            ));
        }
        if module.starts_with("seloryn") {
            return Err(Diagnostic::new(
                "SEL-M002",
                format!("branded standard-library namespace `{module}` is unavailable"),
                Span::new(0, 0),
            ));
        }
        let mut candidate = self.source_root.clone();
        for part in module.split('.') {
            candidate.push(part);
        }
        candidate.set_extension("sely");
        let normalized = normalize_path(&candidate);
        if !normalized.starts_with(&self.source_root) {
            return Err(Diagnostic::new(
                "SEL-M006",
                format!("module path `{module}` escapes `source_root`"),
                Span::new(0, 0),
            ));
        }
        if !normalized.is_file() {
            return Err(Diagnostic::new(
                "SEL-M002",
                format!("module `{module}` was not found"),
                Span::new(0, 0),
            ));
        }
        Ok(normalized)
    }

    fn is_exported(&self, path: &Path, symbol: &str) -> bool {
        self.visited.get(path).is_some_and(|unit| {
            unit.program.items.iter().any(|item| match item {
                Item::Function(function) => function.public && function.name == symbol,
                Item::Record(record) => record.public && record.name == symbol,
                Item::Enum(enum_decl) => enum_decl.public && enum_decl.name == symbol,
                _ => false,
            })
        })
    }
}

enum Import {
    Local {
        module: String,
        symbol: Option<String>,
        alias: Option<String>,
    },
    Standard,
}

fn imports(program: &Program) -> Vec<Import> {
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
                return Some(if is_standard_module(&module) {
                    Import::Standard
                } else {
                    Import::Local {
                        module,
                        symbol: None,
                        alias,
                    }
                });
            }
            if keyword == "from" {
                let mut parts = text.split_whitespace();
                let module = parts.next()?.to_owned();
                let _import = parts.next()?;
                let symbol = parts.next()?.trim_end_matches(',').to_owned();
                return Some(if is_standard_module(&module) {
                    Import::Standard
                } else {
                    Import::Local {
                        module,
                        symbol: Some(symbol),
                        alias: None,
                    }
                });
            }
            None
        })
        .collect()
}

fn is_standard_module(module: &str) -> bool {
    matches!(
        module,
        "io" | "json" | "models" | "autograd" | "training" | "data" | "tensor" | "audit"
    )
}

fn is_local_directive(item: &Item, _source_root: &Path) -> bool {
    let Item::Directive(Directive { keyword, text, .. }) = item else {
        return false;
    };
    if keyword == "module" {
        return true;
    }
    imports_from_directive(keyword, text)
}

fn imports_from_directive(keyword: &str, text: &str) -> bool {
    let module = text.split_whitespace().next().unwrap_or_default();
    (keyword == "import" || keyword == "from") && !is_standard_module(module)
}

fn parse_manifest(manifest: &str) -> Result<toml::Value, Diagnostic> {
    let document = toml::from_str(manifest).map_err(|error| {
        Diagnostic::new(
            "SEL-M001",
            format!("invalid project manifest `seloryn.toml`: {error}"),
            Span::new(0, 0),
        )
    })?;
    validate_manifest_schema(&document)?;
    Ok(document)
}

fn invalid_manifest(message: impl Into<String>) -> Diagnostic {
    Diagnostic::new("SEL-M001", message, Span::new(0, 0))
}

fn require_manifest_string(
    table: &toml::map::Map<String, toml::Value>,
    key: &str,
    required: bool,
) -> Result<(), Diagnostic> {
    let Some(value) = table.get(key) else {
        if required {
            return Err(invalid_manifest(format!(
                "project manifest field `[package] {key}` must be a string"
            )));
        }
        return Ok(());
    };
    if !value.is_str() {
        return Err(invalid_manifest(format!(
            "project manifest field `[package] {key}` must be a string"
        )));
    }
    Ok(())
}

fn validate_manifest_schema(manifest: &toml::Value) -> Result<(), Diagnostic> {
    let Some(root) = manifest.as_table() else {
        return Err(invalid_manifest(
            "project manifest must contain a TOML table",
        ));
    };
    for key in root.keys() {
        if !matches!(key.as_str(), "package" | "capabilities" | "limits") {
            return Err(invalid_manifest(format!(
                "unknown project manifest section or field `{key}`"
            )));
        }
    }
    let Some(package) = root.get("package").and_then(toml::Value::as_table) else {
        return Err(invalid_manifest(
            "project manifest must contain a `[package]` table",
        ));
    };
    for key in package.keys() {
        if !matches!(
            key.as_str(),
            "name" | "language" | "source_root" | "entry" | "entrypoint"
        ) {
            return Err(invalid_manifest(format!(
                "unknown project manifest field `[package] {key}`"
            )));
        }
    }
    require_manifest_string(package, "name", true)?;
    require_manifest_string(package, "language", true)?;
    require_manifest_string(package, "source_root", false)?;
    require_manifest_string(package, "entry", false)?;
    require_manifest_string(package, "entrypoint", false)?;
    if package.contains_key("entry") && package.contains_key("entrypoint") {
        return Err(invalid_manifest(
            "project manifest cannot declare both `[package] entry` and `entrypoint`",
        ));
    }

    if let Some(capabilities) = root.get("capabilities") {
        let Some(capabilities) = capabilities.as_table() else {
            return Err(invalid_manifest(
                "project manifest `[capabilities]` must be a table",
            ));
        };
        for key in capabilities.keys() {
            if !matches!(key.as_str(), "read" | "write") {
                return Err(invalid_manifest(format!(
                    "unknown project manifest field `[capabilities] {key}`"
                )));
            }
            let Some(values) = capabilities.get(key).and_then(toml::Value::as_array) else {
                return Err(invalid_manifest(format!(
                    "project manifest field `[capabilities] {key}` must be an array of strings"
                )));
            };
            if values.iter().any(|value| !value.is_str()) {
                return Err(invalid_manifest(format!(
                    "project manifest field `[capabilities] {key}` must be an array of strings"
                )));
            }
        }
    }

    if let Some(limits) = root.get("limits") {
        let Some(limits) = limits.as_table() else {
            return Err(invalid_manifest(
                "project manifest `[limits]` must be a table",
            ));
        };
        for key in limits.keys() {
            if !matches!(key.as_str(), "json_max_bytes" | "json_max_depth") {
                return Err(invalid_manifest(format!(
                    "unknown project manifest field `[limits] {key}`"
                )));
            }
            let Some(value) = limits.get(key).and_then(toml::Value::as_integer) else {
                return Err(invalid_manifest(format!(
                    "project manifest field `[limits] {key}` must be a positive integer"
                )));
            };
            if value <= 0 {
                return Err(invalid_manifest(format!(
                    "project manifest field `[limits] {key}` must be a positive integer"
                )));
            }
        }
    }
    Ok(())
}

fn validate_language_version(manifest: &toml::Value) -> Result<(), Diagnostic> {
    let language = manifest
        .get("package")
        .and_then(|package| package.get("language"))
        .and_then(toml::Value::as_str)
        .ok_or_else(|| {
            invalid_manifest("project manifest `[package]` must declare `language = \"M.mm\"`")
        })?;
    let Some((major, minor)) = language.split_once('.') else {
        return Err(Diagnostic::new(
            "SEL-M001",
            format!("project language version `{language}` is not canonical M.mm"),
            Span::new(0, 0),
        ));
    };
    if major.is_empty()
        || (major != "0" && major.starts_with('0'))
        || !major.chars().all(|character| character.is_ascii_digit())
        || minor.len() != 2
        || !minor.chars().all(|character| character.is_ascii_digit())
    {
        return Err(Diagnostic::new(
            "SEL-M001",
            format!("project language version `{language}` is not canonical M.mm"),
            Span::new(0, 0),
        ));
    }
    let requested = (
        major.parse::<u64>().unwrap_or(u64::MAX),
        minor.parse::<u64>().unwrap_or(u64::MAX),
    );
    let Some((supported_major, supported_minor)) = crate::LANGUAGE_VERSION.split_once('.') else {
        return Err(Diagnostic::new(
            "SEL-M001",
            "runtime language version is invalid",
            Span::new(0, 0),
        ));
    };
    let supported = (
        supported_major.parse::<u64>().unwrap_or(u64::MAX),
        supported_minor.parse::<u64>().unwrap_or(u64::MAX),
    );
    if requested > supported {
        return Err(Diagnostic::new(
            "SEL-M001",
            format!(
                "project language version `{language}` requires a newer Seloryn runtime (supported: {})",
                crate::LANGUAGE_VERSION
            ),
            Span::new(0, 0),
        ));
    }
    Ok(())
}

fn value_for(manifest: &toml::Value, key: &str) -> Option<String> {
    ["package", "limits"]
        .into_iter()
        .filter_map(|table| manifest.get(table))
        .chain(std::iter::once(manifest))
        .find_map(|table| table.get(key))
        .and_then(|value| match value {
            toml::Value::String(value) => Some(value.clone()),
            toml::Value::Integer(value) => Some(value.to_string()),
            _ => None,
        })
}

fn manifest_relative_path(value: &str, field: &str) -> Result<PathBuf, Diagnostic> {
    let path = Path::new(value);
    if value.is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(Diagnostic::new(
            "SEL-S004",
            format!("manifest field {field} must be a relative contained path"),
            Span::new(0, 0),
        ));
    }
    Ok(path.to_path_buf())
}

fn declared_read_roots(manifest: &toml::Value) -> Vec<String> {
    declared_capability_roots(manifest, "read")
}

fn declared_write_roots(manifest: &toml::Value) -> Vec<String> {
    declared_capability_roots(manifest, "write")
}

fn declared_capability_roots(manifest: &toml::Value, capability: &str) -> Vec<String> {
    manifest
        .get("capabilities")
        .and_then(|capabilities| capabilities.get(capability))
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_str)
        .map(str::to_owned)
        .collect()
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            component => result.push(component.as_os_str()),
        }
    }
    result
}

fn rewrite_item(item: Item, aliases: &HashMap<String, String>) -> Item {
    match item {
        Item::Function(mut function) => {
            function.body = rewrite_statements(function.body, aliases);
            Item::Function(function)
        }
        Item::Binding(mut binding) => {
            binding.value = rewrite_expr(binding.value, aliases);
            Item::Binding(binding)
        }
        Item::Statement(statement) => Item::Statement(rewrite_statement(statement, aliases)),
        other => other,
    }
}

fn rewrite_statements(
    statements: Vec<Statement>,
    aliases: &HashMap<String, String>,
) -> Vec<Statement> {
    statements
        .into_iter()
        .map(|statement| rewrite_statement(statement, aliases))
        .collect()
}

fn rewrite_statement(statement: Statement, aliases: &HashMap<String, String>) -> Statement {
    match statement {
        Statement::Binding(mut binding) => {
            binding.value = rewrite_expr(binding.value, aliases);
            Statement::Binding(binding)
        }
        Statement::Assignment { name, value, span } => Statement::Assignment {
            name,
            value: rewrite_expr(value, aliases),
            span,
        },
        Statement::Return { value, span } => Statement::Return {
            value: value.map(|value| rewrite_expr(value, aliases)),
            span,
        },
        Statement::Require {
            condition,
            failure,
            span,
        } => Statement::Require {
            condition: rewrite_expr(condition, aliases),
            failure: rewrite_expr(failure, aliases),
            span,
        },
        Statement::If {
            branches,
            otherwise,
            span,
        } => Statement::If {
            branches: branches
                .into_iter()
                .map(|branch| IfBranch {
                    condition: rewrite_expr(branch.condition, aliases),
                    body: rewrite_statements(branch.body, aliases),
                    span: branch.span,
                })
                .collect(),
            otherwise: otherwise.map(|body| rewrite_statements(body, aliases)),
            span,
        },
        Statement::Match(statement) => Statement::Match(rewrite_match(statement, aliases)),
        Statement::For {
            name,
            iterable,
            body,
            span,
        } => Statement::For {
            name,
            iterable: rewrite_expr(iterable, aliases),
            body: rewrite_statements(body, aliases),
            span,
        },
        Statement::Expression(expression) => {
            Statement::Expression(rewrite_expr(expression, aliases))
        }
    }
}

fn rewrite_match(
    mut statement: MatchStatement,
    aliases: &HashMap<String, String>,
) -> MatchStatement {
    statement.value = rewrite_expr(statement.value, aliases);
    statement.cases = statement
        .cases
        .into_iter()
        .map(|case| MatchCase {
            pattern: rewrite_pattern(case.pattern),
            guard: case.guard.map(|guard| rewrite_expr(guard, aliases)),
            body: rewrite_statements(case.body, aliases),
            span: case.span,
        })
        .collect();
    statement
}

fn rewrite_pattern(pattern: Pattern) -> Pattern {
    pattern
}

fn rewrite_expr(expression: Expr, aliases: &HashMap<String, String>) -> Expr {
    match expression {
        Expr::Member {
            object,
            name,
            optional,
            span,
        } => {
            if !optional
                && let Expr::Name { name: alias, .. } = object.as_ref()
                && aliases.contains_key(alias)
            {
                return Expr::Name { name, span };
            }
            Expr::Member {
                object: Box::new(rewrite_expr(*object, aliases)),
                name,
                optional,
                span,
            }
        }
        Expr::Group { value, span } => Expr::Group {
            value: Box::new(rewrite_expr(*value, aliases)),
            span,
        },
        Expr::Unary {
            operator,
            value,
            span,
        } => Expr::Unary {
            operator,
            value: Box::new(rewrite_expr(*value, aliases)),
            span,
        },
        Expr::Binary {
            left,
            operator,
            right,
            span,
        } => Expr::Binary {
            left: Box::new(rewrite_expr(*left, aliases)),
            operator,
            right: Box::new(rewrite_expr(*right, aliases)),
            span,
        },
        Expr::Call {
            callee,
            arguments,
            span,
        } => Expr::Call {
            callee: Box::new(rewrite_expr(*callee, aliases)),
            arguments: arguments
                .into_iter()
                .map(|argument| rewrite_expr(argument, aliases))
                .collect(),
            span,
        },
        Expr::NamedArgument { name, value, span } => Expr::NamedArgument {
            name,
            value: Box::new(rewrite_expr(*value, aliases)),
            span,
        },
        Expr::Index {
            object,
            index,
            span,
        } => Expr::Index {
            object: Box::new(rewrite_expr(*object, aliases)),
            index: Box::new(rewrite_expr(*index, aliases)),
            span,
        },
        Expr::Propagate { value, span } => Expr::Propagate {
            value: Box::new(rewrite_expr(*value, aliases)),
            span,
        },
        Expr::List { values, span } => Expr::List {
            values: values
                .into_iter()
                .map(|value| rewrite_expr(value, aliases))
                .collect(),
            span,
        },
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::{Bundle, BundleFile, decode_bundle, encode_bundle_data};

    #[test]
    fn bundle_round_trip_is_deterministic_and_relative() {
        let bundle = Bundle {
            format_version: 1,
            profile: "debug".to_owned(),
            files: vec![
                BundleFile {
                    path: "seloryn.toml".to_owned(),
                    bytes: b"source_root = \"src\"\n".to_vec(),
                },
                BundleFile {
                    path: "src/main.sely".to_owned(),
                    bytes: b"fn main() -> Unit:\n    return\n".to_vec(),
                },
            ],
        };
        let first = encode_bundle_data(&bundle).expect("bundle should encode");
        let second = encode_bundle_data(&bundle).expect("bundle should encode again");
        assert_eq!(first, second);
        let decoded = decode_bundle(&first).expect("bundle should decode");
        assert_eq!(decoded.profile, "debug");
        assert_eq!(decoded.format_version, 1);
        assert_eq!(decoded.files[1].path, "src/main.sely");
    }

    #[test]
    fn bundle_decoder_rejects_path_escape_and_trailing_data() {
        let bundle = Bundle {
            format_version: 1,
            profile: "debug".to_owned(),
            files: vec![BundleFile {
                path: "../outside.sely".to_owned(),
                bytes: Vec::new(),
            }],
        };
        assert_eq!(encode_bundle_data(&bundle).unwrap_err().code, "SEL-B001");

        let valid = Bundle {
            format_version: 1,
            profile: "debug".to_owned(),
            files: vec![BundleFile {
                path: "seloryn.toml".to_owned(),
                bytes: Vec::new(),
            }],
        };
        let mut bytes = encode_bundle_data(&valid).expect("bundle should encode");
        bytes.push(0);
        assert_eq!(decode_bundle(&bytes).unwrap_err().code, "SEL-B001");
    }

    #[test]
    fn bundle_decoder_rejects_unknown_format_version() {
        let bundle = Bundle {
            format_version: 1,
            profile: "debug".to_owned(),
            files: vec![BundleFile {
                path: "seloryn.toml".to_owned(),
                bytes: Vec::new(),
            }],
        };
        let mut bytes = encode_bundle_data(&bundle).expect("bundle should encode");
        let version_offset = super::BUNDLE_MAGIC.len();
        bytes[version_offset..version_offset + 4].copy_from_slice(&99_u32.to_le_bytes());
        let error = decode_bundle(&bytes).expect_err("unknown bundle version must fail");
        assert!(
            error
                .message
                .contains("unsupported `.selyapp` format version 99")
        );
    }
}
