use std::path::Path;

pub const ANALYSIS_SCHEMA: &str = "seloryn.analysis.v1";
pub const LANGUAGE_VERSION: &str = "0.10";
pub const RUNTIME_PACKAGE: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
    pub severity: &'static str,
    pub category: &'static str,
    pub help: Option<String>,
    pub stack: Vec<String>,
}

impl Diagnostic {
    pub fn new(code: &'static str, message: impl Into<String>, span: Span) -> Self {
        let (severity, category) = metadata(code);
        Self {
            code,
            message: message.into(),
            span,
            severity,
            category,
            help: default_help(code),
            stack: Vec::new(),
        }
    }

    pub fn with_help(
        code: &'static str,
        message: impl Into<String>,
        span: Span,
        help: impl Into<String>,
    ) -> Self {
        let mut diagnostic = Self::new(code, message, span);
        diagnostic.help = Some(help.into());
        diagnostic
    }
}

pub fn render(path: &Path, source: &str, diagnostic: &Diagnostic) -> String {
    let (line, column) = line_column(source, diagnostic.span.start);
    let mut rendered = format!(
        "{}:{}:{}: {} {}\n",
        path.display(),
        line,
        column,
        diagnostic.code,
        diagnostic.message
    );
    if let Some(help) = &diagnostic.help {
        rendered.push_str(&format!("  help: {help}\n"));
    }
    rendered
}

pub fn render_json(path: &Path, source: &str, diagnostics: &[Diagnostic]) -> String {
    let mut rendered = format!(
        "{{\"schema\":\"{}\",\"language_version\":\"{}\",\"runtime_package\":\"{}\",\"diagnostics\":[",
        ANALYSIS_SCHEMA, LANGUAGE_VERSION, RUNTIME_PACKAGE
    );
    for (index, diagnostic) in diagnostics.iter().enumerate() {
        if index > 0 {
            rendered.push(',');
        }
        let (start_line, start_column) = line_column(source, diagnostic.span.start);
        let (end_line, end_column) = line_column(source, diagnostic.span.end);
        rendered.push_str("{\"code\":");
        rendered.push_str(&json_string(diagnostic.code));
        rendered.push_str(",\"severity\":");
        rendered.push_str(&json_string(diagnostic.severity));
        rendered.push_str(",\"category\":");
        rendered.push_str(&json_string(diagnostic.category));
        rendered.push_str(",\"message\":");
        rendered.push_str(&json_string(&diagnostic.message));
        rendered.push_str(",\"file\":");
        rendered.push_str(&json_string(&path.to_string_lossy()));
        rendered.push_str(",\"primary\":{\"start_byte\":");
        rendered.push_str(&diagnostic.span.start.to_string());
        rendered.push_str(",\"end_byte\":");
        rendered.push_str(&diagnostic.span.end.to_string());
        rendered.push_str(",\"start\":{\"line\":");
        rendered.push_str(&start_line.to_string());
        rendered.push_str(",\"column\":");
        rendered.push_str(&start_column.to_string());
        rendered.push_str("},\"end\":{\"line\":");
        rendered.push_str(&end_line.to_string());
        rendered.push_str(",\"column\":");
        rendered.push_str(&end_column.to_string());
        rendered.push_str("}}");
        rendered.push_str(",\"help\":");
        match &diagnostic.help {
            Some(help) => rendered.push_str(&json_string(help)),
            None => rendered.push_str("null"),
        }
        rendered.push_str(",\"stack\":[");
        for (index, frame) in diagnostic.stack.iter().enumerate() {
            if index > 0 {
                rendered.push(',');
            }
            rendered.push_str(&json_string(frame));
        }
        rendered.push(']');
        rendered.push('}');
    }
    rendered.push_str("]}\n");
    rendered
}

fn metadata(code: &str) -> (&'static str, &'static str) {
    let category = match code.as_bytes().get(4).copied() {
        Some(b'L') => "lexical",
        Some(b'P') => "parser",
        Some(b'T') => "type",
        Some(b'M') => "module",
        Some(b'R') => "runtime",
        Some(b'S') => "security",
        Some(b'C') => "cli",
        _ => "internal",
    };
    ("error", category)
}

fn default_help(code: &str) -> Option<String> {
    match code.as_bytes().get(4).copied() {
        Some(b'L') => Some("check UTF-8, indentation, and literal syntax".to_owned()),
        Some(b'P') => Some("check the surrounding syntax and block indentation".to_owned()),
        Some(b'T') => Some("check the expected and actual types".to_owned()),
        Some(b'M') => Some("check the referenced name or module".to_owned()),
        Some(b'R') => Some("handle the reported runtime condition".to_owned()),
        Some(b'C') => Some("check the command and its arguments".to_owned()),
        _ => None,
    }
}

fn json_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character.is_control() => {
                escaped.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => escaped.push(character),
        }
    }
    escaped.push('"');
    escaped
}

fn line_column(source: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(source.len());
    let prefix = &source[..offset];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
    let column = source[line_start..offset].chars().count() + 1;
    (line, column)
}

#[cfg(test)]
mod tests {
    use super::{Diagnostic, Span, line_column, render};
    use std::path::Path;

    #[test]
    fn locations_use_one_based_line_and_column() {
        assert_eq!(line_column("first\nsecond", 6), (2, 1));
        assert_eq!(line_column("first\nsecond", 8), (2, 3));
    }

    #[test]
    fn rendered_diagnostics_include_stable_code_and_location() {
        let diagnostic = Diagnostic::new("SEL-P001", "expected expression", Span::new(6, 6));
        assert_eq!(
            render(Path::new("sample.sely"), "first\nsecond", &diagnostic),
            "sample.sely:2:1: SEL-P001 expected expression\n  help: check the surrounding syntax and block indentation\n"
        );
    }
}
