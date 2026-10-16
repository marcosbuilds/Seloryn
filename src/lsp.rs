//! Bounded Language Server Protocol support backed by the Seloryn parser.

use crate::diagnostic::Diagnostic;
use crate::json::{self, JsonValue};
use crate::parser::{Item, Program};
use std::collections::BTreeMap;
use std::io::{self, BufRead, BufReader, Write};

const MAX_MESSAGE_BYTES: usize = 4 * 1024 * 1024;

pub fn serve_stdio() -> u8 {
    match serve() {
        Ok(code) => code,
        Err(error) => {
            let _ = writeln!(io::stderr(), "SEL-LSP-001 {error}");
            74
        }
    }
}

fn serve() -> io::Result<u8> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = stdout.lock();
    let mut documents = BTreeMap::<String, String>::new();
    loop {
        let Some(message) = read_message(&mut reader)? else {
            return Ok(0);
        };
        let Some(object) = message.as_object() else {
            continue;
        };
        let method = object.get("method").and_then(|value| value.as_str());
        let id = object.get("id").cloned();
        match method {
            Some("initialize") => {
                if let Some(id) = id {
                    write_response(&mut writer, id, initialize_result())?;
                }
            }
            Some("initialized") | Some("$/cancelRequest") => {}
            Some("shutdown") => {
                if let Some(id) = id {
                    write_response(&mut writer, id, JsonValue::Null)?;
                }
            }
            Some("exit") => return Ok(0),
            Some("textDocument/didOpen") => {
                if let Some((uri, text)) = document_text(object.get("params")) {
                    documents.insert(uri.clone(), text.clone());
                    publish_diagnostics(&mut writer, &uri, &text)?;
                }
            }
            Some("textDocument/didChange") => {
                if let Some(params) = object.get("params")
                    && let (Some(uri), Some(changes)) = (
                        nested_string(Some(params), &["textDocument", "uri"]),
                        params
                            .get("contentChanges")
                            .and_then(|value| value.as_array()),
                    )
                    && let Some(text) = changes
                        .last()
                        .and_then(|change| change.get("text"))
                        .and_then(|value| value.as_str())
                {
                    documents.insert(uri.to_owned(), text.to_owned());
                    publish_diagnostics(&mut writer, uri, text)?;
                }
            }
            Some("textDocument/didSave") => {
                if let Some(uri) = nested_string(object.get("params"), &["textDocument", "uri"])
                    && let Some(text) = documents.get(uri)
                {
                    publish_diagnostics(&mut writer, uri, text)?;
                }
            }
            Some("textDocument/completion") => {
                if let Some(id) = id {
                    let uri = nested_string(object.get("params"), &["textDocument", "uri"]);
                    let text = uri.and_then(|value| documents.get(value));
                    write_response(
                        &mut writer,
                        id,
                        completion_result(text.map(String::as_str).unwrap_or_default()),
                    )?;
                }
            }
            Some("textDocument/definition") => {
                if let Some(id) = id {
                    let uri = nested_string(object.get("params"), &["textDocument", "uri"]);
                    let text = uri.and_then(|value| documents.get(value));
                    let position = object
                        .get("params")
                        .and_then(|params| params.get("position"));
                    let result = text.zip(position).and_then(|(source, point)| {
                        uri.and_then(|value| definition_result(value, source, point))
                    });
                    write_response(&mut writer, id, result.unwrap_or(JsonValue::Null))?;
                }
            }
            Some("textDocument/documentSymbol") => {
                if let Some(id) = id {
                    let uri = nested_string(object.get("params"), &["textDocument", "uri"]);
                    let text = uri.and_then(|value| documents.get(value));
                    write_response(
                        &mut writer,
                        id,
                        document_symbols(text.map(String::as_str).unwrap_or_default()),
                    )?;
                }
            }
            Some(_) => {
                if let Some(id) = id {
                    write_error(&mut writer, id, -32601, "method not supported")?;
                }
            }
            None => {}
        }
    }
}

fn read_message<R: BufRead>(reader: &mut R) -> io::Result<Option<JsonValue>> {
    let mut content_length = None;
    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            return Ok(None);
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length:") {
            content_length = Some(value.trim().parse::<usize>().map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "invalid Content-Length")
            })?);
        }
    }
    let length = content_length
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing Content-Length"))?;
    if length > MAX_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "LSP message exceeds the byte limit",
        ));
    }
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes)?;
    let source = String::from_utf8(bytes)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "LSP message is not UTF-8"))?;
    json::parse(&source, MAX_MESSAGE_BYTES, 64)
        .map(Some)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.message))
}

fn write_message<W: Write>(writer: &mut W, value: JsonValue) -> io::Result<()> {
    let payload = json::encode(&value);
    write!(
        writer,
        "Content-Length: {}\r\n\r\n{}",
        payload.len(),
        payload
    )?;
    writer.flush()
}

