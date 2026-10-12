use crate::diagnostic::{Diagnostic, Span};
use crate::lexer::{Keyword, Symbol, Token, TokenKind, lex};

pub struct ParseResult {
    pub program: Option<Program>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Program {
    pub items: Vec<Item>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Function(Function),
    Binding(Binding),
    Record(Record),
    Enum(EnumDecl),
    Directive(Directive),
    Statement(Statement),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Function {
    pub public: bool,
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<TypeExpr>,
    pub body: Vec<Statement>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Parameter {
    pub name: String,
    pub type_expr: Option<TypeExpr>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    pub public: bool,
    pub name: String,
    pub fields: Vec<Field>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EnumDecl {
    pub public: bool,
    pub name: String,
    pub variants: Vec<Variant>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub name: String,
    pub type_expr: TypeExpr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Variant {
    pub name: String,
    pub fields: Vec<TypeExpr>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Directive {
    pub keyword: String,
    pub text: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Statement {
    Binding(Binding),
    Assignment {
        name: String,
        value: Expr,
        span: Span,
    },
    Return {
        value: Option<Expr>,
        span: Span,
    },
    Require {
        condition: Expr,
        failure: Expr,
        span: Span,
    },
    If {
        branches: Vec<IfBranch>,
        otherwise: Option<Vec<Statement>>,
        span: Span,
    },
    Match(MatchStatement),
    For {
        name: String,
        iterable: Expr,
        body: Vec<Statement>,
        span: Span,
    },
    Expression(Expr),
}

#[derive(Clone, Debug, PartialEq)]
pub struct IfBranch {
    pub condition: Expr,
    pub body: Vec<Statement>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MatchStatement {
    pub value: Expr,
    pub cases: Vec<MatchCase>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MatchCase {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Vec<Statement>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Pattern {
    Wildcard(Span),
    Binding {
        name: String,
        span: Span,
    },
    Boolean {
        value: bool,
        span: Span,
    },
    Integer {
        text: String,
        span: Span,
    },
    String {
        value: String,
        span: Span,
    },
    Null(Span),
    Constructor {
        path: Vec<String>,
        arguments: Vec<Pattern>,
        span: Span,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Binding {
    pub mutable: bool,
    pub name: String,
    pub annotation: Option<TypeExpr>,
    pub value: Expr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TypeExpr {
    pub name: String,
    pub arguments: Vec<TypeExpr>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Name {
        name: String,
        span: Span,
    },
    Integer {
        text: String,
        span: Span,
    },
    Float {
        text: String,
        span: Span,
    },
    String {
        value: String,
        span: Span,
    },
    Boolean {
        value: bool,
        span: Span,
    },
    Null(Span),
    List {
        values: Vec<Expr>,
        span: Span,
    },
    Group {
        value: Box<Expr>,
        span: Span,
    },
    Unary {
        operator: String,
        value: Box<Expr>,
        span: Span,
    },
    Binary {
        left: Box<Expr>,
        operator: String,
        right: Box<Expr>,
        span: Span,
    },
    Call {
        callee: Box<Expr>,
        arguments: Vec<Expr>,
        span: Span,
    },
    Index {
        object: Box<Expr>,
        index: Box<Expr>,
        span: Span,
    },
    Member {
        object: Box<Expr>,
        name: String,
        optional: bool,
        span: Span,
    },
    Propagate {
        value: Box<Expr>,
        span: Span,
    },
}

pub fn parse(source: &str) -> ParseResult {
    let lexed = lex(source);
    let mut parser = Parser::new(source, lexed.tokens, lexed.diagnostics);
    let program = parser.parse_program();
    let program = (parser.diagnostics.is_empty()).then_some(program);
    ParseResult {
        program,
        diagnostics: parser.diagnostics,
    }
}

struct Parser<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    cursor: usize,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str, tokens: Vec<Token>, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            source,
            tokens,
            cursor: 0,
            diagnostics,
        }
    }

    fn parse_program(&mut self) -> Program {
        let start = self.peek().span.start;
        let mut items = Vec::new();
        while !self.at(TokenKind::Eof) {
            if self.consume_kind(TokenKind::Newline).is_some() {
                continue;
            }
            if self.at(TokenKind::Dedent) {
                self.error("SEL-P002", "unexpected block end", self.peek().span);
                self.bump();
                continue;
            }
            let before = self.cursor;
            if let Some(item) = self.parse_item() {
                items.push(item);
            }
            if self.cursor == before {
                self.error("SEL-P002", "parser made no progress", self.peek().span);
                self.synchronize_line();
            }
        }
        let end = self.peek().span.end;
        Program {
            items,
            span: Span::new(start, end),
        }
    }

    fn parse_item(&mut self) -> Option<Item> {
        let public = self.consume_keyword(Keyword::Pub).is_some();
        let item = match self.peek().kind.clone() {
            TokenKind::Keyword(Keyword::Fn) => self.parse_function(public),
            TokenKind::Keyword(Keyword::Record) => self.parse_record(public),
            TokenKind::Keyword(Keyword::Enum) => self.parse_enum(public),
            TokenKind::Keyword(Keyword::Let) | TokenKind::Keyword(Keyword::Var) => {
                if public {
                    self.error(
                        "SEL-P003",
                        "`pub` is only supported on functions, records and enums",
                        self.previous_span(),
                    );
                }
                self.parse_binding().map(Item::Binding)
            }
            TokenKind::Keyword(Keyword::Module)
            | TokenKind::Keyword(Keyword::Import)
            | TokenKind::Keyword(Keyword::From) => {
                if public {
                    self.error(
                        "SEL-P003",
                        "`pub` is only supported on functions, records and enums",
                        self.previous_span(),
                    );
                }
                self.parse_directive()
            }
            TokenKind::Keyword(Keyword::Const) => {
                let span = self.bump().span;
                self.error(
                    "SEL-P003",
                    "`const` is reserved until constant evaluation is implemented",
                    span,
                );
                None
            }
            TokenKind::Reserved(name) => {
                let span = self.bump().span;
                if public {
                    self.error(
                        "SEL-P003",
                        "`pub` is only supported on functions, records and enums",
                        self.previous_span(),
                    );
                }
                self.error(
                    "SEL-P003",
                    format!("reserved syntax `{name}` is not implemented"),
                    span,
                );
                None
            }
            _ => {
                if public {
                    self.error(
                        "SEL-P003",
                        "`pub` is only supported on functions, records and enums",
                        self.previous_span(),
                    );
                }
                self.parse_statement().map(Item::Statement)
            }
        }?;
        Some(item)
    }

    fn parse_function(&mut self, public: bool) -> Option<Item> {
        let start = self
            .expect_keyword(Keyword::Fn, "expected `fn`")?
            .span
            .start;
        let (name, _) = self.expect_identifier("expected a function name")?;
        self.expect_symbol(Symbol::LeftParen, "expected `(` after function name")?;
        let mut parameters = Vec::new();
        while !self.at_symbol(Symbol::RightParen) && !self.at(TokenKind::Eof) {
            let parameter_start = self.peek().span.start;
            let (parameter_name, parameter_span) =
                self.expect_identifier("expected parameter name")?;
            let type_expr = if self.consume_symbol(Symbol::Colon).is_some() {
                self.parse_type()
            } else {
                None
            };
            let end = type_expr
                .as_ref()
                .map_or(parameter_span.end, |type_expr| type_expr.span.end);
            parameters.push(Parameter {
                name: parameter_name,
                type_expr,
                span: Span::new(parameter_start, end),
            });
            if self.consume_symbol(Symbol::Comma).is_none() {
                break;
            }
        }
        self.expect_symbol(Symbol::RightParen, "expected `)` after parameters")?;
        let return_type = if self.consume_symbol(Symbol::Arrow).is_some() {
            self.parse_type()
        } else {
            None
        };
        self.expect_symbol(Symbol::Colon, "expected `:` after function signature")?;
        let body = self.parse_block()?;
        let end = self.previous_span().end;
        Some(Item::Function(Function {
            public,
            name,
            parameters,
            return_type,
            body,
            span: Span::new(start, end),
        }))
    }

    fn parse_record(&mut self, public: bool) -> Option<Item> {
        let start = self
            .expect_keyword(Keyword::Record, "expected `record`")?
            .span
            .start;
        let (name, _) = self.expect_identifier("expected a record name")?;
        self.expect_symbol(Symbol::Colon, "expected `:` after record name")?;
        self.expect_newline("expected a new line after record header")?;
        self.expect_kind(TokenKind::Indent, "expected an indented record body")?;
        let mut fields = Vec::new();
        while !self.at(TokenKind::Dedent) && !self.at(TokenKind::Eof) {
            if self.consume_kind(TokenKind::Newline).is_some() {
                continue;
            }
            let field_start = self.peek().span.start;
            let (field_name, _) = self.expect_identifier("expected record field name")?;
            self.expect_symbol(Symbol::Colon, "expected `:` after record field name")?;
            let Some(type_expr) = self.parse_type() else {
                self.synchronize_line();
                continue;
            };
            let field_end = type_expr.span.end;
            fields.push(Field {
                name: field_name,
                type_expr,
                span: Span::new(field_start, field_end),
            });
            self.finish_line();
        }
        let end = self
            .expect_kind(TokenKind::Dedent, "expected end of record body")?
            .span
            .end;
        Some(Item::Record(Record {
            public,
            name,
            fields,
            span: Span::new(start, end),
        }))
    }

    fn parse_enum(&mut self, public: bool) -> Option<Item> {
        let start = self
            .expect_keyword(Keyword::Enum, "expected `enum`")?
            .span
            .start;
        let (name, _) = self.expect_identifier("expected an enum name")?;
        self.expect_symbol(Symbol::Colon, "expected `:` after enum name")?;
        self.expect_newline("expected a new line after enum header")?;
        self.expect_kind(TokenKind::Indent, "expected an indented enum body")?;
        let mut variants = Vec::new();
        while !self.at(TokenKind::Dedent) && !self.at(TokenKind::Eof) {
            if self.consume_kind(TokenKind::Newline).is_some() {
                continue;
            }
            let variant_start = self.peek().span.start;
            let (variant_name, _) = self.expect_identifier("expected enum variant name")?;
            let mut fields = Vec::new();
            if self.consume_symbol(Symbol::LeftParen).is_some() {
                while !self.at_symbol(Symbol::RightParen) && !self.at(TokenKind::Eof) {
                    if let Some(type_expr) = self.parse_type() {
                        fields.push(type_expr);
                    }
                    if self.consume_symbol(Symbol::Comma).is_none() {
                        break;
                    }
                }
                self.expect_symbol(Symbol::RightParen, "expected `)` after enum variant")?;
            }
            let end = self.previous_span().end;
            variants.push(Variant {
                name: variant_name,
                fields,
                span: Span::new(variant_start, end),
            });
            self.finish_line();
        }
        let end = self
            .expect_kind(TokenKind::Dedent, "expected end of enum body")?
            .span
            .end;
        Some(Item::Enum(EnumDecl {
            public,
            name,
            variants,
            span: Span::new(start, end),
        }))
    }

    fn parse_directive(&mut self) -> Option<Item> {
        let start = self.peek().span.start;
        let keyword = self.bump_text();
        let content_start = self.peek().span.start;
        while !self.at(TokenKind::Newline) && !self.at(TokenKind::Eof) {
            self.bump();
        }
        let content_end = self.previous_span().end.max(content_start);
        self.finish_line();
        Some(Item::Directive(Directive {
            keyword,
            text: self.source[content_start..content_end].trim().to_owned(),
            span: Span::new(start, content_end),
        }))
    }

    fn parse_statement(&mut self) -> Option<Statement> {
        let statement = match self.peek().kind.clone() {
            TokenKind::Keyword(Keyword::Let) | TokenKind::Keyword(Keyword::Var) => {
                self.parse_binding().map(Statement::Binding)
            }
            TokenKind::Keyword(Keyword::Return) => self.parse_return(),
            TokenKind::Keyword(Keyword::Require) => self.parse_require(),
            TokenKind::Keyword(Keyword::If) => self.parse_if(),
            TokenKind::Keyword(Keyword::Match) => self.parse_match(),
            TokenKind::Keyword(Keyword::For) => self.parse_for(),
            TokenKind::Keyword(Keyword::Const) => {
                let span = self.bump().span;
                self.error(
                    "SEL-P003",
                    "`const` is reserved until constant evaluation is implemented",
                    span,
                );
                None
            }
            TokenKind::Reserved(name) => {
                let span = self.bump().span;
                self.error(
                    "SEL-P003",
                    format!("reserved syntax `{name}` is not implemented"),
                    span,
                );
                None
            }
            _ => {
                let expression = self.parse_expression(0)?;
                if self.consume_symbol(Symbol::Equal).is_some() {
                    let Expr::Name { name, span } = expression else {
                        self.error(
                            "SEL-P004",
                            "only a named `var` binding can be assigned",
                            expression.span(),
                        );
                        self.synchronize_line();
                        return None;
                    };
                    let value = self.parse_expression(0)?;
                    let span = Span::new(span.start, value.span().end);
                    self.finish_line();
                    return Some(Statement::Assignment { name, value, span });
                }
                self.finish_line();
                Some(Statement::Expression(expression))
            }
        }?;
        Some(statement)
    }

    fn parse_binding(&mut self) -> Option<Binding> {
        let keyword = self.bump();
        let mutable = keyword.kind == TokenKind::Keyword(Keyword::Var);
        let start = keyword.span.start;
        let (name, _) = self.expect_identifier("expected a binding name")?;
        let annotation = if self.consume_symbol(Symbol::Colon).is_some() {
            self.parse_type()
        } else {
            None
        };
        self.expect_symbol(Symbol::Equal, "expected `=` in binding")?;
        let value = self.parse_expression(0)?;
        let end = value.span().end;
        self.finish_line();
        Some(Binding {
            mutable,
            name,
            annotation,
            value,
            span: Span::new(start, end),
        })
    }

    fn parse_return(&mut self) -> Option<Statement> {
        let start = self
            .expect_keyword(Keyword::Return, "expected `return`")?
            .span
            .start;
        let value =
            if self.at(TokenKind::Newline) || self.at(TokenKind::Dedent) || self.at(TokenKind::Eof)
            {
                None
            } else {
                Some(self.parse_expression(0)?)
            };
        let end = value
            .as_ref()
            .map_or(self.previous_span().end, |value| value.span().end);
        self.finish_line();
        Some(Statement::Return {
            value,
            span: Span::new(start, end),
        })
    }

    fn parse_require(&mut self) -> Option<Statement> {
        let start = self
            .expect_keyword(Keyword::Require, "expected `require`")?
            .span
            .start;
        let condition = self.parse_expression(0)?;
        self.expect_keyword(Keyword::Else, "expected `else` in require statement")?;
        let failure = self.parse_expression(0)?;
        let end = failure.span().end;
        self.finish_line();
        Some(Statement::Require {
            condition,
            failure,
            span: Span::new(start, end),
        })
    }

    fn parse_if(&mut self) -> Option<Statement> {
        let start = self.peek().span.start;
        let mut branches = Vec::new();
        let condition = self.parse_if_header(Keyword::If)?;
        let mut branch_end = condition.1;
        branches.push(IfBranch {
            condition: condition.0,
            body: condition.2,
            span: Span::new(start, branch_end),
        });
        while self.at_keyword(Keyword::Elif) {
            let elif_start = self.bump().span.start;
            let (elif_condition, end, body) = self.parse_if_tail(elif_start)?;
            branch_end = end;
            branches.push(IfBranch {
                condition: elif_condition,
                body,
                span: Span::new(elif_start, end),
            });
        }
        let otherwise = if self.at_keyword(Keyword::Else) {
            self.bump();
            self.expect_symbol(Symbol::Colon, "expected `:` after else")?;
            let body = self.parse_block()?;
            branch_end = self.previous_span().end;
            Some(body)
        } else {
            None
        };
        Some(Statement::If {
            branches,
            otherwise,
            span: Span::new(start, branch_end),
        })
    }

    fn parse_for(&mut self) -> Option<Statement> {
        let start = self
            .expect_keyword(Keyword::For, "expected `for`")?
            .span
            .start;
        let (name, _) = self.expect_identifier("expected a loop binding name")?;
        self.expect_keyword(Keyword::In, "expected `in` in for loop")?;
        let iterable = self.parse_expression(0)?;
        self.expect_symbol(Symbol::Colon, "expected `:` after for iterable")?;
        let body = self.parse_block()?;
        let end = self.previous_span().end;
        Some(Statement::For {
            name,
            iterable,
            body,
            span: Span::new(start, end),
        })
    }

    fn parse_if_header(&mut self, keyword: Keyword) -> Option<(Expr, usize, Vec<Statement>)> {
        let start = self
            .expect_keyword(keyword, "expected conditional keyword")?
            .span
            .start;
        self.parse_if_tail(start)
    }

    fn parse_if_tail(&mut self, _start: usize) -> Option<(Expr, usize, Vec<Statement>)> {
        let condition = self.parse_expression(0)?;
        self.expect_symbol(Symbol::Colon, "expected `:` after condition")?;
        let body = self.parse_block()?;
        let end = self.previous_span().end;
        Some((condition, end, body))
    }

    fn parse_match(&mut self) -> Option<Statement> {
        let start = self
            .expect_keyword(Keyword::Match, "expected `match`")?
            .span
            .start;
        let value = self.parse_expression(0)?;
        self.expect_symbol(Symbol::Colon, "expected `:` after match expression")?;
        self.expect_newline("expected a new line before match cases")?;
        self.expect_kind(TokenKind::Indent, "expected an indented match body")?;

        let mut cases = Vec::new();
        while !self.at(TokenKind::Dedent) && !self.at(TokenKind::Eof) {
            if self.consume_kind(TokenKind::Newline).is_some() {
                continue;
            }
            let case_start = self
                .expect_keyword(Keyword::Case, "expected `case` in match body")?
                .span
                .start;
            let pattern = self.parse_pattern()?;
            let guard = if self.consume_keyword(Keyword::If).is_some() {
                Some(self.parse_expression(0)?)
            } else {
                None
            };
            self.expect_symbol(Symbol::Colon, "expected `:` after match case")?;
            self.expect_newline("expected a new line before match case body")?;
            self.expect_kind(TokenKind::Indent, "expected an indented match case body")?;
            let body = self.parse_indented_statements()?;
            let end = self.previous_span().end;
            cases.push(MatchCase {
                pattern,
                guard,
                body,
                span: Span::new(case_start, end),
            });
        }
        let end = self
            .expect_kind(TokenKind::Dedent, "expected end of match body")?
            .span
            .end;
        Some(Statement::Match(MatchStatement {
            value,
            cases,
            span: Span::new(start, end),
        }))
    }

    fn parse_pattern(&mut self) -> Option<Pattern> {
        let token = self.bump();
        match token.kind {
            TokenKind::Keyword(Keyword::True) => Some(Pattern::Boolean {
                value: true,
                span: token.span,
            }),
            TokenKind::Keyword(Keyword::False) => Some(Pattern::Boolean {
                value: false,
                span: token.span,
            }),
            TokenKind::Keyword(Keyword::Null) => Some(Pattern::Null(token.span)),
            TokenKind::Integer(text) => Some(Pattern::Integer {
                text,
                span: token.span,
            }),
            TokenKind::StringLiteral(value) => Some(Pattern::String {
                value,
                span: token.span,
            }),
            TokenKind::Identifier(name) => {
                if name == "_" {
                    return Some(Pattern::Wildcard(token.span));
                }
                let mut path = vec![name];
                let mut end = token.span.end;
                while self.consume_symbol(Symbol::Dot).is_some() {
                    let (name, span) =
                        self.expect_identifier("expected a pattern name after `.`")?;
                    path.push(name);
                    end = span.end;
                }
                let mut arguments = Vec::new();
                if self.consume_symbol(Symbol::LeftParen).is_some() {
                    while !self.at_symbol(Symbol::RightParen) && !self.at(TokenKind::Eof) {
                        arguments.push(self.parse_pattern()?);
                        if self.consume_symbol(Symbol::Comma).is_none() {
                            break;
                        }
                    }
                    end = self
                        .expect_symbol(Symbol::RightParen, "expected `)` after pattern")?
                        .span
                        .end;
                }
                if path.len() > 1 || !arguments.is_empty() {
                    Some(Pattern::Constructor {
                        path,
                        arguments,
                        span: Span::new(token.span.start, end),
                    })
                } else {
                    Some(Pattern::Binding {
                        name: path.remove(0),
                        span: token.span,
                    })
                }
            }
            TokenKind::Reserved(name) => {
                self.error(
                    "SEL-P003",
                    format!("reserved syntax `{name}` is not implemented"),
                    token.span,
                );
                None
            }
            _ => {
                self.error("SEL-P001", "expected a match pattern", token.span);
                None
            }
        }
    }

    fn parse_block(&mut self) -> Option<Vec<Statement>> {
        self.expect_newline("expected a new line before block")?;
        self.expect_kind(TokenKind::Indent, "expected an indented block")?;
        self.parse_indented_statements()
    }

    fn parse_indented_statements(&mut self) -> Option<Vec<Statement>> {
        let mut statements = Vec::new();
        while !self.at(TokenKind::Dedent) && !self.at(TokenKind::Eof) {
            if self.consume_kind(TokenKind::Newline).is_some() {
                continue;
            }
            let before = self.cursor;
            if let Some(statement) = self.parse_statement() {
                statements.push(statement);
            }
            if before == self.cursor {
                self.synchronize_line();
            }
        }
        self.expect_kind(TokenKind::Dedent, "expected end of block")?;
        Some(statements)
    }

    fn parse_type(&mut self) -> Option<TypeExpr> {
        let start = self.peek().span.start;
        let mut type_expr = self.parse_type_atom()?;
        if self.consume_symbol(Symbol::LeftBracket).is_some() {
            let mut arguments = Vec::new();
            while !self.at_symbol(Symbol::RightBracket) && !self.at(TokenKind::Eof) {
                if let Some(argument) = self.parse_type() {
                    arguments.push(argument);
                }
                if self.consume_symbol(Symbol::Comma).is_none() {
                    break;
                }
            }
            let end = self
                .expect_symbol(Symbol::RightBracket, "expected `]` after type arguments")?
                .span
                .end;
            type_expr = TypeExpr {
                name: type_expr.name,
                arguments,
                span: Span::new(start, end),
            };
        }
        if self.consume_symbol(Symbol::Question).is_some() {
            self.error(
                "SEL-P003",
                "nullable type syntax `T?` is reserved; use `Option[T]`",
                self.previous_span(),
            );
        }
        Some(type_expr)
    }

    fn parse_type_atom(&mut self) -> Option<TypeExpr> {
        let token = self.bump();
        match token.kind {
            TokenKind::Identifier(name) => Some(TypeExpr {
                name,
                arguments: Vec::new(),
                span: token.span,
            }),
            TokenKind::Symbol(Symbol::LeftParen) => {
                let mut arguments = Vec::new();
                while !self.at_symbol(Symbol::RightParen) && !self.at(TokenKind::Eof) {
                    if let Some(argument) = self.parse_type() {
                        arguments.push(argument);
                    }
                    if self.consume_symbol(Symbol::Comma).is_none() {
                        break;
                    }
                }
                let end = self
                    .expect_symbol(Symbol::RightParen, "expected `)` after tuple type")?
                    .span
                    .end;
                Some(TypeExpr {
                    name: "tuple".to_owned(),
                    arguments,
                    span: Span::new(token.span.start, end),
                })
            }
            TokenKind::Reserved(name) => {
                self.error(
                    "SEL-P003",
                    format!("reserved syntax `{name}` is not implemented"),
                    token.span,
                );
                None
            }
            _ => {
                self.error("SEL-P001", "expected a type", token.span);
                None
            }
        }
    }

    fn parse_expression(&mut self, minimum_binding_power: u8) -> Option<Expr> {
        let mut left = self.parse_prefix()?;
        loop {
            if self.at_symbol(Symbol::LeftParen) {
                left = self.parse_call(left)?;
                continue;
            }
            if self.at_symbol(Symbol::Dot) || self.at_symbol(Symbol::QuestionDot) {
                let optional = self.consume_symbol(Symbol::QuestionDot).is_some();
                if !optional {
                    self.consume_symbol(Symbol::Dot);
                }
                let (name, name_span) = self.expect_identifier("expected member name after `.`")?;
                left = Expr::Member {
                    span: Span::new(left.span().start, name_span.end),
                    object: Box::new(left),
                    name,
                    optional,
                };
                continue;
            }
            if self.consume_symbol(Symbol::LeftBracket).is_some() {
                let index = self.parse_expression(0)?;
                let end = self
                    .expect_symbol(Symbol::RightBracket, "expected `]` after index")?
                    .span
                    .end;
                left = Expr::Index {
                    object: Box::new(left.clone()),
                    index: Box::new(index),
                    span: Span::new(left.span().start, end),
                };
                continue;
            }
            if self.consume_symbol(Symbol::Question).is_some() {
                let span = Span::new(left.span().start, self.previous_span().end);
                left = Expr::Propagate {
                    value: Box::new(left),
                    span,
                };
                continue;
            }

            let Some((operator, left_power, right_power)) = self.infix_binding_power() else {
                break;
            };
            if left_power < minimum_binding_power {
                break;
            }
            self.bump();
            let right = self.parse_expression(right_power)?;
            let span = Span::new(left.span().start, right.span().end);
            left = Expr::Binary {
                left: Box::new(left),
                operator,
                right: Box::new(right),
                span,
            };
        }
        Some(left)
    }

    fn parse_prefix(&mut self) -> Option<Expr> {
        let token = self.bump();
        match token.kind {
            TokenKind::Identifier(name) => Some(Expr::Name {
                name,
                span: token.span,
            }),
            TokenKind::Integer(text) => Some(Expr::Integer {
                text,
                span: token.span,
            }),
            TokenKind::Float(text) => Some(Expr::Float {
                text,
                span: token.span,
            }),
            TokenKind::StringLiteral(value) => Some(Expr::String {
                value,
                span: token.span,
            }),
            TokenKind::Keyword(Keyword::True) => Some(Expr::Boolean {
                value: true,
                span: token.span,
            }),
            TokenKind::Keyword(Keyword::False) => Some(Expr::Boolean {
                value: false,
                span: token.span,
            }),
            TokenKind::Keyword(Keyword::Null) => Some(Expr::Null(token.span)),
            TokenKind::Keyword(Keyword::Not) => {
                let value = self.parse_expression(60)?;
                Some(Expr::Unary {
                    operator: "not".to_owned(),
                    span: Span::new(token.span.start, value.span().end),
                    value: Box::new(value),
                })
            }
            TokenKind::Symbol(Symbol::Minus) | TokenKind::Symbol(Symbol::Bang) => {
                let operator = self.source[token.span.start..token.span.end].to_owned();
                let value = self.parse_expression(60)?;
                Some(Expr::Unary {
                    operator,
                    span: Span::new(token.span.start, value.span().end),
                    value: Box::new(value),
                })
            }
            TokenKind::Symbol(Symbol::LeftParen) => {
                let value = self.parse_expression(0)?;
                let end = self
                    .expect_symbol(Symbol::RightParen, "expected `)` after expression")?
                    .span
                    .end;
                Some(Expr::Group {
                    value: Box::new(value),
                    span: Span::new(token.span.start, end),
                })
            }
            TokenKind::Symbol(Symbol::LeftBracket) => {
                let mut values = Vec::new();
                while !self.at_symbol(Symbol::RightBracket) && !self.at(TokenKind::Eof) {
                    values.push(self.parse_expression(0)?);
                    if self.consume_symbol(Symbol::Comma).is_none() {
                        break;
                    }
                }
                let end = self
                    .expect_symbol(Symbol::RightBracket, "expected `]` after list")?
                    .span
                    .end;
                Some(Expr::List {
                    values,
                    span: Span::new(token.span.start, end),
                })
            }
            TokenKind::Reserved(name) => {
                self.error(
                    "SEL-P003",
                    format!("reserved syntax `{name}` is not implemented"),
                    token.span,
                );
                None
            }
            _ => {
                self.error("SEL-P001", "expected an expression", token.span);
                None
            }
        }
    }

    fn parse_call(&mut self, callee: Expr) -> Option<Expr> {
        self.expect_symbol(Symbol::LeftParen, "expected `(`")?;
        let mut arguments = Vec::new();
        while !self.at_symbol(Symbol::RightParen) && !self.at(TokenKind::Eof) {
            arguments.push(self.parse_expression(0)?);
            if self.consume_symbol(Symbol::Comma).is_none() {
                break;
            }
        }
        let end = self
            .expect_symbol(Symbol::RightParen, "expected `)` after call")?
            .span
            .end;
        Some(Expr::Call {
            callee: Box::new(callee.clone()),
            arguments,
            span: Span::new(callee.span().start, end),
        })
    }

    fn infix_binding_power(&self) -> Option<(String, u8, u8)> {
        let (operator, left, right) = match self.peek().kind {
            TokenKind::Symbol(Symbol::QuestionQuestion) => ("??", 10, 10),
            TokenKind::Symbol(Symbol::Pipe) => ("|>", 5, 6),
            TokenKind::Keyword(Keyword::Or) => ("or", 20, 21),
            TokenKind::Keyword(Keyword::And) => ("and", 30, 31),
            TokenKind::Symbol(Symbol::EqualEqual) => ("==", 40, 41),
            TokenKind::Symbol(Symbol::BangEqual) => ("!=", 40, 41),
            TokenKind::Symbol(Symbol::Less) => ("<", 40, 41),
            TokenKind::Symbol(Symbol::LessEqual) => ("<=", 40, 41),
            TokenKind::Symbol(Symbol::Greater) => (">", 40, 41),
            TokenKind::Symbol(Symbol::GreaterEqual) => (">=", 40, 41),
            TokenKind::Keyword(Keyword::In) => ("in", 40, 41),
            TokenKind::Keyword(Keyword::Is) => ("is", 40, 41),
            TokenKind::Symbol(Symbol::Plus) => ("+", 50, 51),
            TokenKind::Symbol(Symbol::Minus) => ("-", 50, 51),
            TokenKind::Symbol(Symbol::Star) => ("*", 60, 61),
            TokenKind::Symbol(Symbol::Slash) => ("/", 60, 61),
            TokenKind::Symbol(Symbol::Percent) => ("%", 60, 61),
            TokenKind::Symbol(Symbol::Power) => ("**", 70, 70),
            _ => return None,
        };
        Some((operator.to_owned(), left, right))
    }

    fn finish_line(&mut self) {
        if self.consume_kind(TokenKind::Newline).is_none()
            && !self.at(TokenKind::Dedent)
            && !self.at(TokenKind::Eof)
        {
            self.error("SEL-P002", "expected end of line", self.peek().span);
            self.synchronize_line();
        }
    }

    fn synchronize_line(&mut self) {
        while !self.at(TokenKind::Newline)
            && !self.at(TokenKind::Dedent)
            && !self.at(TokenKind::Eof)
        {
            self.bump();
        }
        self.consume_kind(TokenKind::Newline);
    }

    fn expect_identifier(&mut self, message: &str) -> Option<(String, Span)> {
        let token = self.bump();
        match token.kind {
            TokenKind::Identifier(name) => Some((name, token.span)),
            TokenKind::Reserved(name) => {
                self.error(
                    "SEL-P003",
                    format!("reserved syntax `{name}` is not implemented"),
                    token.span,
                );
                None
            }
            _ => {
                self.error("SEL-P001", message, token.span);
                None
            }
        }
    }

    fn expect_newline(&mut self, message: &str) -> Option<Token> {
        self.expect_kind(TokenKind::Newline, message)
    }

    fn expect_keyword(&mut self, keyword: Keyword, message: &str) -> Option<Token> {
        if self.at_keyword(keyword) {
            Some(self.bump())
        } else {
            self.error("SEL-P001", message, self.peek().span);
            None
        }
    }

    fn expect_symbol(&mut self, symbol: Symbol, message: &str) -> Option<Token> {
        if self.at_symbol(symbol) {
            Some(self.bump())
        } else {
            self.error("SEL-P001", message, self.peek().span);
            None
        }
    }

    fn expect_kind(&mut self, kind: TokenKind, message: &str) -> Option<Token> {
        if self.at(kind.clone()) {
            Some(self.bump())
        } else {
            self.error("SEL-P001", message, self.peek().span);
            None
        }
    }

    fn consume_keyword(&mut self, keyword: Keyword) -> Option<Token> {
        self.at_keyword(keyword).then(|| self.bump())
    }

    fn consume_symbol(&mut self, symbol: Symbol) -> Option<Token> {
        self.at_symbol(symbol).then(|| self.bump())
    }

    fn consume_kind(&mut self, kind: TokenKind) -> Option<Token> {
        self.at(kind).then(|| self.bump())
    }

    fn at_keyword(&self, keyword: Keyword) -> bool {
        self.peek().kind == TokenKind::Keyword(keyword)
    }

    fn at_symbol(&self, symbol: Symbol) -> bool {
        self.peek().kind == TokenKind::Symbol(symbol)
    }

    fn at(&self, kind: TokenKind) -> bool {
        self.peek().kind == kind
    }

    fn bump_text(&mut self) -> String {
        let token = self.bump();
        self.source[token.span.start..token.span.end].to_owned()
    }

    fn bump(&mut self) -> Token {
        let token = self.tokens.get(self.cursor).cloned().unwrap_or(Token {
            kind: TokenKind::Eof,
            span: Span::new(self.source.len(), self.source.len()),
        });
        self.cursor = self.cursor.saturating_add(1);
        token
    }

    fn peek(&self) -> &Token {
        self.tokens
            .get(self.cursor)
            .unwrap_or_else(|| self.tokens.last().expect("lexer always emits EOF"))
    }

    fn previous_span(&self) -> Span {
        self.tokens
            .get(self.cursor.saturating_sub(1))
            .map_or(Span::new(0, 0), |token| token.span)
    }

    fn error(&mut self, code: &'static str, message: impl Into<String>, span: Span) {
        self.diagnostics.push(Diagnostic::new(code, message, span));
    }
}

impl Expr {
    pub(crate) fn span(&self) -> Span {
        match self {
            Self::Null(span)
            | Self::Name { span, .. }
            | Self::Integer { span, .. }
            | Self::Float { span, .. }
            | Self::String { span, .. }
            | Self::Boolean { span, .. }
            | Self::List { span, .. }
            | Self::Group { span, .. }
            | Self::Unary { span, .. }
            | Self::Binary { span, .. }
            | Self::Call { span, .. }
            | Self::Index { span, .. }
            | Self::Member { span, .. }
            | Self::Propagate { span, .. } => *span,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Expr, Item, Statement, parse};

    #[test]
    fn parses_function_blocks_and_keeps_byte_spans() {
        let source = "fn main() -> Unit:\r\n    let value = -2 ** 2\r\n    return value\r\n";
        let result = parse(source);
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        let program = result.program.expect("program should parse");
        assert_eq!(program.span.start, 0);
        assert_eq!(program.span.end, source.len());
        let Item::Function(function) = &program.items[0] else {
            panic!("expected function")
        };
        let Statement::Binding(binding) = &function.body[0] else {
            panic!("expected binding")
        };
        assert!(matches!(binding.value, Expr::Unary { .. }));
        assert_eq!(&source[function.span.start..function.span.end], source);
    }

    #[test]
    fn power_binds_inside_unary_expression() {
        let result = parse("let value = -2 ** 2\n");
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        let Item::Binding(binding) = &result.program.expect("program").items[0] else {
            panic!("expected binding")
        };
        let Expr::Unary { value, .. } = &binding.value else {
            panic!("expected unary expression")
        };
        assert!(matches!(value.as_ref(), Expr::Binary { .. }));
    }

    #[test]
    fn reserved_syntax_has_a_parser_diagnostic() {
        let result = parse("class Example:\n");
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "SEL-P003")
        );
    }

    #[test]
    fn preserves_e3_precedence_and_match_ast() {
        let result = parse("let value = left ?? fallback |> normalize\n");
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        let Item::Binding(binding) = &result.program.expect("program").items[0] else {
            panic!("expected binding")
        };
        let Expr::Binary {
            operator,
            left,
            right,
            ..
        } = &binding.value
        else {
            panic!("expected pipeline expression")
        };
        assert_eq!(operator, "|>");
        assert!(matches!(left.as_ref(), Expr::Binary { operator, .. } if operator == "??"));
        assert!(matches!(right.as_ref(), Expr::Name { name, .. } if name == "normalize"));

        let match_source = "pub fn choose(flag: Bool) -> Unit:\n    match flag:\n        case true:\n            return\n        case false:\n            return\n";
        let result = parse(match_source);
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        let Item::Function(function) = &result.program.expect("program").items[0] else {
            panic!("expected function")
        };
        assert!(function.public);
        assert!(matches!(function.body[0], Statement::Match(_)));
    }

    #[test]
    fn nullable_type_syntax_is_reserved_explicitly() {
        let result = parse("fn bad(value: I64?) -> I64:\n    return value\n");
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "SEL-P003")
        );
    }

    #[test]
    fn parses_declarations_directives_and_control_flow() {
        let source = r#"module demo
import core as c
record User:
    name: Text
enum ResultKind:
    Ok(Text)
    Empty
fn main(input: I64) -> Result[Text, Error]:
    let items: List[I64] = [1, 2]
    if input > 0:
        require input > 1 else "small"
        return "ok"
    elif input == 0:
        return
    else:
        return "no"
"#;
        let result = parse(source);
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        let program = result.program.expect("program should parse");
        assert_eq!(program.items.len(), 5);

        let Item::Record(record) = &program.items[2] else {
            panic!("expected record")
        };
        assert_eq!(record.fields.len(), 1);

        let Item::Enum(enum_decl) = &program.items[3] else {
            panic!("expected enum")
        };
        assert_eq!(enum_decl.variants.len(), 2);
        assert_eq!(enum_decl.variants[0].fields.len(), 1);

        let Item::Function(function) = &program.items[4] else {
            panic!("expected function")
        };
        assert_eq!(function.parameters[0].name, "input");
        assert_eq!(function.body.len(), 2);
        assert!(matches!(function.body[1], Statement::If { .. }));
    }

    #[test]
    fn malformed_function_reports_a_stable_span() {
        let result = parse("fn broken(\n");
        let diagnostic = result
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == "SEL-P001")
            .expect("parser diagnostic");
        assert_eq!(diagnostic.span.start, 10);
    }
}
