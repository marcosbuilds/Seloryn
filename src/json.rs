use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<JsonValue>),
    Object(BTreeMap<String, JsonValue>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JsonError {
    pub code: &'static str,
    pub message: String,
}

pub fn parse(source: &str, max_bytes: usize, max_depth: usize) -> Result<JsonValue, JsonError> {
    if source.len() > max_bytes {
        return Err(error("SEL-J003", "JSON input exceeds the byte limit"));
    }
    let mut parser = Parser {
        source,
        cursor: 0,
        max_depth,
    };
    let value = parser.value(0)?;
    parser.whitespace();
    if parser.cursor != source.len() {
        return Err(error("SEL-J001", "trailing data after JSON value"));
    }
    Ok(value)
}

pub fn encode(value: &JsonValue) -> String {
    match value {
        JsonValue::Null => "null".to_owned(),
        JsonValue::Bool(value) => value.to_string(),
        JsonValue::Number(value) => value.clone(),
        JsonValue::String(value) => string(value),
        JsonValue::Array(values) => format!(
            "[{}]",
            values.iter().map(encode).collect::<Vec<_>>().join(",")
        ),
        JsonValue::Object(values) => format!(
            "{{{}}}",
            values
                .iter()
                .map(|(key, value)| format!("{}:{}", string(key), encode(value)))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

pub fn get<'a>(value: &'a JsonValue, path: &[String]) -> Result<&'a JsonValue, JsonError> {
    let mut current = value;
    for part in path {
        current = match current {
            JsonValue::Object(values) => values
                .get(part)
                .ok_or_else(|| error("SEL-J001", format!("JSON key `{part}` was not found")))?,
            _ => return Err(error("SEL-J001", "JSON path requires an object")),
        };
    }
    Ok(current)
}

fn error(code: &'static str, message: impl Into<String>) -> JsonError {
    JsonError {
        code,
        message: message.into(),
    }
}

fn string(value: &str) -> String {
    let mut result = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            character if character.is_control() => {
                result.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => result.push(character),
        }
    }
    result.push('"');
    result
}

struct Parser<'a> {
    source: &'a str,
    cursor: usize,
    max_depth: usize,
}

impl<'a> Parser<'a> {
    fn value(&mut self, depth: usize) -> Result<JsonValue, JsonError> {
        self.whitespace();
        if depth > self.max_depth {
            return Err(error("SEL-J003", "JSON nesting exceeds the depth limit"));
        }
        match self.peek() {
            Some(b'n') => {
                self.expect_bytes(b"null")?;
                Ok(JsonValue::Null)
            }
            Some(b't') => {
                self.expect_bytes(b"true")?;
                Ok(JsonValue::Bool(true))
            }
            Some(b'f') => {
                self.expect_bytes(b"false")?;
                Ok(JsonValue::Bool(false))
            }
            Some(b'"') => self.string().map(JsonValue::String),
            Some(b'[') => self.array(depth),
            Some(b'{') => self.object(depth),
            Some(character) if character == b'-' || character.is_ascii_digit() => {
                self.number().map(JsonValue::Number)
            }
            _ => Err(error("SEL-J001", "expected a JSON value")),
        }
    }

    fn array(&mut self, depth: usize) -> Result<JsonValue, JsonError> {
        self.cursor += 1;
        let mut values = Vec::new();
        self.whitespace();
        if self.consume(b']') {
            return Ok(JsonValue::Array(values));
        }
        loop {
            values.push(self.value(depth + 1)?);
            self.whitespace();
            if self.consume(b']') {
                break;
            }
            if !self.consume(b',') {
                return Err(error("SEL-J001", "expected `,` or `]` in JSON array"));
            }
        }
        Ok(JsonValue::Array(values))
    }

    fn object(&mut self, depth: usize) -> Result<JsonValue, JsonError> {
        self.cursor += 1;
        let mut values = BTreeMap::new();
        self.whitespace();
        if self.consume(b'}') {
            return Ok(JsonValue::Object(values));
        }
        loop {
            self.whitespace();
            let key = match self.peek() {
                Some(b'"') => self.string()?,
                _ => return Err(error("SEL-J001", "JSON object keys must be strings")),
            };
            self.whitespace();
            if !self.consume(b':') {
                return Err(error("SEL-J001", "expected `:` after JSON object key"));
            }
            let value = self.value(depth + 1)?;
            if values.insert(key, value).is_some() {
                return Err(error("SEL-J002", "duplicate JSON object key"));
            }
            self.whitespace();
            if self.consume(b'}') {
                break;
            }
            if !self.consume(b',') {
                return Err(error("SEL-J001", "expected `,` or `}` in JSON object"));
            }
        }
        Ok(JsonValue::Object(values))
    }