fn write_response<W: Write>(writer: &mut W, id: JsonValue, result: JsonValue) -> io::Result<()> {
    write_message(
        writer,
        object([
            ("jsonrpc", JsonValue::String("2.0".to_owned())),
            ("id", id),
            ("result", result),
        ]),
    )
}

fn write_error<W: Write>(
    writer: &mut W,
    id: JsonValue,
    code: i64,
    message: &str,
) -> io::Result<()> {
    write_message(
        writer,
        object([
            ("jsonrpc", JsonValue::String("2.0".to_owned())),
            ("id", id),
            (
                "error",
                object([
                    ("code", JsonValue::Number(code.to_string())),
                    ("message", JsonValue::String(message.to_owned())),
                ]),
            ),
        ]),
    )
}

fn initialize_result() -> JsonValue {
    object([(
        "capabilities",
        object([
            ("textDocumentSync", JsonValue::Number("1".to_owned())),
            ("completionProvider", object([])),
            ("definitionProvider", JsonValue::Bool(true)),
            ("documentSymbolProvider", JsonValue::Bool(true)),
        ]),
    )])
}

fn publish_diagnostics<W: Write>(writer: &mut W, uri: &str, source: &str) -> io::Result<()> {
    let parsed = crate::parser::parse(source);
    let diagnostics = match parsed.program {
        Some(program) => crate::semantic::check(&program),
        None => parsed.diagnostics,
    };
    let values = diagnostics
        .iter()
        .map(|value| lsp_diagnostic(source, value))
        .collect();
    write_message(
        writer,
        object([
            ("jsonrpc", JsonValue::String("2.0".to_owned())),
            (
                "method",
                JsonValue::String("textDocument/publishDiagnostics".to_owned()),
            ),
            (
                "params",
                object([
                    ("uri", JsonValue::String(uri.to_owned())),
                    ("diagnostics", JsonValue::Array(values)),
                ]),
            ),
        ]),
    )
}

fn lsp_diagnostic(source: &str, diagnostic: &Diagnostic) -> JsonValue {
    object([
        (
            "range",
            range_value(source, diagnostic.span.start, diagnostic.span.end),
        ),
        ("severity", JsonValue::Number("1".to_owned())),
        ("code", JsonValue::String(diagnostic.code.to_owned())),
        ("source", JsonValue::String("Seloryn".to_owned())),
        ("message", JsonValue::String(diagnostic.message.clone())),
    ])
}

fn completion_result(source: &str) -> JsonValue {
    let mut items = Vec::new();
    for keyword in [
        "fn", "let", "var", "return", "record", "enum", "if", "elif", "else", "for", "match",
        "case", "require", "import", "from", "pub",
    ] {
        items.push(object([
            ("label", JsonValue::String(keyword.to_owned())),
            ("kind", JsonValue::Number("14".to_owned())),
            ("detail", JsonValue::String("Seloryn keyword".to_owned())),
        ]));
    }
    for builtin in [
        "print",
        "tensor_f32",
        "tensor_i64",
        "train_linear",
        "train_mlp",
        "range",
    ] {
        items.push(object([
            ("label", JsonValue::String(builtin.to_owned())),
            ("kind", JsonValue::Number("3".to_owned())),
            ("detail", JsonValue::String("Seloryn builtin".to_owned())),
        ]));
    }
    if let Some(program) = crate::parser::parse(source).program {
        append_program_items(&mut items, &program);
    }
    object([
        ("isIncomplete", JsonValue::Bool(false)),
        ("items", JsonValue::Array(items)),
    ])
}

fn append_program_items(items: &mut Vec<JsonValue>, program: &Program) {
    for item in &program.items {
        let (name, kind, detail) = match item {
            Item::Function(value) => (&value.name, 3, "Seloryn function"),
            Item::Binding(value) => (&value.name, 6, "Seloryn binding"),
            Item::Record(value) => (&value.name, 7, "Seloryn record"),
            Item::Enum(value) => (&value.name, 13, "Seloryn enum"),
            _ => continue,
        };
        items.push(object([
            ("label", JsonValue::String(name.clone())),
            ("kind", JsonValue::Number(kind.to_string())),
            ("detail", JsonValue::String(detail.to_owned())),
        ]));
    }
}

fn document_symbols(source: &str) -> JsonValue {
    let Some(program) = crate::parser::parse(source).program else {
        return JsonValue::Array(Vec::new());
    };
    JsonValue::Array(
        program
            .items
            .iter()
            .filter_map(|item| {
                let (name, kind, span) = match item {
                    Item::Function(value) => (&value.name, 12, value.span),
                    Item::Binding(value) => (&value.name, 13, value.span),
                    Item::Record(value) => (&value.name, 5, value.span),
                    Item::Enum(value) => (&value.name, 10, value.span),
                    _ => return None,
                };
                Some(object([
                    ("name", JsonValue::String(name.clone())),
                    ("kind", JsonValue::Number(kind.to_string())),
                    ("range", range_value(source, span.start, span.end)),
                    ("selectionRange", range_value(source, span.start, span.end)),
                ]))
            })
            .collect(),
    )
}

