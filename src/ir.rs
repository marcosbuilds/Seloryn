//! Deterministic core representation used between semantic checking and execution.
//!
//! The parser owns the source-facing AST. This module owns the execution-facing
//! representation so the runtime does not interpret parser nodes directly. The
//! lowering is deliberately structural at this stage: it preserves source spans,
//! explicit annotations and evaluation order, while the semantic checker remains
//! the type gate before lowering.

use crate::diagnostic::Span;
use crate::parser;

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
    NamedArgument {
        name: String,
        value: Box<Expr>,
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

pub fn lower(program: &parser::Program) -> Program {
    Program {
        items: program.items.iter().map(lower_item).collect(),
        span: program.span,
    }
}

fn lower_item(item: &parser::Item) -> Item {
    match item {
        parser::Item::Function(function) => Item::Function(Function {
            public: function.public,
            name: function.name.clone(),
            parameters: function.parameters.iter().map(lower_parameter).collect(),
            return_type: function.return_type.as_ref().map(lower_type),
            body: lower_statements(&function.body),
            span: function.span,
        }),
        parser::Item::Binding(binding) => Item::Binding(lower_binding(binding)),
        parser::Item::Record(record) => Item::Record(Record {
            public: record.public,
            name: record.name.clone(),
            fields: record.fields.iter().map(lower_field).collect(),
            span: record.span,
        }),
        parser::Item::Enum(enum_decl) => Item::Enum(EnumDecl {
            public: enum_decl.public,
            name: enum_decl.name.clone(),
            variants: enum_decl.variants.iter().map(lower_variant).collect(),
            span: enum_decl.span,
        }),
        parser::Item::Directive(directive) => Item::Directive(Directive {
            keyword: directive.keyword.clone(),
            text: directive.text.clone(),
            span: directive.span,
        }),
        parser::Item::Statement(statement) => Item::Statement(lower_statement(statement)),
    }
}

fn lower_parameter(parameter: &parser::Parameter) -> Parameter {
    Parameter {
        name: parameter.name.clone(),
        type_expr: parameter.type_expr.as_ref().map(lower_type),
        span: parameter.span,
    }
}

fn lower_field(field: &parser::Field) -> Field {
    Field {
        name: field.name.clone(),
        type_expr: lower_type(&field.type_expr),
        span: field.span,
    }
}

fn lower_variant(variant: &parser::Variant) -> Variant {
    Variant {
        name: variant.name.clone(),
        fields: variant.fields.iter().map(lower_type).collect(),
        span: variant.span,
    }
}

fn lower_type(type_expr: &parser::TypeExpr) -> TypeExpr {
    TypeExpr {
        name: type_expr.name.clone(),
        arguments: type_expr.arguments.iter().map(lower_type).collect(),
        span: type_expr.span,
    }
}

fn lower_binding(binding: &parser::Binding) -> Binding {
    Binding {
        mutable: binding.mutable,
        name: binding.name.clone(),
        annotation: binding.annotation.as_ref().map(lower_type),
        value: lower_expr(&binding.value),
        span: binding.span,
    }
}

fn lower_statements(statements: &[parser::Statement]) -> Vec<Statement> {
    statements.iter().map(lower_statement).collect()
}

fn lower_statement(statement: &parser::Statement) -> Statement {
    match statement {
        parser::Statement::Binding(binding) => Statement::Binding(lower_binding(binding)),
        parser::Statement::Assignment { name, value, span } => Statement::Assignment {
            name: name.clone(),
            value: lower_expr(value),
            span: *span,
        },
        parser::Statement::Return { value, span } => Statement::Return {
            value: value.as_ref().map(lower_expr),
            span: *span,
        },
        parser::Statement::Require {
            condition,
            failure,
            span,
        } => Statement::Require {
            condition: lower_expr(condition),
            failure: lower_expr(failure),
            span: *span,
        },
        parser::Statement::If {
            branches,
            otherwise,
            span,
        } => Statement::If {
            branches: branches.iter().map(lower_branch).collect(),
            otherwise: otherwise.as_ref().map(|body| lower_statements(body)),
            span: *span,
        },
        parser::Statement::Match(statement) => Statement::Match(MatchStatement {
            value: lower_expr(&statement.value),
            cases: statement.cases.iter().map(lower_case).collect(),
            span: statement.span,
        }),
        parser::Statement::For {
            name,
            iterable,
            body,
            span,
        } => Statement::For {
            name: name.clone(),
            iterable: lower_expr(iterable),
            body: lower_statements(body),
            span: *span,
        },
        parser::Statement::Expression(expression) => Statement::Expression(lower_expr(expression)),
    }
}

fn lower_branch(branch: &parser::IfBranch) -> IfBranch {
    IfBranch {
        condition: lower_expr(&branch.condition),
        body: lower_statements(&branch.body),
        span: branch.span,
    }
}

fn lower_case(case: &parser::MatchCase) -> MatchCase {
    MatchCase {
        pattern: lower_pattern(&case.pattern),
        guard: case.guard.as_ref().map(lower_expr),
        body: lower_statements(&case.body),
        span: case.span,
    }
}

fn lower_pattern(pattern: &parser::Pattern) -> Pattern {
    match pattern {
        parser::Pattern::Wildcard(span) => Pattern::Wildcard(*span),
        parser::Pattern::Binding { name, span } => Pattern::Binding {
            name: name.clone(),
            span: *span,
        },
        parser::Pattern::Boolean { value, span } => Pattern::Boolean {
            value: *value,
            span: *span,
        },
        parser::Pattern::Integer { text, span } => Pattern::Integer {
            text: text.clone(),
            span: *span,
        },
        parser::Pattern::String { value, span } => Pattern::String {
            value: value.clone(),
            span: *span,
        },
        parser::Pattern::Null(span) => Pattern::Null(*span),
        parser::Pattern::Constructor {
            path,
            arguments,
            span,
        } => Pattern::Constructor {
            path: path.clone(),
            arguments: arguments.iter().map(lower_pattern).collect(),
            span: *span,
        },
    }
}

fn lower_expr(expression: &parser::Expr) -> Expr {
    match expression {
        parser::Expr::Name { name, span } => Expr::Name {
            name: name.clone(),
            span: *span,
        },
        parser::Expr::Integer { text, span } => Expr::Integer {
            text: text.clone(),
            span: *span,
        },
        parser::Expr::Float { text, span } => Expr::Float {
            text: text.clone(),
            span: *span,
        },
        parser::Expr::String { value, span } => Expr::String {
            value: value.clone(),
            span: *span,
        },
        parser::Expr::Boolean { value, span } => Expr::Boolean {
            value: *value,
            span: *span,
        },
        parser::Expr::Null(span) => Expr::Null(*span),
        parser::Expr::List { values, span } => Expr::List {
            values: values.iter().map(lower_expr).collect(),
            span: *span,
        },
        parser::Expr::Group { value, span } => Expr::Group {
            value: Box::new(lower_expr(value)),
            span: *span,
        },
        parser::Expr::Unary {
            operator,
            value,
            span,
        } => Expr::Unary {
            operator: operator.clone(),
            value: Box::new(lower_expr(value)),
            span: *span,
        },
        parser::Expr::Binary {
            left,
            operator,
            right,
            span,
        } => Expr::Binary {
            left: Box::new(lower_expr(left)),
            operator: operator.clone(),
            right: Box::new(lower_expr(right)),
            span: *span,
        },
        parser::Expr::Call {
            callee,
            arguments,
            span,
        } => Expr::Call {
            callee: Box::new(lower_expr(callee)),
            arguments: arguments.iter().map(lower_expr).collect(),
            span: *span,
        },
        parser::Expr::NamedArgument { name, value, span } => Expr::NamedArgument {
            name: name.clone(),
            value: Box::new(lower_expr(value)),
            span: *span,
        },
        parser::Expr::Index {
            object,
            index,
            span,
        } => Expr::Index {
            object: Box::new(lower_expr(object)),
            index: Box::new(lower_expr(index)),
            span: *span,
        },
        parser::Expr::Member {
            object,
            name,
            optional,
            span,
        } => Expr::Member {
            object: Box::new(lower_expr(object)),
            name: name.clone(),
            optional: *optional,
            span: *span,
        },
        parser::Expr::Propagate { value, span } => Expr::Propagate {
            value: Box::new(lower_expr(value)),
            span: *span,
        },
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
            | Self::NamedArgument { span, .. }
            | Self::Index { span, .. }
            | Self::Member { span, .. }
            | Self::Propagate { span, .. } => *span,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::lower;
    use crate::parser;

    #[test]
    fn lowering_preserves_order_and_spans_without_parser_nodes() {
        let source = "fn main() -> I64:\n    return 7\n";
        let program = parser::parse(source).program.expect("valid program");
        let ir = lower(&program);
        assert_eq!(ir.span, program.span);
        assert_eq!(ir.items.len(), 1);
        assert!(format!("{ir:?}").contains("Integer"));
    }

    #[test]
    fn identical_sources_lower_to_identical_ir() {
        let source = "fn main() -> I64:\n    return 7\n";
        let left = parser::parse(source).program.expect("valid program");
        let right = parser::parse(source).program.expect("valid program");
        assert_eq!(lower(&left), lower(&right));
    }
}
