use crate::builtins::{self, Builtin};
use crate::diagnostic::{Diagnostic, Span};
use crate::parser::{
    Binding, EnumDecl, Expr, Function, IfBranch, Item, Pattern, Program, Record, Statement,
};
use crate::tensor::{Tensor, TensorError};
use std::cmp::Ordering;
use std::collections::HashMap;

const MAX_CALL_DEPTH: usize = 128;
const MAX_LOOP_ITERATIONS: usize = 1_000_000;

/// Values that can be produced by the pure E4 interpreter.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Unit,
    Bool(bool),
    I64(i64),
    F64(f64),
    Text(String),
    List(Vec<Value>),
    Tensor(Tensor),
    Record {
        name: String,
        fields: Vec<(String, Value)>,
    },
    Enum {
        name: String,
        variant: String,
        fields: Vec<Value>,
    },
    Option(Option<Box<Value>>),
    Result {
        ok: bool,
        value: Box<Value>,
    },
    Function(String),
}

impl Value {
    pub fn render(&self) -> String {
        match self {
            Self::Unit => "unit".to_owned(),
            Self::Bool(value) => value.to_string(),
            Self::I64(value) => value.to_string(),
            Self::F64(value) => format_f64(*value),
            Self::Text(value) => format!("\"{}\"", escape_text(value)),
            Self::List(values) => format!(
                "[{}]",
                values
                    .iter()
                    .map(Self::render)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Tensor(tensor) => tensor.render(),
            Self::Record { name, fields } => format!(
                "{name}({})",
                fields
                    .iter()
                    .map(|(field, value)| format!("{field}: {}", value.render()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Enum {
                name,
                variant,
                fields,
            } => {
                if fields.is_empty() {
                    format!("{name}.{variant}")
                } else {
                    format!(
                        "{name}.{variant}({})",
                        fields
                            .iter()
                            .map(Self::render)
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
            }
            Self::Option(Some(value)) => format!("Some({})", value.render()),
            Self::Option(None) => "None".to_owned(),
            Self::Result { ok: true, value } => format!("Ok({})", value.render()),
            Self::Result { ok: false, value } => format!("Err({})", value.render()),
            Self::Function(name) => format!("<fn {name}>").to_owned(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeError {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
    pub stack: Vec<String>,
}

impl RuntimeError {
    fn new(code: &'static str, message: impl Into<String>, span: Span, stack: &[String]) -> Self {
        Self {
            code,
            message: message.into(),
            span,
            stack: stack.to_vec(),
        }
    }

    pub fn diagnostic(&self) -> Diagnostic {
        Diagnostic::new(self.code, self.message.clone(), self.span)
    }
}

/// Execute a semantically valid program without performing external effects.
pub fn execute(program: &Program) -> Result<Value, RuntimeError> {
    Interpreter::new(program).execute()
}

#[derive(Clone, Debug)]
enum Control {
    Return(Value),
    Runtime(RuntimeError),
}

type Eval<T> = Result<T, Control>;

struct Interpreter<'a> {
    program: &'a Program,
    functions: HashMap<String, Function>,
    records: HashMap<String, Record>,
    enums: HashMap<String, EnumDecl>,
    globals: HashMap<String, Value>,
    scopes: Vec<HashMap<String, Value>>,
    call_stack: Vec<String>,
}

impl<'a> Interpreter<'a> {
    fn new(program: &'a Program) -> Self {
        let mut functions = HashMap::new();
        let mut records = HashMap::new();
        let mut enums = HashMap::new();
        for item in &program.items {
            match item {
                Item::Function(function) => {
                    functions.insert(function.name.clone(), function.clone());
                }
                Item::Record(record) => {
                    records.insert(record.name.clone(), record.clone());
                }
                Item::Enum(enum_decl) => {
                    enums.insert(enum_decl.name.clone(), enum_decl.clone());
                }
                Item::Binding(_) | Item::Directive(_) | Item::Statement(_) => {}
            }
        }
        Self {
            program,
            functions,
            records,
            enums,
            globals: HashMap::new(),
            scopes: Vec::new(),
            call_stack: Vec::new(),
        }
    }

    fn execute(mut self) -> Result<Value, RuntimeError> {
        for item in &self.program.items {
            match item {
                Item::Binding(binding) => {
                    let value = self
                        .eval_expr(&binding.value)
                        .map_err(Self::runtime_error)?;
                    self.globals.insert(binding.name.clone(), value);
                }
                Item::Statement(statement) => {
                    self.eval_statement(statement)
                        .map_err(Self::runtime_error)?;
                }
                Item::Function(_) | Item::Record(_) | Item::Enum(_) | Item::Directive(_) => {}
            }
        }

        if !self.functions.contains_key("main") {
            return Err(self.error(
                "SEL-R101",
                "program has no `main` function",
                self.program.span,
            ));
        }
        self.call_named("main", Vec::new(), self.program.span)
            .map_err(Self::runtime_error)
    }

    fn call_named(&mut self, name: &str, arguments: Vec<Value>, span: Span) -> Eval<Value> {
        let Some(function) = self.functions.get(name).cloned() else {
            return Err(Control::Runtime(self.runtime(
                "SEL-R102",
                format!("cannot call undefined function `{name}`"),
                span,
            )));
        };
        if function.parameters.len() != arguments.len() {
            return Err(Control::Runtime(self.runtime(
                "SEL-R103",
                format!(
                    "function `{name}` expects {} argument(s), got {}",
                    function.parameters.len(),
                    arguments.len()
                ),
                span,
            )));
        }
        if self.call_stack.len() >= MAX_CALL_DEPTH {
            return Err(Control::Runtime(self.runtime(
                "SEL-R132",
                format!("call depth exceeds maximum {MAX_CALL_DEPTH}"),
                span,
            )));
        }

        self.call_stack.push(name.to_owned());
        let mut frame = HashMap::new();
        for (parameter, argument) in function.parameters.iter().zip(arguments) {
            frame.insert(parameter.name.clone(), argument);
        }
        self.scopes.push(frame);
        let result = self.eval_statements(&function.body);
        self.scopes.pop();
        self.call_stack.pop();

        match result {
            Ok(()) => Ok(Value::Unit),
            Err(Control::Return(value)) => Ok(value),
            Err(Control::Runtime(error)) => Err(Control::Runtime(error)),
        }
    }

    fn eval_statements(&mut self, statements: &[Statement]) -> Eval<()> {
        for statement in statements {
            self.eval_statement(statement)?;
        }
        Ok(())
    }

    fn eval_statement(&mut self, statement: &Statement) -> Eval<()> {
        match statement {
            Statement::Binding(binding) => {
                let value = self.eval_expr(&binding.value)?;
                self.bind(binding, value)
            }
            Statement::Assignment { name, value, span } => {
                let value = self.eval_expr(value)?;
                self.assign(name, value, *span)
            }
            Statement::Return { value, .. } => {
                let value = value
                    .as_ref()
                    .map_or(Ok(Value::Unit), |expression| self.eval_expr(expression))?;
                Err(Control::Return(value))
            }
            Statement::Require {
                condition,
                failure,
                span,
            } => {
                let condition = self.eval_bool(condition)?;
                if condition {
                    return Ok(());
                }
                let failure = self.eval_expr(failure)?;
                let result = Value::Result {
                    ok: false,
                    value: Box::new(failure),
                };
                let _ = span;
                Err(Control::Return(result))
            }
            Statement::If {
                branches,
                otherwise,
                ..
            } => self.eval_if(branches, otherwise.as_deref()),
            Statement::Match(statement) => self.eval_match(statement),
            Statement::For {
                name,
                iterable,
                body,
                span,
            } => self.eval_for(name, iterable, body, *span),
            Statement::Expression(expression) => {
                self.eval_expr(expression)?;
                Ok(())
            }
        }
    }

    fn eval_if(&mut self, branches: &[IfBranch], otherwise: Option<&[Statement]>) -> Eval<()> {
        for branch in branches {
            if self.eval_bool(&branch.condition)? {
                return self.eval_scoped(&branch.body);
            }
        }
        otherwise.map_or(Ok(()), |body| self.eval_scoped(body))
    }

    fn eval_match(&mut self, statement: &crate::parser::MatchStatement) -> Eval<()> {
        let value = self.eval_expr(&statement.value)?;
        for case in &statement.cases {
            self.scopes.push(HashMap::new());
            let mut bindings = Vec::new();
            let matched = self.pattern_matches(&case.pattern, &value, &mut bindings)?;
            if matched {
                for (name, bound) in bindings {
                    let Some(scope) = self.scopes.last_mut() else {
                        return Err(Control::Runtime(self.runtime(
                            "SEL-R127",
                            "interpreter lost the match scope",
                            case.span,
                        )));
                    };
                    scope.insert(name, bound);
                }
                let guard_matches = case
                    .guard
                    .as_ref()
                    .map_or(Ok(true), |guard| self.eval_bool(guard));
                if guard_matches? {
                    let result = self.eval_statements(&case.body);
                    self.scopes.pop();
                    return result;
                }
            }
            self.scopes.pop();
        }
        Err(Control::Runtime(self.runtime(
            "SEL-R104",
            "no match case accepted the runtime value",
            statement.span,
        )))
    }

    fn eval_scoped(&mut self, statements: &[Statement]) -> Eval<()> {
        self.scopes.push(HashMap::new());
        let result = self.eval_statements(statements);
        self.scopes.pop();
        result
    }

    fn eval_for(
        &mut self,
        name: &str,
        iterable: &Expr,
        body: &[Statement],
        span: Span,
    ) -> Eval<()> {
        let value = self.eval_expr(iterable)?;
        let Value::List(values) = value else {
            return Err(Control::Runtime(self.runtime(
                "SEL-R128",
                "for iteration requires a List value",
                span,
            )));
        };
        if values.len() > MAX_LOOP_ITERATIONS {
            return Err(Control::Runtime(self.runtime(
                "SEL-R131",
                format!(
                    "for loop has {} iterations; maximum is {MAX_LOOP_ITERATIONS}",
                    values.len()
                ),
                span,
            )));
        }
        for value in values {
            self.scopes.push(HashMap::from([(name.to_owned(), value)]));
            let result = self.eval_statements(body);
            self.scopes.pop();
            result?;
        }
        Ok(())
    }

    fn pattern_matches(
        &mut self,
        pattern: &Pattern,
        value: &Value,
        bindings: &mut Vec<(String, Value)>,
    ) -> Eval<bool> {
        match pattern {
            Pattern::Wildcard(_) => Ok(true),
            Pattern::Binding { name, .. } => {
                if self.is_constructor_binding(name, value) {
                    Ok(true)
                } else {
                    bindings.push((name.clone(), value.clone()));
                    Ok(true)
                }
            }
            Pattern::Boolean {
                value: expected, ..
            } => Ok(matches!(value, Value::Bool(actual) if actual == expected)),
            Pattern::Integer { text, span } => {
                let expected = self.parse_i64(text, *span)?;
                Ok(matches!(value, Value::I64(actual) if *actual == expected))
            }
            Pattern::String {
                value: expected, ..
            } => Ok(matches!(value, Value::Text(actual) if actual == expected)),
            Pattern::Null(_) => Ok(matches!(value, Value::Option(None))),
            Pattern::Constructor {
                path,
                arguments,
                span,
            } => self.constructor_matches(path, arguments, *span, value, bindings),
        }
    }

    fn constructor_matches(
        &mut self,
        path: &[String],
        patterns: &[Pattern],
        span: Span,
        value: &Value,
        bindings: &mut Vec<(String, Value)>,
    ) -> Eval<bool> {
        let Some(name) = path.last() else {
            return Ok(false);
        };
        match value {
            Value::Option(Some(inner)) if name == "Some" && patterns.len() == 1 => {
                self.pattern_matches(&patterns[0], inner, bindings)
            }
            Value::Option(None) if name == "None" && patterns.is_empty() => Ok(true),
            Value::Result { ok: true, value } if name == "Ok" && patterns.len() == 1 => {
                self.pattern_matches(&patterns[0], value, bindings)
            }
            Value::Result { ok: false, value } if name == "Err" && patterns.len() == 1 => {
                self.pattern_matches(&patterns[0], value, bindings)
            }
            Value::Enum {
                name: enum_name,
                variant,
                fields,
            } if variant == name
                && fields.len() == patterns.len()
                && (path.len() == 1 || path.first() == Some(enum_name)) =>
            {
                for (pattern, field) in patterns.iter().zip(fields) {
                    if !self.pattern_matches(pattern, field, bindings)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            _ => {
                let _ = span;
                Ok(false)
            }
        }
    }

    fn is_constructor_binding(&self, name: &str, value: &Value) -> bool {
        matches!(value, Value::Enum { variant, fields, .. } if variant == name && fields.is_empty())
            || matches!(value, Value::Option(None) if name == "None")
    }

    fn eval_expr(&mut self, expression: &Expr) -> Eval<Value> {
        match expression {
            Expr::Name { name, .. } if name == "None" => Ok(Value::Option(None)),
            Expr::Name { name, span } => self.lookup(name).ok_or_else(|| {
                Control::Runtime(self.runtime(
                    "SEL-R105",
                    format!("runtime name `{name}` is unavailable"),
                    *span,
                ))
            }),
            Expr::Integer { text, span } => self.parse_i64(text, *span).map(Value::I64),
            Expr::Float { text, span } => self.parse_f64(text, *span).map(Value::F64),
            Expr::String { value, .. } => Ok(Value::Text(value.clone())),
            Expr::Boolean { value, .. } => Ok(Value::Bool(*value)),
            Expr::Null(_) => Ok(Value::Option(None)),
            Expr::List { values, .. } => {
                let values = values
                    .iter()
                    .map(|value| self.eval_expr(value))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Value::List(values))
            }
            Expr::Group { value, .. } => self.eval_expr(value),
            Expr::Unary {
                operator,
                value,
                span,
            } => {
                let value = self.eval_expr(value)?;
                match (operator.as_str(), value) {
                    ("not" | "!", Value::Bool(value)) => Ok(Value::Bool(!value)),
                    ("-", Value::I64(value)) => {
                        value.checked_neg().map(Value::I64).ok_or_else(|| {
                            Control::Runtime(self.runtime(
                                "SEL-R106",
                                "integer negation overflowed",
                                *span,
                            ))
                        })
                    }
                    ("-", Value::F64(value)) => finite_f64(
                        -value,
                        "floating negation produced a non-finite value",
                        *span,
                        self,
                    ),
                    (operator, _) => Err(Control::Runtime(self.runtime(
                        "SEL-R107",
                        format!("operator `{operator}` received an incompatible value"),
                        *span,
                    ))),
                }
            }
            Expr::Binary {
                left,
                operator,
                right,
                span,
            } => self.eval_binary(left, operator, right, *span),
            Expr::Call {
                callee,
                arguments,
                span,
            } => self.eval_call(callee, arguments, *span),
            Expr::Index {
                object,
                index,
                span,
            } => self.eval_index(object, index, *span),
            Expr::Member {
                object,
                name,
                optional: false,
                span,
            } => self.eval_member(object, name, *span),
            Expr::Member {
                optional: true,
                span,
                ..
            } => Err(Control::Runtime(self.runtime(
                "SEL-R108",
                "optional member access is unavailable in E4",
                *span,
            ))),
            Expr::Propagate { value, span } => {
                let value = self.eval_expr(value)?;
                match value {
                    Value::Result { ok: true, value } => Ok(*value),
                    Value::Result { ok: false, value } => {
                        Err(Control::Return(Value::Result { ok: false, value }))
                    }
                    _ => Err(Control::Runtime(self.runtime(
                        "SEL-R109",
                        "postfix `?` received a non-Result value",
                        *span,
                    ))),
                }
            }
        }
    }

    fn eval_binary(
        &mut self,
        left: &Expr,
        operator: &str,
        right: &Expr,
        span: Span,
    ) -> Eval<Value> {
        if operator == "|>" {
            let input = self.eval_expr(left)?;
            let Expr::Name { name, .. } = right else {
                return Err(Control::Runtime(self.runtime(
                    "SEL-R110",
                    "pipeline RHS must be a named function",
                    span,
                )));
            };
            return self.call_named(name, vec![input], right.span());
        }

        let left = self.eval_expr(left)?;
        if operator == "and" || operator == "or" {
            let left = self.expect_bool(left, span)?;
            if (operator == "and" && !left) || (operator == "or" && left) {
                return Ok(Value::Bool(left));
            }
            let right_span = right.span();
            let right_value = self.eval_expr(right)?;
            let right = self.expect_bool(right_value, right_span)?;
            return Ok(Value::Bool(right));
        }
        if operator == "??" {
            return match left {
                Value::Option(Some(value)) => Ok(*value),
                Value::Option(None) => self.eval_expr(right),
                _ => Err(Control::Runtime(self.runtime(
                    "SEL-R111",
                    "`??` requires an Option value on the left",
                    span,
                ))),
            };
        }

        let right = self.eval_expr(right)?;
        match operator {
            "+" => self.add_values(left, right, span),
            "-" | "*" | "/" | "%" | "**" => self.numeric_values(operator, left, right, span),
            "==" => Ok(Value::Bool(left == right)),
            "!=" => Ok(Value::Bool(left != right)),
            "<" | "<=" | ">" | ">=" => self.compare_values(operator, left, right, span),
            _ => Err(Control::Runtime(self.runtime(
                "SEL-R112",
                format!("operator `{operator}` is unavailable in E4"),
                span,
            ))),
        }
    }

    fn eval_call(&mut self, callee: &Expr, arguments: &[Expr], span: Span) -> Eval<Value> {
        if let Expr::Name { name, .. } = callee {
            if let Some(builtin) = builtins::lookup(name) {
                return self.eval_builtin(builtin, arguments, span);
            }
            if matches!(name.as_str(), "Ok" | "Err" | "Some") {
                if arguments.len() != 1 {
                    return Err(Control::Runtime(self.runtime(
                        "SEL-R103",
                        format!("`{name}` expects one argument"),
                        span,
                    )));
                }
                let value = self.eval_expr(&arguments[0])?;
                let value = match name.as_str() {
                    "Ok" => Value::Result {
                        ok: true,
                        value: Box::new(value),
                    },
                    "Err" => Value::Result {
                        ok: false,
                        value: Box::new(value),
                    },
                    "Some" => Value::Option(Some(Box::new(value))),
                    _ => {
                        return Err(Control::Runtime(self.runtime(
                            "SEL-R103",
                            "unknown value constructor",
                            span,
                        )));
                    }
                };
                return Ok(value);
            }
            if self.records.contains_key(name) {
                return self.construct_record(name, arguments, span);
            }
        }
        if let Expr::Member { object, name, .. } = callee
            && let Expr::Name {
                name: enum_name, ..
            } = object.as_ref()
            && self.enums.contains_key(enum_name)
        {
            return self.construct_enum(enum_name, name, arguments, span);
        }
        let callee = self.eval_expr(callee)?;
        let arguments = arguments
            .iter()
            .map(|argument| self.eval_expr(argument))
            .collect::<Result<Vec<_>, _>>()?;
        match callee {
            Value::Function(name) => self.call_named(&name, arguments, span),
            _ => Err(Control::Runtime(self.runtime(
                "SEL-R113",
                "expression is not callable",
                span,
            ))),
        }
    }

    fn eval_builtin(&mut self, builtin: Builtin, arguments: &[Expr], span: Span) -> Eval<Value> {
        let expected = match builtin {
            Builtin::TensorF32 | Builtin::TensorI64 | Builtin::Reshape | Builtin::Matmul => 2,
            Builtin::ZerosF32 | Builtin::ZerosI64 => 1,
            Builtin::TensorShape
            | Builtin::TensorRank
            | Builtin::TensorElements
            | Builtin::TensorDtype => 1,
            Builtin::Sum | Builtin::Mean => 0,
        };
        let valid_arity = if matches!(builtin, Builtin::Sum | Builtin::Mean) {
            arguments.len() == 1 || arguments.len() == 2
        } else {
            arguments.len() == expected
        };
        if !valid_arity {
            let expected_description = if matches!(builtin, Builtin::Sum | Builtin::Mean) {
                "1 or 2".to_owned()
            } else {
                expected.to_string()
            };
            return Err(Control::Runtime(self.runtime(
                "SEL-R103",
                format!(
                    "builtin expects {} argument(s), got {}",
                    expected_description,
                    arguments.len()
                ),
                span,
            )));
        }
        let values = arguments
            .iter()
            .map(|argument| self.eval_expr(argument))
            .collect::<Result<Vec<_>, _>>()?;
        match builtin {
            Builtin::TensorF32 => {
                let source = list_values(&values[0], span, self)?;
                let shape = shape_values(&values[1], span, self)?;
                let values = source
                    .iter()
                    .map(|value| match value {
                        Value::F64(value) => {
                            let converted = *value as f32;
                            converted.is_finite().then_some(converted).ok_or_else(|| {
                                Control::Runtime(self.tensor_error(TensorError::NonFinite, span))
                            })
                        }
                        _ => Err(Control::Runtime(self.tensor_error(
                            TensorError::DtypeMismatch {
                                expected: "F64",
                                actual: value_type_name(value),
                            },
                            span,
                        ))),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Tensor::from_f32(values, &shape)
                    .map(Value::Tensor)
                    .map_err(|error| Control::Runtime(self.tensor_error(error, span)))
            }
            Builtin::TensorI64 => {
                let source = list_values(&values[0], span, self)?;
                let shape = shape_values(&values[1], span, self)?;
                let values = source
                    .iter()
                    .map(|value| match value {
                        Value::I64(value) => Ok(*value),
                        _ => Err(Control::Runtime(self.tensor_error(
                            TensorError::DtypeMismatch {
                                expected: "I64",
                                actual: value_type_name(value),
                            },
                            span,
                        ))),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Tensor::from_i64(values, &shape)
                    .map(Value::Tensor)
                    .map_err(|error| Control::Runtime(self.tensor_error(error, span)))
            }
            Builtin::ZerosF32 => {
                let shape = shape_values(&values[0], span, self)?;
                Tensor::zeros_f32(&shape)
                    .map(Value::Tensor)
                    .map_err(|error| Control::Runtime(self.tensor_error(error, span)))
            }
            Builtin::ZerosI64 => {
                let shape = shape_values(&values[0], span, self)?;
                Tensor::zeros_i64(&shape)
                    .map(Value::Tensor)
                    .map_err(|error| Control::Runtime(self.tensor_error(error, span)))
            }
            Builtin::TensorShape => {
                let tensor = tensor_value(&values[0], span, self)?;
                let shape = tensor
                    .shape()
                    .iter()
                    .map(|dimension| {
                        i64::try_from(*dimension).map(Value::I64).map_err(|_| {
                            Control::Runtime(self.tensor_error(TensorError::ShapeOverflow, span))
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Value::List(shape))
            }
            Builtin::TensorRank => {
                let tensor = tensor_value(&values[0], span, self)?;
                Ok(Value::I64(i64::try_from(tensor.rank()).map_err(|_| {
                    Control::Runtime(self.tensor_error(TensorError::ShapeOverflow, span))
                })?))
            }
            Builtin::TensorElements => {
                let tensor = tensor_value(&values[0], span, self)?;
                Ok(Value::I64(i64::try_from(tensor.element_count()).map_err(
                    |_| Control::Runtime(self.tensor_error(TensorError::ShapeOverflow, span)),
                )?))
            }
            Builtin::TensorDtype => {
                let tensor = tensor_value(&values[0], span, self)?;
                Ok(Value::Text(tensor.dtype().name().to_owned()))
            }
            Builtin::Reshape => {
                let tensor = tensor_value(&values[0], span, self)?;
                let shape = shape_values(&values[1], span, self)?;
                tensor
                    .reshape(&shape)
                    .map(Value::Tensor)
                    .map_err(|error| Control::Runtime(self.tensor_error(error, span)))
            }
            Builtin::Sum => {
                let tensor = tensor_value(&values[0], span, self)?;
                let axis = values
                    .get(1)
                    .map(|value| self.reduction_axis(value, tensor.rank(), span))
                    .transpose()?;
                let result = match axis {
                    Some(axis) => tensor.sum_axis(Some(axis)),
                    None => tensor.sum_all(),
                };
                result
                    .map(Value::Tensor)
                    .map_err(|error| Control::Runtime(self.tensor_error(error, span)))
            }
            Builtin::Mean => {
                let tensor = tensor_value(&values[0], span, self)?;
                let axis = values
                    .get(1)
                    .map(|value| self.reduction_axis(value, tensor.rank(), span))
                    .transpose()?;
                let result = match axis {
                    Some(axis) => tensor.mean_axis(Some(axis)),
                    None => tensor.mean_all(),
                };
                result
                    .map(Value::Tensor)
                    .map_err(|error| Control::Runtime(self.tensor_error(error, span)))
            }
            Builtin::Matmul => {
                let left = tensor_value(&values[0], span, self)?;
                let right = tensor_value(&values[1], span, self)?;
                left.matmul(right)
                    .map(Value::Tensor)
                    .map_err(|error| Control::Runtime(self.tensor_error(error, span)))
            }
        }
    }

    fn construct_record(&mut self, name: &str, arguments: &[Expr], span: Span) -> Eval<Value> {
        let fields = self
            .records
            .get(name)
            .map(|record| record.fields.clone())
            .unwrap_or_default();
        if fields.len() != arguments.len() {
            return Err(Control::Runtime(self.runtime(
                "SEL-R103",
                format!("record `{name}` expects {} argument(s)", fields.len()),
                span,
            )));
        }
        let values = arguments
            .iter()
            .map(|argument| self.eval_expr(argument))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Value::Record {
            name: name.to_owned(),
            fields: fields
                .into_iter()
                .zip(values)
                .map(|(field, value)| (field.name, value))
                .collect(),
        })
    }

    fn construct_enum(
        &mut self,
        enum_name: &str,
        variant_name: &str,
        arguments: &[Expr],
        span: Span,
    ) -> Eval<Value> {
        let fields = self
            .enums
            .get(enum_name)
            .and_then(|enum_decl| {
                enum_decl
                    .variants
                    .iter()
                    .find(|variant| variant.name == variant_name)
            })
            .map(|variant| variant.fields.clone())
            .ok_or_else(|| {
                Control::Runtime(self.runtime(
                    "SEL-R114",
                    format!("unknown enum constructor `{enum_name}.{variant_name}`"),
                    span,
                ))
            })?;
        if fields.len() != arguments.len() {
            return Err(Control::Runtime(self.runtime(
                "SEL-R103",
                format!(
                    "variant `{variant_name}` expects {} argument(s)",
                    fields.len()
                ),
                span,
            )));
        }
        let values = arguments
            .iter()
            .map(|argument| self.eval_expr(argument))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Value::Enum {
            name: enum_name.to_owned(),
            variant: variant_name.to_owned(),
            fields: values,
        })
    }

    fn eval_member(&mut self, object: &Expr, name: &str, span: Span) -> Eval<Value> {
        if let Expr::Name {
            name: enum_name, ..
        } = object
            && self.enums.contains_key(enum_name)
        {
            let has_variant = self.enums.get(enum_name).is_some_and(|enum_decl| {
                enum_decl
                    .variants
                    .iter()
                    .any(|variant| variant.name == name && variant.fields.is_empty())
            });
            if has_variant {
                return Ok(Value::Enum {
                    name: enum_name.clone(),
                    variant: name.to_owned(),
                    fields: Vec::new(),
                });
            }
        }
        let object = self.eval_expr(object)?;
        if let Value::Record {
            name: record_name,
            fields,
        } = object
        {
            if let Some((_, value)) = fields.into_iter().find(|(field, _)| field == name) {
                return Ok(value);
            }
            return Err(Control::Runtime(self.runtime(
                "SEL-R115",
                format!("record `{record_name}` has no field `{name}`"),
                span,
            )));
        }
        Err(Control::Runtime(self.runtime(
            "SEL-R115",
            format!("cannot access member `{name}`"),
            span,
        )))
    }

    fn eval_index(&mut self, object: &Expr, index: &Expr, span: Span) -> Eval<Value> {
        let object = self.eval_expr(object)?;
        let index = self.eval_expr(index)?;
        let Value::List(values) = object else {
            return Err(Control::Runtime(self.runtime(
                "SEL-R128",
                "indexing requires a List value",
                span,
            )));
        };
        let Value::I64(index) = index else {
            return Err(Control::Runtime(self.runtime(
                "SEL-R128",
                "list index requires an I64 value",
                span,
            )));
        };
        let Ok(index) = usize::try_from(index) else {
            return Err(Control::Runtime(self.runtime(
                "SEL-R129",
                format!("list index {index} is outside the valid range"),
                span,
            )));
        };
        values.get(index).cloned().ok_or_else(|| {
            Control::Runtime(self.runtime(
                "SEL-R129",
                format!(
                    "list index {index} is out of bounds for length {}",
                    values.len()
                ),
                span,
            ))
        })
    }

    fn bind(&mut self, binding: &Binding, value: Value) -> Eval<()> {
        let Some(scope) = self.scopes.last_mut() else {
            return Err(Control::Runtime(self.runtime(
                "SEL-R127",
                "interpreter lost the binding scope",
                binding.span,
            )));
        };
        scope.insert(binding.name.clone(), value);
        Ok(())
    }

    fn assign(&mut self, name: &str, value: Value, span: Span) -> Eval<()> {
        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_owned(), value);
                return Ok(());
            }
        }
        if self.globals.contains_key(name) {
            self.globals.insert(name.to_owned(), value);
            return Ok(());
        }
        Err(Control::Runtime(self.runtime(
            "SEL-R130",
            format!("cannot assign undefined name `{name}`"),
            span,
        )))
    }

    fn lookup(&self, name: &str) -> Option<Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(value) = scope.get(name) {
                return Some(value.clone());
            }
        }
        self.globals.get(name).cloned().or_else(|| {
            self.functions
                .contains_key(name)
                .then(|| Value::Function(name.to_owned()))
        })
    }

    fn eval_bool(&mut self, expression: &Expr) -> Eval<bool> {
        let value = self.eval_expr(expression)?;
        self.expect_bool(value, expression.span())
    }

    fn expect_bool(&self, value: Value, span: Span) -> Eval<bool> {
        match value {
            Value::Bool(value) => Ok(value),
            _ => Err(Control::Runtime(self.runtime(
                "SEL-R116",
                "condition requires a Bool value",
                span,
            ))),
        }
    }

    fn add_values(&mut self, left: Value, right: Value, span: Span) -> Eval<Value> {
        match (left, right) {
            (Value::Text(left), Value::Text(right)) => Ok(Value::Text(left + &right)),
            (Value::List(mut left), Value::List(right)) => {
                left.extend(right);
                Ok(Value::List(left))
            }
            (Value::I64(left), Value::I64(right)) => {
                left.checked_add(right).map(Value::I64).ok_or_else(|| {
                    Control::Runtime(self.runtime("SEL-R106", "integer addition overflowed", span))
                })
            }
            (Value::F64(left), Value::F64(right)) => finite_f64(
                left + right,
                "floating addition produced a non-finite value",
                span,
                self,
            ),
            (Value::Tensor(left), Value::Tensor(right)) => left
                .elementwise(&right, "+")
                .map(Value::Tensor)
                .map_err(|error| Control::Runtime(self.tensor_error(error, span))),
            _ => Err(Control::Runtime(self.runtime(
                "SEL-R117",
                "`+` received incompatible values",
                span,
            ))),
        }
    }

    fn numeric_values(
        &mut self,
        operator: &str,
        left: Value,
        right: Value,
        span: Span,
    ) -> Eval<Value> {
        match (left, right) {
            (Value::Tensor(left), Value::Tensor(right)) => left
                .elementwise(&right, operator)
                .map(Value::Tensor)
                .map_err(|error| Control::Runtime(self.tensor_error(error, span))),
            (Value::I64(left), Value::I64(right)) => {
                let value = match operator {
                    "-" => left.checked_sub(right),
                    "*" => left.checked_mul(right),
                    "/" => {
                        if right == 0 {
                            return Err(Control::Runtime(self.runtime(
                                "SEL-R118",
                                "integer division by zero",
                                span,
                            )));
                        }
                        left.checked_div(right)
                    }
                    "%" => {
                        if right == 0 {
                            return Err(Control::Runtime(self.runtime(
                                "SEL-R119",
                                "integer remainder by zero",
                                span,
                            )));
                        }
                        left.checked_rem(right)
                    }
                    "**" => {
                        if right < 0 || right > u32::MAX as i64 {
                            return Err(Control::Runtime(self.runtime(
                                "SEL-R120",
                                "integer exponent must be between zero and u32::MAX",
                                span,
                            )));
                        }
                        left.checked_pow(right as u32)
                    }
                    _ => None,
                };
                value.map(Value::I64).ok_or_else(|| {
                    Control::Runtime(self.runtime(
                        "SEL-R106",
                        "integer arithmetic overflowed",
                        span,
                    ))
                })
            }
            (Value::F64(left), Value::F64(right)) => {
                if (operator == "/" || operator == "%") && right == 0.0 {
                    return Err(Control::Runtime(self.runtime(
                        "SEL-R121",
                        "floating division by zero",
                        span,
                    )));
                }
                let value = match operator {
                    "-" => left - right,
                    "*" => left * right,
                    "/" => left / right,
                    "%" => left % right,
                    "**" => left.powf(right),
                    _ => {
                        return Err(Control::Runtime(self.runtime(
                            "SEL-R112",
                            "unknown numeric operator",
                            span,
                        )));
                    }
                };
                finite_f64(
                    value,
                    "floating arithmetic produced a non-finite value",
                    span,
                    self,
                )
            }
            _ => Err(Control::Runtime(self.runtime(
                "SEL-R117",
                format!("numeric operator `{operator}` received incompatible values"),
                span,
            ))),
        }
    }

    fn compare_values(
        &mut self,
        operator: &str,
        left: Value,
        right: Value,
        span: Span,
    ) -> Eval<Value> {
        let ordering = match (left, right) {
            (Value::I64(left), Value::I64(right)) => left.cmp(&right),
            (Value::F64(left), Value::F64(right)) => left.partial_cmp(&right).ok_or_else(|| {
                Control::Runtime(self.runtime("SEL-R122", "cannot order a non-finite F64", span))
            })?,
            (Value::Text(left), Value::Text(right)) => left.cmp(&right),
            _ => {
                return Err(Control::Runtime(self.runtime(
                    "SEL-R123",
                    "ordering requires matching I64, F64 or Text values",
                    span,
                )));
            }
        };
        let result = match operator {
            "<" => ordering == Ordering::Less,
            "<=" => ordering != Ordering::Greater,
            ">" => ordering == Ordering::Greater,
            ">=" => ordering != Ordering::Less,
            _ => false,
        };
        Ok(Value::Bool(result))
    }

    fn parse_i64(&self, text: &str, span: Span) -> Eval<i64> {
        text.replace('_', "").parse::<i64>().map_err(|_| {
            Control::Runtime(self.runtime(
                "SEL-R124",
                format!("integer literal `{text}` is outside I64 range"),
                span,
            ))
        })
    }

    fn parse_f64(&self, text: &str, span: Span) -> Eval<f64> {
        let value = text.replace('_', "").parse::<f64>().map_err(|_| {
            Control::Runtime(self.runtime(
                "SEL-R125",
                format!("invalid F64 literal `{text}`"),
                span,
            ))
        })?;
        if value.is_finite() {
            Ok(value)
        } else {
            Err(Control::Runtime(self.runtime(
                "SEL-R125",
                format!("F64 literal `{text}` is not finite"),
                span,
            )))
        }
    }

    fn runtime(&self, code: &'static str, message: impl Into<String>, span: Span) -> RuntimeError {
        RuntimeError::new(code, message, span, &self.call_stack)
    }

    fn tensor_error(&self, error: TensorError, span: Span) -> RuntimeError {
        self.runtime(error.code(), error.to_string(), span)
    }

    fn reduction_axis(&self, value: &Value, rank: usize, span: Span) -> Eval<usize> {
        let Value::I64(axis) = value else {
            return Err(Control::Runtime(self.runtime(
                "SEL-R205",
                format!(
                    "reduction axis requires I64, got {}",
                    value_type_name(value)
                ),
                span,
            )));
        };
        let Ok(axis) = usize::try_from(*axis) else {
            return Err(Control::Runtime(self.runtime(
                "SEL-R205",
                format!("reduction axis {axis} is invalid for rank {rank}"),
                span,
            )));
        };
        if axis >= rank {
            return Err(Control::Runtime(self.runtime(
                "SEL-R205",
                format!("reduction axis {axis} is invalid for rank {rank}"),
                span,
            )));
        }
        Ok(axis)
    }

    fn error(&self, code: &'static str, message: impl Into<String>, span: Span) -> RuntimeError {
        self.runtime(code, message, span)
    }

    fn runtime_error(control: Control) -> RuntimeError {
        match control {
            Control::Runtime(error) => error,
            Control::Return(_) => RuntimeError {
                code: "SEL-R126",
                message: "a return escaped the program boundary".to_owned(),
                span: Span::new(0, 0),
                stack: Vec::new(),
            },
        }
    }
}

fn finite_f64(value: f64, message: &str, span: Span, interpreter: &Interpreter<'_>) -> Eval<Value> {
    if value.is_finite() {
        Ok(Value::F64(value))
    } else {
        Err(Control::Runtime(
            interpreter.runtime("SEL-R125", message, span),
        ))
    }
}

fn list_values<'a>(
    value: &'a Value,
    span: Span,
    interpreter: &Interpreter<'_>,
) -> Eval<&'a [Value]> {
    match value {
        Value::List(values) => Ok(values),
        _ => Err(Control::Runtime(interpreter.runtime(
            "SEL-R203",
            format!(
                "expected List[I64] or List[F64], got {}",
                value_type_name(value)
            ),
            span,
        ))),
    }
}

fn shape_values(value: &Value, span: Span, interpreter: &Interpreter<'_>) -> Eval<Vec<i64>> {
    list_values(value, span, interpreter)?
        .iter()
        .map(|value| match value {
            Value::I64(value) => Ok(*value),
            _ => Err(Control::Runtime(interpreter.runtime(
                "SEL-R203",
                format!(
                    "tensor shape requires I64 dimensions, got {}",
                    value_type_name(value)
                ),
                span,
            ))),
        })
        .collect()
}

fn tensor_value<'a>(
    value: &'a Value,
    span: Span,
    interpreter: &Interpreter<'_>,
) -> Eval<&'a Tensor> {
    match value {
        Value::Tensor(tensor) => Ok(tensor),
        _ => Err(Control::Runtime(interpreter.runtime(
            "SEL-R203",
            format!("expected a tensor, got {}", value_type_name(value)),
            span,
        ))),
    }
}

fn value_type_name(value: &Value) -> &'static str {
    match value {
        Value::Unit => "Unit",
        Value::Bool(_) => "Bool",
        Value::I64(_) => "I64",
        Value::F64(_) => "F64",
        Value::Text(_) => "Text",
        Value::List(_) => "List",
        Value::Tensor(_) => "Tensor",
        Value::Record { .. } => "Record",
        Value::Enum { .. } => "Enum",
        Value::Option(_) => "Option",
        Value::Result { .. } => "Result",
        Value::Function(_) => "Function",
    }
}

fn format_f64(value: f64) -> String {
    let rendered = value.to_string();
    if rendered.contains('.') || rendered.contains('e') || rendered.contains('E') {
        rendered
    } else {
        format!("{rendered}.0")
    }
}

fn escape_text(value: &str) -> String {
    let mut escaped = String::new();
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character.is_control() => {
                escaped.push_str(&format!("\\u{{{:x}}}", character as u32));
            }
            character => escaped.push(character),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::{RuntimeError, Value, execute};
    use crate::parser::parse;
    use crate::semantic::check;

    fn run(source: &str) -> Result<Value, RuntimeError> {
        let parsed = parse(source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let program = parsed.program.expect("program");
        let diagnostics = check(&program);
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        execute(&program)
    }

    #[test]
    fn executes_scalars_conditionals_and_recursion() {
        let value = run(
            "fn factorial(value: I64) -> I64:\n    if value <= 1:\n        return 1\n    else:\n        return value * factorial(value - 1)\nfn main() -> I64:\n    return factorial(5)\n",
        )
        .expect("execution should succeed");
        assert_eq!(value, Value::I64(120));
    }

    #[test]
    fn executes_f64_text_bool_and_unit_values() {
        let value = run(
            "fn halve(value: F64) -> F64:\n    return value / 2.0\nfn main() -> F64:\n    return 7.0 |> halve\n",
        )
        .expect("F64 execution should succeed");
        assert_eq!(value.render(), "3.5");

        let unit =
            run("fn main() -> Unit:\n    if true:\n        return\n    else:\n        return\n")
                .expect("Unit execution should succeed");
        assert_eq!(unit, Value::Unit);
    }

    #[test]
    fn executes_lists_records_enums_and_pipeline() {
        let value = run(
            "record Box:\n    value: I64\nenum Signal:\n    Ready\n    Value(I64)\nfn add_one(value: I64) -> I64:\n    return value + 1\nfn read(signal: Signal) -> I64:\n    match signal:\n        case Signal.Value(number):\n            return number\n        case Signal.Ready:\n            return 0\nfn main() -> List[List[I64]]:\n    let values: List[I64] = [1, 2, 3]\n    let box: Box = Box(4)\n    let answer: I64 = box.value |> add_one\n    let signal: Signal = Signal.Value(4)\n    let variant_answer: I64 = read(signal)\n    return [values, [answer, variant_answer]]\n",
        )
        .expect("execution should succeed");
        assert_eq!(value.render(), "[[1, 2, 3], [5, 4]]");
    }

    #[test]
    fn executes_list_indexing_iteration_assignment_and_concat() {
        let indexed = run(
            "fn main() -> List[I64]:\n    let values: List[I64] = [1, 2]\n    return values + [values[0]]\n",
        )
        .expect("list indexing should execute");
        assert_eq!(indexed.render(), "[1, 2, 1]");

        let total = run(
            "fn main() -> I64:\n    var total: I64 = 0\n    for value in [1, 2, 3]:\n        total = total + value\n    return total\n",
        )
        .expect("list iteration should execute");
        assert_eq!(total, Value::I64(6));
    }

    #[test]
    fn executes_option_result_require_match_and_propagation() {
        let value = run(
            "fn validate(value: I64) -> Result[I64, Text]:\n    require value > 0 else \"bad\"\n    return Ok(value)\nfn main() -> Result[I64, Text]:\n    let maybe: Option[I64] = None\n    let fallback: I64 = maybe ?? 7\n    let result: Result[I64, Text] = validate(fallback)\n    match result:\n        case Ok(value):\n            return Ok(value)\n        case Err(error):\n            return Err(error)\n",
        )
        .expect("execution should succeed");
        assert_eq!(value.render(), "Ok(7)");

        let error = run(
            "fn validate() -> Result[I64, Text]:\n    require false else \"bad\"\n    return Ok(1)\nfn wrapper() -> Result[I64, Text]:\n    return Ok(validate()?)\nfn main() -> Result[I64, Text]:\n    return wrapper()\n",
        )
        .expect("a Result error is a normal value");
        assert_eq!(error.render(), "Err(\"bad\")");
    }

    #[test]
    fn reports_deterministic_runtime_errors_with_stack() {
        let error = run("fn main() -> I64:\n    return 1 / 0\n").expect_err("division must fail");
        assert_eq!(error.code, "SEL-R118");
        assert_eq!(error.stack, vec!["main"]);
        assert_eq!(error.span.start, 29);
    }

    #[test]
    fn reports_bounds_and_call_depth_without_panicking() {
        let bounds = run("fn main() -> I64:\n    return [1][2]\n").expect_err("bounds must fail");
        assert_eq!(bounds.code, "SEL-R129");

        let depth = run(
            "fn recurse(value: I64) -> I64:\n    return recurse(value)\nfn main() -> I64:\n    return recurse(0)\n",
        )
        .expect_err("call depth must fail");
        assert_eq!(depth.code, "SEL-R132");
    }
}