    fn string(&mut self) -> Result<String, JsonError> {
        self.cursor += 1;
        let mut result = String::new();
        while let Some(character) = self.next() {
            match character {
                b'"' => return Ok(result),
                b'\\' => {
                    let Some(escape) = self.next() else {
                        return Err(error("SEL-J001", "unterminated JSON escape"));
                    };
                    match escape {
                        b'"' => result.push('"'),
                        b'\\' => result.push('\\'),
                        b'/' => result.push('/'),
                        b'b' => result.push('\u{0008}'),
                        b'f' => result.push('\u{000c}'),
                        b'n' => result.push('\n'),
                        b'r' => result.push('\r'),
                        b't' => result.push('\t'),
                        b'u' => result.push(self.unicode_escape()?),
                        _ => return Err(error("SEL-J001", "unsupported JSON escape")),
                    }
                }
                character if character < 0x20 => {
                    return Err(error(
                        "SEL-J001",
                        "JSON strings cannot contain control bytes",
                    ));
                }
                _character => {
                    let start = self.cursor - 1;
                    let tail = &self.source.as_bytes()[start..];
                    let decoded = std::str::from_utf8(tail)
                        .ok()
                        .and_then(|text| text.chars().next())
                        .ok_or_else(|| error("SEL-J001", "invalid UTF-8 JSON string"))?;
                    self.cursor = start + decoded.len_utf8();
                    result.push(decoded);
                }
            }
        }
        Err(error("SEL-J001", "unterminated JSON string"))
    }

    fn unicode_escape(&mut self) -> Result<char, JsonError> {
        if self.cursor + 4 > self.source.len() {
            return Err(error("SEL-J001", "incomplete JSON Unicode escape"));
        }
        let digits = &self.source.as_bytes()[self.cursor..self.cursor + 4];
        let text = std::str::from_utf8(digits)
            .map_err(|_| error("SEL-J001", "invalid JSON Unicode escape"))?;
        let value = u16::from_str_radix(text, 16)
            .map_err(|_| error("SEL-J001", "invalid JSON Unicode escape"))?;
        self.cursor += 4;
        char::from_u32(value as u32).ok_or_else(|| error("SEL-J001", "invalid JSON code point"))
    }

    fn number(&mut self) -> Result<String, JsonError> {
        let start = self.cursor;
        self.consume(b'-');
        if self.consume(b'0') {
            if self
                .peek()
                .is_some_and(|character| character.is_ascii_digit())
            {
                return Err(error(
                    "SEL-J001",
                    "JSON numbers cannot contain leading zeroes",
                ));
            }
        } else if !self.consume_digits() {
            return Err(error("SEL-J001", "invalid JSON number"));
        }
        if self.consume(b'.') && !self.consume_digits() {
            return Err(error("SEL-J001", "JSON fraction requires digits"));
        }
        if self
            .peek()
            .is_some_and(|character| character == b'e' || character == b'E')
        {
            self.cursor += 1;
            self.consume(b'+');
            self.consume(b'-');
            if !self.consume_digits() {
                return Err(error("SEL-J001", "JSON exponent requires digits"));
            }
        }
        Ok(self.source[start..self.cursor].to_owned())
    }

    fn consume_digits(&mut self) -> bool {
        let start = self.cursor;
        while self
            .peek()
            .is_some_and(|character| character.is_ascii_digit())
        {
            self.cursor += 1;
        }
        self.cursor > start
    }

    fn expect_bytes(&mut self, expected: &[u8]) -> Result<(), JsonError> {
        if self
            .source
            .as_bytes()
            .get(self.cursor..self.cursor + expected.len())
            == Some(expected)
        {
            self.cursor += expected.len();
            Ok(())
        } else {
            Err(error("SEL-J001", "invalid JSON literal"))
        }
    }

    fn whitespace(&mut self) {
        while self
            .peek()
            .is_some_and(|character| matches!(character, b' ' | b'\n' | b'\r' | b'\t'))
        {
            self.cursor += 1;
        }
    }

    fn consume(&mut self, expected: u8) -> bool {
        if self.peek() == Some(expected) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    fn peek(&self) -> Option<u8> {
        self.source.as_bytes().get(self.cursor).copied()
    }

    fn next(&mut self) -> Option<u8> {
        let value = self.peek()?;
        self.cursor += 1;
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{JsonValue, encode, parse};

    #[test]
    fn parses_and_encodes_canonically() {
        let value = parse(r#"{"b":2,"a":true}"#, 128, 4).expect("valid JSON");
        assert_eq!(encode(&value), r#"{"a":true,"b":2}"#);
    }

    #[test]
    fn rejects_duplicate_keys_and_depth() {
        assert_eq!(
            parse(r#"{"a":1,"a":2}"#, 128, 4).unwrap_err().code,
            "SEL-J002"
        );
        assert_eq!(
            parse(r#"{"a":{"b":{"c":{"d":1}}}}"#, 128, 2)
                .unwrap_err()
                .code,
            "SEL-J003"
        );
    }

    #[test]
    fn preserves_unicode_text() {
        let value = parse(r#"{"text":"Santarém — café ☕"}"#, 128, 4).expect("valid JSON");
        assert_eq!(
            value,
            JsonValue::Object(BTreeMap::from([(
                "text".to_owned(),
                JsonValue::String("Santarém — café ☕".to_owned()),
            )]))
        );
    }
}
