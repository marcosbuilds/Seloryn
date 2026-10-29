use crate::diagnostic::{Diagnostic, Span};
use unicode_ident::{is_xid_continue, is_xid_start};
use unicode_normalization::UnicodeNormalization;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TokenKind {
    Identifier(String),
    Integer(String),
    Float(String),
    StringLiteral(String),
    Keyword(Keyword),
    Reserved(String),
    Symbol(Symbol),
    Newline,
    Indent,
    Dedent,
    Eof,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Keyword {
    Let,
    Var,
    Const,
    Fn,
    Return,
    Record,
    Enum,
    Match,
    Case,
    If,
    Elif,
    Else,
    For,
    While,
    Loop,
    Break,
    Continue,
    In,
    Is,
    Not,
    And,
    Or,
    Require,
    Import,
    From,
    As,
    Pub,
    Module,
    True,
    False,
    Null,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Symbol {
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    LeftBrace,
    RightBrace,
    Colon,
    Comma,
    Dot,
    Question,
    QuestionDot,
    QuestionQuestion,
    Arrow,
    FatArrow,
    Pipe,
    Plus,
    Minus,
    Star,
    Power,
    Slash,
    Percent,
    Equal,
    EqualEqual,
    Bang,
    BangEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

pub struct LexResult {
    pub tokens: Vec<Token>,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn lex(source: &str) -> LexResult {
    Lexer::new(source).run()
}

struct Lexer<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    diagnostics: Vec<Diagnostic>,
    indent_stack: Vec<usize>,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            tokens: Vec::new(),
            diagnostics: Vec::new(),
            indent_stack: vec![0],
        }
    }

    fn run(mut self) -> LexResult {
        if self.source.starts_with('\u{feff}') {
            self.diagnostics.push(Diagnostic::new(
                "SEL-L009",
                "a UTF-8 BOM is not accepted in this syntax gate",
                Span::new(0, '\u{feff}'.len_utf8()),
            ));
        }

        let mut offset = 0;
        while offset < self.source.len() {
            let remaining = &self.source[offset..];
            let line_end = remaining
                .find('\n')
                .map_or(self.source.len(), |relative| offset + relative + 1);
            let mut body_end = line_end;
            if body_end > offset && self.source.as_bytes()[body_end - 1] == b'\n' {
                body_end -= 1;
                if body_end > offset && self.source.as_bytes()[body_end - 1] == b'\r' {
                    body_end -= 1;
                }
            } else if body_end > offset && self.source.as_bytes()[body_end - 1] == b'\r' {
                self.diagnostics.push(Diagnostic::new(
                    "SEL-L006",
                    "a line must end with LF or CRLF",
                    Span::new(body_end - 1, body_end),
                ));
                body_end -= 1;
            }

            let body = &self.source[offset..body_end];
            self.scan_line(offset, body);
            offset = line_end;
        }

        let end = self.source.len();
        while self.indent_stack.len() > 1 {
            self.indent_stack.pop();
            self.tokens
                .push(Token::new(TokenKind::Dedent, Span::new(end, end)));
        }
        self.tokens
            .push(Token::new(TokenKind::Eof, Span::new(end, end)));

        LexResult {
            tokens: self.tokens,
            diagnostics: self.diagnostics,
        }
    }

    fn scan_line(&mut self, line_start: usize, body: &str) {
        let mut leading_end = 0;
        let mut spaces = 0;
        let mut has_leading_tab = false;
        for (index, character) in body.char_indices() {
            match character {
                ' ' => {
                    spaces += 1;
                    leading_end = index + 1;
                }
                '\t' => {
                    has_leading_tab = true;
                    leading_end = index + 1;
                }
                _ => break,
            }
        }

        if has_leading_tab {
            self.diagnostics.push(Diagnostic::new(
                "SEL-L002",
                "tabs are not allowed in leading indentation",
                Span::new(line_start, line_start + leading_end),
            ));
        }
        if spaces % 4 != 0 {
            self.diagnostics.push(Diagnostic::new(
                "SEL-L003",
                "leading indentation must use multiples of four spaces",
                Span::new(line_start, line_start + leading_end),
            ));
        }

        let content = &body[leading_end..];
        if content.is_empty() || content.starts_with('#') {
            return;
        }

        self.update_indentation(spaces, line_start, leading_end);
        self.scan_tokens(content, line_start + leading_end);
        let newline_start = line_start + body.len();
        self.tokens.push(Token::new(
            TokenKind::Newline,
            Span::new(newline_start, newline_start),
        ));
    }

    fn update_indentation(&mut self, indentation: usize, line_start: usize, leading_end: usize) {
        let current = *self.indent_stack.last().expect("root indentation exists");
        let span = Span::new(line_start, line_start + leading_end);
        if indentation > current {
            self.indent_stack.push(indentation);
            self.tokens.push(Token::new(TokenKind::Indent, span));
            return;
        }

        if indentation < current {
            while self.indent_stack.len() > 1
                && indentation < *self.indent_stack.last().expect("indentation exists")
            {
                self.indent_stack.pop();
                self.tokens.push(Token::new(TokenKind::Dedent, span));
            }
            if indentation != *self.indent_stack.last().expect("root indentation exists") {
                self.diagnostics.push(Diagnostic::new(
                    "SEL-L003",
                    "indentation does not match an outer block",
                    span,
                ));
            }
        }
    }

    fn scan_tokens(&mut self, line: &str, line_start: usize) {
        let mut index = 0;
        while index < line.len() {
            let character = line[index..]
                .chars()
                .next()
                .expect("index is on a character boundary");
            if character == ' ' {
                index += 1;
                continue;
            }
            if character == '\t' {
                self.diagnostics.push(Diagnostic::new(
                    "SEL-L002",
                    "tabs are only allowed inside string literals",
                    Span::new(line_start + index, line_start + index + 1),
                ));
                index += 1;
                continue;
            }
            if character == '#' {
                break;
            }

            let start = index;
            if character == '_' || is_xid_start(character) {
                index += character.len_utf8();
                while index < line.len() {
                    let next = line[index..].chars().next().expect("valid boundary");
                    if next == '_' || is_xid_continue(next) {
                        index += next.len_utf8();
                    } else {
                        break;
                    }
                }
                let text = &line[start..index];
                if text.nfc().collect::<String>() != text {
                    self.diagnostics.push(Diagnostic::new(
                        "SEL-L010",
                        "Unicode identifiers must already use NFC normalization",
                        Span::new(line_start + start, line_start + index),
                    ));
                }
                let kind = if reserved(text) {
                    TokenKind::Reserved(text.to_owned())
                } else {
                    keyword(text).map_or_else(
                        || TokenKind::Identifier(text.to_owned()),
                        |keyword| match keyword {
                            Keyword::True
                            | Keyword::False
                            | Keyword::Null
                            | Keyword::Let
                            | Keyword::Var
                            | Keyword::Const
                            | Keyword::Fn
                            | Keyword::Return
                            | Keyword::Record
                            | Keyword::Enum
                            | Keyword::Match
                            | Keyword::Case
                            | Keyword::If
                            | Keyword::Elif
                            | Keyword::Else
                            | Keyword::For
                            | Keyword::While
                            | Keyword::Loop
                            | Keyword::Break
                            | Keyword::Continue
                            | Keyword::In
                            | Keyword::Is
                            | Keyword::Not
                            | Keyword::And
                            | Keyword::Or
                            | Keyword::Require
                            | Keyword::Import
                            | Keyword::From
                            | Keyword::As
                            | Keyword::Pub
                            | Keyword::Module => TokenKind::Keyword(keyword),
                        },
                    )
                };
                self.tokens.push(Token::new(
                    kind,
                    Span::new(line_start + start, line_start + index),
                ));
                continue;
            }

            if character.is_ascii_digit() {
                index = self.scan_number(line, line_start, start);
                continue;
            }

            if character == '"' || character == '\'' {
                index = self.scan_string(line, line_start, start, character);
                continue;
            }

            if let Some((symbol, width)) = multi_symbol(&line[start..]) {
                index += width;
                self.tokens.push(Token::new(
                    TokenKind::Symbol(symbol),
                    Span::new(line_start + start, line_start + index),
                ));
                continue;
            }

            if let Some(symbol) = single_symbol(character) {
                index += character.len_utf8();
                self.tokens.push(Token::new(
                    TokenKind::Symbol(symbol),
                    Span::new(line_start + start, line_start + index),
                ));
                continue;
            }

            self.diagnostics.push(Diagnostic::new(
                "SEL-L004",
                format!("unsupported character `{character}`"),
                Span::new(
                    line_start + start,
                    line_start + start + character.len_utf8(),
                ),
            ));
            index += character.len_utf8();
        }
    }

    fn scan_number(&mut self, line: &str, line_start: usize, start: usize) -> usize {
        let mut index = start;
        while index < line.len() {
            let character = line[index..].chars().next().expect("valid boundary");
            if character.is_ascii_digit() || character == '_' {
                index += character.len_utf8();
            } else {
                break;
            }
        }

        let integer_end = index;
        let mut is_float = false;
        if line[index..].starts_with('.') && !line[index..].starts_with("..") {
            is_float = true;
            index += 1;
            let fraction_start = index;
            while index < line.len() {
                let character = line[index..].chars().next().expect("valid boundary");
                if character.is_ascii_digit() || character == '_' {
                    index += character.len_utf8();
                } else {
                    break;
                }
            }
            if fraction_start == index {
                self.diagnostics.push(Diagnostic::new(
                    "SEL-L007",
                    "a decimal point in a floating literal must be followed by digits",
                    Span::new(line_start + start, line_start + index),
                ));
            }
        }

        if matches!(line[index..].chars().next(), Some('e' | 'E')) {
            is_float = true;
            index += 1;
            if matches!(line[index..].chars().next(), Some('+' | '-')) {
                index += 1;
            }
            let exponent_start = index;
            while index < line.len() {
                let character = line[index..].chars().next().expect("valid boundary");
                if character.is_ascii_digit() || character == '_' {
                    index += character.len_utf8();
                } else {
                    break;
                }
            }
            if exponent_start == index {
                self.diagnostics.push(Diagnostic::new(
                    "SEL-L007",
                    "an exponent must contain at least one digit",
                    Span::new(line_start + start, line_start + index),
                ));
            }
        }

        let text = &line[start..index];
        if !valid_digit_grouping(text, is_float) {
            self.diagnostics.push(Diagnostic::new(
                "SEL-L007",
                "numeric separators must appear only between digits",
                Span::new(line_start + start, line_start + index),
            ));
        }
        if line[index..].starts_with("x")
            || line[index..].starts_with("X")
            || line[index..].starts_with("o")
            || line[index..].starts_with("O")
            || line[index..].starts_with("b")
            || line[index..].starts_with("B")
        {
            self.diagnostics.push(Diagnostic::new(
                "SEL-L007",
                "only decimal numeric literals are accepted in this syntax gate",
                Span::new(line_start + start, line_start + index + 1),
            ));
        }
        if line[index..]
            .chars()
            .next()
            .is_some_and(|next| next == '_' || is_xid_start(next))
        {
            let mut end = index;
            while end < line.len() {
                let next = line[end..].chars().next().expect("valid boundary");
                if next == '_' || is_xid_continue(next) {
                    end += next.len_utf8();
                } else {
                    break;
                }
            }
            self.diagnostics.push(Diagnostic::new(
                "SEL-L007",
                "numeric literals cannot be immediately followed by identifier characters",
                Span::new(line_start + start, line_start + end),
            ));
            index = end;
        }
        let kind = if is_float {
            TokenKind::Float(text.to_owned())
        } else {
            TokenKind::Integer(line[start..integer_end].to_owned())
        };
        self.tokens.push(Token::new(
            kind,
            Span::new(line_start + start, line_start + index),
        ));
        index
    }

    fn scan_string(&mut self, line: &str, line_start: usize, start: usize, quote: char) -> usize {
        let mut index = start + quote.len_utf8();
        let mut value = String::new();
        let mut closed = false;
        while index < line.len() {
            let character = line[index..].chars().next().expect("valid boundary");
            index += character.len_utf8();
            if character == quote {
                closed = true;
                break;
            }
            if character == '\\' {
                let escape_start = index - 1;
                let Some(escape) = line[index..].chars().next() else {
                    break;
                };
                index += escape.len_utf8();
                match escape {
                    'n' => value.push('\n'),
                    'r' => value.push('\r'),
                    't' => value.push('\t'),
                    '\\' => value.push('\\'),
                    '"' => value.push('"'),
                    '\'' => value.push('\''),
                    'u' => match self.parse_unicode_escape(line, &mut index) {
                        Some(character) => value.push(character),
                        None => self.diagnostics.push(Diagnostic::new(
                            "SEL-L008",
                            "invalid Unicode escape; expected `\\u{hex}`",
                            Span::new(line_start + escape_start, line_start + index),
                        )),
                    },
                    _ => self.diagnostics.push(Diagnostic::new(
                        "SEL-L008",
                        format!("unsupported escape `\\{escape}`"),
                        Span::new(line_start + escape_start, line_start + index),
                    )),
                }
            } else {
                value.push(character);
            }
        }

        if !closed {
            self.diagnostics.push(Diagnostic::new(
                "SEL-L005",
                "unterminated string literal",
                Span::new(line_start + start, line_start + line.len()),
            ));
        }
        self.tokens.push(Token::new(
            TokenKind::StringLiteral(value),
            Span::new(line_start + start, line_start + index),
        ));
        index
    }

    fn parse_unicode_escape(&self, line: &str, index: &mut usize) -> Option<char> {
        if !line[*index..].starts_with('{') {
            return None;
        }
        *index += 1;
        let digits_start = *index;
        while *index < line.len() {
            let character = line[*index..].chars().next().expect("valid boundary");
            if character == '}' {
                break;
            }
            if !character.is_ascii_hexdigit() {
                return None;
            }
            *index += character.len_utf8();
        }
        if digits_start == *index || !line[*index..].starts_with('}') {
            return None;
        }
        let value = u32::from_str_radix(&line[digits_start..*index], 16).ok()?;
        *index += 1;
        char::from_u32(value)
    }
}

impl Token {
    fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}

fn keyword(text: &str) -> Option<Keyword> {
    Some(match text {
        "let" => Keyword::Let,
        "var" => Keyword::Var,
        "const" => Keyword::Const,
        "fn" => Keyword::Fn,
        "return" => Keyword::Return,
        "record" => Keyword::Record,
        "enum" => Keyword::Enum,
        "match" => Keyword::Match,
        "case" => Keyword::Case,
        "if" => Keyword::If,
        "elif" => Keyword::Elif,
        "else" => Keyword::Else,
        "for" => Keyword::For,
        "while" => Keyword::While,
        "loop" => Keyword::Loop,
        "break" => Keyword::Break,
        "continue" => Keyword::Continue,
        "in" => Keyword::In,
        "is" => Keyword::Is,
        "not" => Keyword::Not,
        "and" => Keyword::And,
        "or" => Keyword::Or,
        "require" => Keyword::Require,
        "import" => Keyword::Import,
        "from" => Keyword::From,
        "as" => Keyword::As,
        "pub" => Keyword::Pub,
        "module" => Keyword::Module,
        "true" => Keyword::True,
        "false" => Keyword::False,
        "null" => Keyword::Null,
        "trait" | "impl" | "class" | "async" | "await" | "using" | "defer" | "try" | "catch"
        | "finally" | "yield" => return None,
        _ => return None,
    })
}

fn reserved(text: &str) -> bool {
    matches!(
        text,
        "trait"
            | "impl"
            | "class"
            | "async"
            | "await"
            | "using"
            | "defer"
            | "try"
            | "catch"
            | "finally"
            | "yield"
    )
}

fn valid_digit_grouping(text: &str, is_float: bool) -> bool {
    let mut previous_digit = false;
    for character in text.chars() {
        if character.is_ascii_digit() {
            previous_digit = true;
        } else if character == '_' {
            if !previous_digit {
                return false;
            }
            previous_digit = false;
        } else if character == '.'
            || character == 'e'
            || character == 'E'
            || character == '+'
            || character == '-'
        {
            previous_digit = false;
        } else {
            return false;
        }
    }
    previous_digit
        && (!is_float
            || text
                .chars()
                .any(|character| character == '.' || character == 'e' || character == 'E'))
}

fn multi_symbol(text: &str) -> Option<(Symbol, usize)> {
    [
        ("->", Symbol::Arrow),
        ("=>", Symbol::FatArrow),
        ("==", Symbol::EqualEqual),
        ("!=", Symbol::BangEqual),
        ("<=", Symbol::LessEqual),
        (">=", Symbol::GreaterEqual),
        ("**", Symbol::Power),
        ("??", Symbol::QuestionQuestion),
        ("?.", Symbol::QuestionDot),
        ("|>", Symbol::Pipe),
    ]
    .into_iter()
    .find_map(|(spelling, symbol)| {
        text.starts_with(spelling)
            .then_some((symbol, spelling.len()))
    })
}

fn single_symbol(character: char) -> Option<Symbol> {
    Some(match character {
        '(' => Symbol::LeftParen,
        ')' => Symbol::RightParen,
        '[' => Symbol::LeftBracket,
        ']' => Symbol::RightBracket,
        '{' => Symbol::LeftBrace,
        '}' => Symbol::RightBrace,
        ':' => Symbol::Colon,
        ',' => Symbol::Comma,
        '.' => Symbol::Dot,
        '?' => Symbol::Question,
        '+' => Symbol::Plus,
        '-' => Symbol::Minus,
        '*' => Symbol::Star,
        '/' => Symbol::Slash,
        '%' => Symbol::Percent,
        '=' => Symbol::Equal,
        '!' => Symbol::Bang,
        '<' => Symbol::Less,
        '>' => Symbol::Greater,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::{Keyword, Symbol, TokenKind, lex};

    #[test]
    fn layout_tokens_preserve_crlf_and_nested_blocks() {
        let result =
            lex("fn main():\r\n    let value = 1\r\n    if true:\r\n        return value\r\n");
        assert!(result.diagnostics.is_empty());
        assert!(
            result
                .tokens
                .iter()
                .any(|token| token.kind == TokenKind::Indent)
        );
        assert!(
            result
                .tokens
                .iter()
                .filter(|token| token.kind == TokenKind::Dedent)
                .count()
                >= 2
        );
    }

    #[test]
    fn tabs_and_inconsistent_indent_are_diagnostics() {
        let result = lex("fn main():\n\treturn 1\n  return 2\n");
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "SEL-L002")
        );
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "SEL-L003")
        );
    }

    #[test]
    fn reserved_words_are_explicitly_tokenized() {
        let result = lex("class Example:\n");
        assert!(result.diagnostics.is_empty());
        assert!(
            result
                .tokens
                .iter()
                .any(|token| token.kind == TokenKind::Reserved("class".to_owned()))
        );
    }

    #[test]
    fn numeric_literals_and_operators_get_distinct_tokens() {
        let result = lex("let value = -2 ** 2\n");
        assert!(result.diagnostics.is_empty());
        assert!(
            result
                .tokens
                .iter()
                .any(|token| token.kind == TokenKind::Integer("2".to_owned()))
        );
        assert!(
            result
                .tokens
                .iter()
                .any(|token| token.kind == TokenKind::Symbol(Symbol::Power))
        );
        assert!(
            result
                .tokens
                .iter()
                .any(|token| token.kind == TokenKind::Keyword(Keyword::Let))
        );
    }

    #[test]
    fn non_nfc_identifier_is_rejected_without_rewriting_span() {
        let result = lex("let cafe\u{301} = 1\n");
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "SEL-L010")
        );
    }
}