fn definition_result(uri: &str, source: &str, position: &JsonValue) -> Option<JsonValue> {
    let line = position.get("line")?.as_number()?.parse::<usize>().ok()?;
    let character = position
        .get("character")?
        .as_number()?
        .parse::<usize>()
        .ok()?;
    let offset = utf16_position_to_offset(source, line, character)?;
    let word = word_at(source, offset)?;
    for prefix in ["fn ", "record ", "enum ", "let ", "var "] {
        let needle = format!("{prefix}{word}");
        if let Some(start) = source.find(&needle) {
            let name_start = start + prefix.len();
            return Some(JsonValue::Array(vec![object([
                ("uri", JsonValue::String(uri.to_owned())),
                (
                    "range",
                    range_value(source, name_start, name_start + word.len()),
                ),
            ])]));
        }
    }
    None
}

fn document_text(params: Option<&JsonValue>) -> Option<(String, String)> {
    Some((
        nested_string(params, &["textDocument", "uri"])?.to_owned(),
        nested_string(params, &["textDocument", "text"])?.to_owned(),
    ))
}

fn nested_string<'a>(value: Option<&'a JsonValue>, path: &[&str]) -> Option<&'a str> {
    let mut current = value?;
    for key in path {
        current = current.get(key)?;
    }
    current.as_str()
}

fn range_value(source: &str, start: usize, end: usize) -> JsonValue {
    object([
        ("start", position_value(source, start)),
        ("end", position_value(source, end)),
    ])
}

fn position_value(source: &str, offset: usize) -> JsonValue {
    let offset = offset.min(source.len());
    let prefix = &source[..offset];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count();
    let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
    let character = source[line_start..offset].encode_utf16().count();
    object([
        ("line", JsonValue::Number(line.to_string())),
        ("character", JsonValue::Number(character.to_string())),
    ])
}

fn utf16_position_to_offset(source: &str, line: usize, character: usize) -> Option<usize> {
    let mut line_start = 0;
    for (index, value) in source.char_indices() {
        if value == '\n'
            && source[..index]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count()
                == line.saturating_sub(1)
        {
            line_start = index + 1;
            break;
        }
    }
    if line > source.bytes().filter(|byte| *byte == b'\n').count() {
        return None;
    }
    let line_end = source[line_start..]
        .find('\n')
        .map_or(source.len(), |index| line_start + index);
    let mut units = 0;
    for (index, value) in source[line_start..line_end].char_indices() {
        if units >= character {
            return Some(line_start + index);
        }
        units += value.len_utf16();
    }
    (units == character).then_some(line_end)
}

fn word_at(source: &str, offset: usize) -> Option<String> {
    let mut start = offset.min(source.len());
    while start > 0 {
        let value = source[..start].chars().next_back()?;
        if !(value == '_' || value.is_ascii_alphanumeric()) {
            break;
        }
        start -= value.len_utf8();
    }
    let mut end = offset.min(source.len());
    while end < source.len() {
        let value = source[end..].chars().next()?;
        if !(value == '_' || value.is_ascii_alphanumeric()) {
            break;
        }
        end += value.len_utf8();
    }
    (start < end).then(|| source[start..end].to_owned())
}

fn object<const N: usize>(entries: [(&str, JsonValue); N]) -> JsonValue {
    JsonValue::Object(
        entries
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}

trait JsonExt {
    fn as_object(&self) -> Option<&BTreeMap<String, JsonValue>>;
    fn as_array(&self) -> Option<&Vec<JsonValue>>;
    fn as_str(&self) -> Option<&str>;
    fn as_number(&self) -> Option<&str>;
    fn get(&self, key: &str) -> Option<&JsonValue>;
}

impl JsonExt for JsonValue {
    fn as_object(&self) -> Option<&BTreeMap<String, JsonValue>> {
        if let JsonValue::Object(value) = self {
            Some(value)
        } else {
            None
        }
    }
    fn as_array(&self) -> Option<&Vec<JsonValue>> {
        if let JsonValue::Array(value) = self {
            Some(value)
        } else {
            None
        }
    }
    fn as_str(&self) -> Option<&str> {
        if let JsonValue::String(value) = self {
            Some(value)
        } else {
            None
        }
    }
    fn as_number(&self) -> Option<&str> {
        if let JsonValue::Number(value) = self {
            Some(value)
        } else {
            None
        }
    }
    fn get(&self, key: &str) -> Option<&JsonValue> {
        self.as_object()?.get(key)
    }
}
