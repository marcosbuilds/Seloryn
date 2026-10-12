use std::path::Path;

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
}

impl Diagnostic {
    pub fn new(code: &'static str, message: impl Into<String>, span: Span) -> Self {
        Self {
            code,
            message: message.into(),
            span,
        }
    }
}

pub fn render(path: &Path, source: &str, diagnostic: &Diagnostic) -> String {
    let (line, column) = line_column(source, diagnostic.span.start);
    format!(
        "{}:{}:{}: {} {}\n",
        path.display(),
        line,
        column,
        diagnostic.code,
        diagnostic.message
    )
}

pub fn render_at_start(path: &Path, diagnostic: &Diagnostic) -> String {
    format!(
        "{}:1:1: {} {}\n",
        path.display(),
        diagnostic.code,
        diagnostic.message
    )
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
            "sample.sely:2:1: SEL-P001 expected expression\n"
        );
    }
}
