use crate::builtins::{self, Builtin};
use crate::diagnostic::{Diagnostic, Span};
use crate::parser::{
    Binding, Expr, Function, IfBranch, Item, MatchStatement, Pattern, Program, Statement, TypeExpr,
};
use crate::tensor::TensorDtype;
use std::collections::{HashMap, HashSet, hash_map::Entry};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Type {
    Unit,
    Bool,
    I64,
    F64,
    F32,
    Text,
    List(Box<Type>),
    Option(Box<Type>),
    Result(Box<Type>, Box<Type>),
    Tensor(TensorDtype),
    Json,
    JsonError,
    JsonIoError,
    ModelRequest,
    ModelResponse,
    ModelError,
    CheckpointError,
    Record(String),
    Enum(String),
    Function(Vec<Type>, Box<Type>),
    Unknown,
    Error,
}

#[derive(Clone, Debug)]
struct RecordDef {
    fields: Vec<(String, Type, Span)>,
}

#[derive(Clone, Debug)]
struct VariantDef {
    name: String,
    fields: Vec<(Type, Span)>,
}

#[derive(Clone, Debug)]
struct EnumDef {
    variants: Vec<VariantDef>,
}

#[derive(Clone, Debug)]
enum Symbol {
    Value { ty: Type, mutable: bool },
    Function(FunctionSignature),
}

#[derive(Clone, Debug)]
struct FunctionSignature {
    parameters: Vec<Type>,
    return_type: Type,
}

#[derive(Default)]
struct Scope {
    values: HashMap<String, Symbol>,
}

struct FunctionContext {
    return_type: Type,
}

#[derive(Clone)]
enum Coverage {
    Wildcard,
    Keys(Vec<String>),
    Owned,
}

pub fn check(program: &Program) -> Vec<Diagnostic> {
    Checker::new(program).run()
}

struct Checker<'a> {
    program: &'a Program,
    records: HashMap<String, RecordDef>,
    enums: HashMap<String, EnumDef>,
    globals: HashMap<String, Symbol>,
    scopes: Vec<Scope>,
    current_function: Option<FunctionContext>,
    io_print_imported: bool,
    stdlib_imports: HashSet<String>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Checker<'a> {
    fn new(program: &'a Program) -> Self {
        Self {
            program,
            records: HashMap::new(),
            enums: HashMap::new(),
            globals: HashMap::new(),
            scopes: Vec::new(),
            current_function: None,
            io_print_imported: false,
            stdlib_imports: HashSet::new(),
            diagnostics: Vec::new(),
        }
    }

    fn run(mut self) -> Vec<Diagnostic> {
        self.collect_declarations();
        self.collect_supported_imports();
        self.check_items();
        self.diagnostics
    }

    fn collect_supported_imports(&mut self) {
        for item in &self.program.items {
            let Item::Directive(directive) = item else {
                continue;
            };
            if directive.keyword == "from" && is_supported_standard_directive(&directive.text) {
                let mut parts = directive.text.split_whitespace();
                let Some(module) = parts.next() else {
                    continue;
                };
                let _ = parts.next();
                for symbol in parts {
                    self.stdlib_imports
                        .insert(symbol.trim_end_matches(',').to_owned());
                }
                if module == "io" && self.stdlib_imports.contains("print") {
                    self.io_print_imported = true;
                }
            }
        }
    }

    fn collect_declarations(&mut self) {
        for item in &self.program.items {
            match item {
                Item::Record(record) => {
                    if self.records.contains_key(&record.name)
                        || self.enums.contains_key(&record.name)
                        || self.globals.contains_key(&record.name)
                    {
                        self.error(
                            "SEL-M003",
                            format!("duplicate declaration `{}`", record.name),
                            record.span,
                        );
                    } else {
                        self.records
                            .insert(record.name.clone(), RecordDef { fields: Vec::new() });
                    }
                }
                Item::Enum(enum_decl) => {
                    if self.records.contains_key(&enum_decl.name)
                        || self.enums.contains_key(&enum_decl.name)
                        || self.globals.contains_key(&enum_decl.name)
                    {
                        self.error(
                            "SEL-M003",
                            format!("duplicate declaration `{}`", enum_decl.name),
                            enum_decl.span,
                        );
                    } else {
                        self.enums.insert(
                            enum_decl.name.clone(),
                            EnumDef {
                                variants: Vec::new(),
                            },
                        );
                    }
                }
                _ => {}
            }
        }

        for item in &self.program.items {
            if let Item::Record(record) = item
                && self.records.contains_key(&record.name)
            {
                let mut names = HashSet::new();
                let mut fields = Vec::new();
                for field in &record.fields {
                    if !names.insert(field.name.clone()) {
                        self.error(
                            "SEL-M003",
                            format!("duplicate field `{}`", field.name),
                            field.span,
                        );
                    }
                    let ty = self.type_from_expr(&field.type_expr);
                    fields.push((field.name.clone(), ty, field.span));
                }
                if let Some(def) = self.records.get_mut(&record.name) {
                    def.fields = fields;
                }
            }
            if let Item::Enum(enum_decl) = item
                && self.enums.contains_key(&enum_decl.name)
            {
                let mut names = HashSet::new();
                let mut variants = Vec::new();
                for variant in &enum_decl.variants {
                    if !names.insert(variant.name.clone()) {
                        self.error(
                            "SEL-M003",
                            format!("duplicate enum variant `{}`", variant.name),
                            variant.span,
                        );
                    }
                    let fields = variant
                        .fields
                        .iter()
                        .map(|ty| (self.type_from_expr(ty), ty.span))
                        .collect();
                    variants.push(VariantDef {
                        name: variant.name.clone(),
                        fields,
                    });
                }
                if let Some(def) = self.enums.get_mut(&enum_decl.name) {
                    def.variants = variants;
                }
            }
        }

        for item in &self.program.items {
            if let Item::Function(function) = item {
                if builtins::lookup(&function.name).is_some()
                    || self.globals.contains_key(&function.name)
                    || self.records.contains_key(&function.name)
                    || self.enums.contains_key(&function.name)
                {
                    self.error(
                        "SEL-M003",
                        format!("duplicate declaration `{}`", function.name),
                        function.span,
                    );
                    continue;
                }
                if function.public
                    && (function
                        .parameters
                        .iter()
                        .any(|parameter| parameter.type_expr.is_none())
                        || function.return_type.is_none())
                {
                    self.error(
                        "SEL-T001",
                        format!(
                            "public function `{}` requires an explicit signature",
                            function.name
                        ),
                        function.span,
                    );
                }
                let parameters = function
                    .parameters
                    .iter()
                    .map(|parameter| {
                        parameter
                            .type_expr
                            .as_ref()
                            .map_or(Type::Unknown, |ty| self.type_from_expr(ty))
                    })
                    .collect();
                let return_type = function
                    .return_type
                    .as_ref()
                    .map_or(Type::Unit, |ty| self.type_from_expr(ty));
                self.globals.insert(
                    function.name.clone(),
                    Symbol::Function(FunctionSignature {
                        parameters,
                        return_type,
                    }),
                );
            }
        }
    }

    fn check_items(&mut self) {
        for item in &self.program.items {
            match item {
                Item::Function(function) => self.check_function(function),
                Item::Binding(binding) => self.check_top_binding(binding),
                Item::Directive(directive) => {
                    let supported = directive.keyword == "from"
                        && is_supported_standard_directive(&directive.text);
                    if !supported {
                        self.error(
                            "SEL-M001",
                            format!("{} resolution is not implemented in E3", directive.keyword),
                            directive.span,
                        );
                    }
                }
                Item::Statement(statement) => self.check_statement(statement),
                Item::Record(_) | Item::Enum(_) => {}
            }
        }
    }

    fn check_function(&mut self, function: &Function) {
        let Some(Symbol::Function(signature)) = self.globals.get(&function.name).cloned() else {
            return;
        };
        self.current_function = Some(FunctionContext {
            return_type: signature.return_type.clone(),
        });
        self.scopes.push(Scope::default());
        for (index, parameter) in function.parameters.iter().enumerate() {
            let ty = signature
                .parameters
                .get(index)
                .cloned()
                .unwrap_or(Type::Unknown);
            self.declare_local(
                parameter.name.clone(),
                Symbol::Value { ty, mutable: false },
                parameter.span,
            );
        }
        for statement in &function.body {
            self.check_statement(statement);
        }
        self.scopes.pop();
        self.current_function = None;
    }

    fn check_top_binding(&mut self, binding: &Binding) {
        if builtins::lookup(&binding.name).is_some()
            || self.globals.contains_key(&binding.name)
            || self.records.contains_key(&binding.name)
            || self.enums.contains_key(&binding.name)
        {
            self.error(
                "SEL-M003",
                format!("duplicate declaration `{}`", binding.name),
                binding.span,
            );
            return;
        }
        let actual = self.type_of_expr(&binding.value);
        let expected = binding
            .annotation
            .as_ref()
            .map_or_else(|| actual.clone(), |ty| self.type_from_expr(ty));
        if binding.annotation.is_some() {
            self.ensure_annotated_assignable(
                &actual,
                &expected,
                binding.value.span(),
                "binding value",
            );
        } else {
            self.ensure_assignable(&actual, &expected, binding.value.span(), "binding value");
        }
        self.globals.insert(
            binding.name.clone(),
            Symbol::Value {
                ty: expected,
                mutable: binding.mutable,
            },
        );
    }

    fn check_statement(&mut self, statement: &Statement) {
        match statement {
            Statement::Binding(binding) => self.check_binding(binding),
            Statement::Assignment { name, value, span } => {
                self.check_assignment(name, value, *span)
            }
            Statement::Return { value, span } => self.check_return(value.as_ref(), *span),
            Statement::Require {
                condition,
                failure,
                span,
            } => self.check_require(condition, failure, *span),
            Statement::If {
                branches,
                otherwise,
                ..
            } => {
                for branch in branches {
                    self.check_if_branch(branch);
                }
                if let Some(body) = otherwise {
                    self.scopes.push(Scope::default());
                    for statement in body {
                        self.check_statement(statement);
                    }
                    self.scopes.pop();
                }
            }
            Statement::Match(statement) => self.check_match(statement),
            Statement::For {
                name,
                iterable,
                body,
                span,
            } => self.check_for(name, iterable, body, *span),
            Statement::Expression(expression) => {
                self.type_of_expr(expression);
            }
        }
    }

    fn check_binding(&mut self, binding: &Binding) {
        let actual = self.type_of_expr(&binding.value);
        let expected = binding
            .annotation
            .as_ref()
            .map_or_else(|| actual.clone(), |ty| self.type_from_expr(ty));
        if binding.annotation.is_some() {
            self.ensure_annotated_assignable(
                &actual,
                &expected,
                binding.value.span(),
                "binding value",
            );
        } else {
            self.ensure_assignable(&actual, &expected, binding.value.span(), "binding value");
        }
        self.declare_local(
            binding.name.clone(),
            Symbol::Value {
                ty: expected,
                mutable: binding.mutable,
            },
            binding.span,
        );
    }

    fn check_assignment(&mut self, name: &str, value: &Expr, span: Span) {
        let Some(symbol) = self.lookup(name) else {
            self.error("SEL-M002", format!("undefined name `{name}`"), span);
            self.type_of_expr(value);
            return;
        };
        let Symbol::Value { ty, mutable } = symbol else {
            self.error(
                "SEL-T003",
                format!("function `{name}` cannot be assigned"),
                span,
            );
            self.type_of_expr(value);
            return;
        };
        if !mutable {
            self.error(
                "SEL-T003",
                format!("binding `{name}` is immutable; use `var` for reassignment"),
                span,
            );
        }
        let actual = self.type_of_expr(value);
        self.ensure_assignable(&actual, &ty, value.span(), "assignment value");
    }

    fn check_for(&mut self, name: &str, iterable: &Expr, body: &[Statement], span: Span) {
        let iterable_type = self.type_of_expr(iterable);
        let Type::List(element_type) = iterable_type else {
            self.error(
                "SEL-T003",
                "for iteration requires a List[T] iterable",
                span,
            );
            return;
        };
        self.scopes.push(Scope::default());
        self.declare_local(
            name.to_owned(),
            Symbol::Value {
                ty: *element_type,
                mutable: false,
            },
            span,
        );
        for statement in body {
            self.check_statement(statement);
        }
        self.scopes.pop();
    }

    fn check_return(&mut self, value: Option<&Expr>, span: Span) {
        let Some(expected) = self
            .current_function
            .as_ref()
            .map(|function| function.return_type.clone())
        else {
            self.error("SEL-T006", "`return` is only valid inside a function", span);
            return;
        };
        let actual = value.map_or(Type::Unit, |expr| self.type_of_expr(expr));
        self.ensure_assignable(
            &actual,
            &expected,
            value.map_or(span, Expr::span),
            "return value",
        );
    }

    fn check_require(&mut self, condition: &Expr, failure: &Expr, span: Span) {
        let condition_type = self.type_of_expr(condition);
        self.ensure_bool(&condition_type, condition.span(), "require condition");
        let Some(return_type) = self
            .current_function
            .as_ref()
            .map(|function| function.return_type.clone())
        else {
            self.error(
                "SEL-R001",
                "`require` is only valid inside a Result function",
                span,
            );
            self.type_of_expr(failure);
            return;
        };
        let failure_type = self.type_of_expr(failure);
        if let Type::Result(_, error_type) = return_type {
            self.ensure_assignable(&failure_type, &error_type, failure.span(), "require error");
        } else {
            self.error(
                "SEL-R001",
                "`require` needs a function returning Result[T, E]",
                span,
            );
        }
    }

    fn check_if_branch(&mut self, branch: &IfBranch) {
        let condition_type = self.type_of_expr(&branch.condition);
        self.ensure_bool(&condition_type, branch.condition.span(), "if condition");
        self.scopes.push(Scope::default());
        for statement in &branch.body {
            self.check_statement(statement);
        }
        self.scopes.pop();
    }

    fn check_match(&mut self, statement: &MatchStatement) {
        let value_type = self.type_of_expr(&statement.value);
        let mut covered = HashSet::new();
        let mut total = false;
        for case in &statement.cases {
            if total {
                self.error(
                    "SEL-R004",
                    "match case is unreachable after total coverage",
                    case.span,
                );
            }
            self.scopes.push(Scope::default());
            let coverage = self.check_pattern(&case.pattern, &value_type);
            if let Some(guard) = &case.guard {
                let guard_type = self.type_of_expr(guard);
                self.ensure_bool(&guard_type, guard.span(), "match guard");
            }
            if case.guard.is_none() {
                match coverage {
                    Coverage::Wildcard => total = true,
                    Coverage::Keys(keys) => {
                        covered.extend(keys);
                    }
                    Coverage::Owned => {}
                }
            }
            for body_statement in &case.body {
                self.check_statement(body_statement);
            }
            self.scopes.pop();
        }
        if !total {
            for key in self.required_coverage(&value_type) {
                if !covered.contains(&key) {
                    self.error(
                        "SEL-R003",
                        format!("non-exhaustive match; missing `{key}`"),
                        statement.span,
                    );
                }
            }
        }
    }

    fn check_pattern(&mut self, pattern: &Pattern, expected: &Type) -> Coverage {
        match pattern {
            Pattern::Wildcard(_) => Coverage::Wildcard,
            Pattern::Binding { name, span } => {
                let is_variant = match expected {
                    Type::Enum(enum_name) => self.enums.get(enum_name).is_some_and(|definition| {
                        definition
                            .variants
                            .iter()
                            .any(|variant| variant.name == *name)
                    }),
                    Type::Option(_) => name == "None",
                    _ => false,
                };
                if is_variant {
                    return self.check_constructor_name(name, &[], expected, *span);
                }
                self.declare_local(
                    name.clone(),
                    Symbol::Value {
                        ty: expected.clone(),
                        mutable: false,
                    },
                    *span,
                );
                Coverage::Wildcard
            }
            Pattern::Boolean { value, span } => {
                self.ensure_type(expected, &Type::Bool, *span, "Boolean pattern");
                if *value {
                    Coverage::Keys(vec!["bool:true".to_owned()])
                } else {
                    Coverage::Keys(vec!["bool:false".to_owned()])
                }
            }
            Pattern::Integer { span, .. } => {
                self.ensure_type(expected, &Type::I64, *span, "integer pattern");
                Coverage::Owned
            }
            Pattern::String { span, .. } => {
                self.ensure_type(expected, &Type::Text, *span, "text pattern");
                Coverage::Owned
            }
            Pattern::Null(span) => {
                if !matches!(expected, Type::Option(_)) {
                    self.error("SEL-R002", "`null` pattern requires Option[T]", *span);
                }
                Coverage::Keys(vec!["option:none".to_owned()])
            }
            Pattern::Constructor {
                path,
                arguments,
                span,
            } => {
                if path.len() > 1 {
                    let valid_qualifier = match expected {
                        Type::Enum(enum_name) => path.len() == 2 && path[0].as_str() == enum_name,
                        Type::Option(_) | Type::Result(_, _) => false,
                        _ => false,
                    };
                    if !valid_qualifier {
                        self.error(
                            "SEL-R002",
                            "qualified constructor does not match the matched type",
                            *span,
                        );
                        return Coverage::Owned;
                    }
                }
                let name = path.last().map_or("", String::as_str);
                self.check_constructor_name(name, arguments, expected, *span)
            }
        }
    }

    fn check_constructor_name(
        &mut self,
        name: &str,
        arguments: &[Pattern],
        expected: &Type,
        span: Span,
    ) -> Coverage {
        match expected {
            Type::Bool => {
                self.error(
                    "SEL-R002",
                    "Boolean patterns must use `true` or `false`",
                    span,
                );
                Coverage::Owned
            }
            Type::Option(inner) => match name {
                "Some" => {
                    if arguments.len() != 1 {
                        self.error("SEL-R002", "`Some` expects one pattern", span);
                    } else {
                        self.check_pattern(&arguments[0], inner);
                    }
                    Coverage::Keys(vec!["option:some".to_owned()])
                }
                "None" => {
                    if !arguments.is_empty() {
                        self.error("SEL-R002", "`None` expects no patterns", span);
                    }
                    Coverage::Keys(vec!["option:none".to_owned()])
                }
                _ => {
                    self.error(
                        "SEL-R002",
                        format!("unknown Option constructor `{name}`"),
                        span,
                    );
                    Coverage::Owned
                }
            },
            Type::Result(ok, err) => match name {
                "Ok" | "Err" => {
                    if arguments.len() != 1 {
                        self.error("SEL-R002", format!("`{name}` expects one pattern"), span);
                    } else {
                        self.check_pattern(&arguments[0], if name == "Ok" { ok } else { err });
                    }
                    if name == "Ok" {
                        Coverage::Keys(vec!["result:ok".to_owned()])
                    } else {
                        Coverage::Keys(vec!["result:err".to_owned()])
                    }
                }
                _ => {
                    self.error(
                        "SEL-R002",
                        format!("unknown Result constructor `{name}`"),
                        span,
                    );
                    Coverage::Owned
                }
            },
            Type::Enum(enum_name) => {
                let Some(definition) = self.enums.get(enum_name).cloned() else {
                    return Coverage::Owned;
                };
                let Some(variant) = definition
                    .variants
                    .iter()
                    .find(|variant| variant.name == name)
                else {
                    self.error(
                        "SEL-R002",
                        format!("unknown variant `{name}` for enum `{enum_name}`"),
                        span,
                    );
                    return Coverage::Owned;
                };
                if variant.fields.len() != arguments.len() {
                    self.error(
                        "SEL-R002",
                        format!(
                            "variant `{name}` expects {} pattern(s)",
                            variant.fields.len()
                        ),
                        span,
                    );
                }
                for (pattern, (field_type, _)) in arguments.iter().zip(&variant.fields) {
                    self.check_pattern(pattern, field_type);
                }
                Coverage::Keys(vec![format!("enum:{enum_name}:{name}")])
            }
            _ => {
                self.error(
                    "SEL-R002",
                    "constructor pattern requires Option, Result or enum",
                    span,
                );
                Coverage::Owned
            }
        }
    }

    fn required_coverage(&self, ty: &Type) -> Vec<String> {
        match ty {
            Type::Bool => vec!["bool:true".to_owned(), "bool:false".to_owned()],
            Type::Option(_) => vec!["option:some".to_owned(), "option:none".to_owned()],
            Type::Result(_, _) => vec!["result:ok".to_owned(), "result:err".to_owned()],
            Type::Enum(name) => self
                .enums
                .get(name)
                .map(|definition| {
                    definition
                        .variants
                        .iter()
                        .map(|variant| format!("enum:{name}:{}", variant.name))
                        .collect()
                })
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    fn type_of_expr(&mut self, expression: &Expr) -> Type {
        match expression {
            Expr::Name { name, span } => {
                if name == "None" {
                    return Type::Option(Box::new(Type::Unknown));
                }
                match self.lookup(name) {
                    Some(Symbol::Value { ty, .. }) => ty,
                    Some(Symbol::Function(signature)) => {
                        Type::Function(signature.parameters, Box::new(signature.return_type))
                    }
                    None => {
                        self.error("SEL-M002", format!("undefined name `{name}`"), *span);
                        Type::Error
                    }
                }
            }
            Expr::Integer { .. } => Type::I64,
            Expr::Float { .. } => Type::F64,
            Expr::String { .. } => Type::Text,
            Expr::Boolean { .. } => Type::Bool,
            Expr::Null(_) => Type::Option(Box::new(Type::Unknown)),
            Expr::List { values, span } => {
                let mut element = Type::Unknown;
                for value in values {
                    let value_type = self.type_of_expr(value);
                    if matches!(element, Type::Unknown) {
                        element = value_type;
                    } else {
                        self.ensure_assignable(&value_type, &element, value.span(), "list element");
                    }
                }
                let _ = span;
                Type::List(Box::new(element))
            }
            Expr::Index {
                object,
                index,
                span,
            } => {
                let object_type = self.type_of_expr(object);
                let index_type = self.type_of_expr(index);
                self.ensure_type(&index_type, &Type::I64, index.span(), "list index");
                match object_type {
                    Type::List(element_type) => *element_type,
                    Type::Unknown | Type::Error => Type::Error,
                    other => {
                        self.error(
                            "SEL-T003",
                            format!("indexing requires a List[T], got {}", display_type(&other)),
                            *span,
                        );
                        Type::Error
                    }
                }
            }
            Expr::Group { value, .. } => self.type_of_expr(value),
            Expr::Unary {
                operator,
                value,
                span,
            } => {
                let value_type = self.type_of_expr(value);
                match operator.as_str() {
                    "not" | "!" => {
                        self.ensure_bool(&value_type, *span, "logical operand");
                        Type::Bool
                    }
                    "-" => {
                        if !matches!(
                            value_type,
                            Type::I64 | Type::F64 | Type::Unknown | Type::Error
                        ) {
                            self.error("SEL-T003", "unary `-` requires a numeric operand", *span);
                        }
                        value_type
                    }
                    _ => Type::Error,
                }
            }
            Expr::Binary {
                left,
                operator,
                right,
                span,
            } => self.type_of_binary(left, operator, right, *span),
            Expr::Call {
                callee,
                arguments,
                span,
            } => self.type_of_call(callee, arguments, *span),
            Expr::NamedArgument { value, .. } => self.type_of_expr(value),
            Expr::Member {
                object,
                name,
                optional,
                span,
            } => self.type_of_member(object, name, *optional, *span),
            Expr::Propagate { value, span } => {
                let value_type = self.type_of_expr(value);
                let Some(return_type) = self
                    .current_function
                    .as_ref()
                    .map(|function| function.return_type.clone())
                else {
                    self.error(
                        "SEL-R002",
                        "`?` is only valid inside a Result function",
                        *span,
                    );
                    return Type::Error;
                };
                let Type::Result(inner, error_type) = value_type else {
                    self.error("SEL-R002", "`?` requires a Result value", *span);
                    return Type::Error;
                };
                let Type::Result(_, expected_error) = return_type else {
                    self.error(
                        "SEL-R002",
                        "`?` requires a Result-returning function",
                        *span,
                    );
                    return Type::Error;
                };
                self.ensure_assignable(&error_type, &expected_error, *span, "propagated error");
                *inner
            }
        }
    }

    fn type_of_binary(&mut self, left: &Expr, operator: &str, right: &Expr, span: Span) -> Type {
        if operator == "|>" {
            let left_type = self.type_of_expr(left);
            return self.type_of_pipeline(left_type, right, span);
        }
        let left_type = self.type_of_expr(left);
        let right_type = self.type_of_expr(right);
        match operator {
            "and" | "or" => {
                self.ensure_bool(&left_type, left.span(), "logical operand");
                self.ensure_bool(&right_type, right.span(), "logical operand");
                Type::Bool
            }
            "??" => {
                if let Type::Option(inner) = left_type {
                    self.ensure_assignable(&right_type, &inner, right.span(), "Option fallback");
                    *inner
                } else {
                    self.error(
                        "SEL-T003",
                        "`??` requires an Option value on the left",
                        span,
                    );
                    Type::Error
                }
            }
            "+" => {
                if left_type == Type::Text && right_type == Type::Text {
                    Type::Text
                } else if let (Type::List(left_element), Type::List(right_element)) =
                    (&left_type, &right_type)
                {
                    self.ensure_assignable(
                        right_element,
                        left_element,
                        right.span(),
                        "list element",
                    );
                    Type::List(left_element.clone())
                } else {
                    self.numeric_binary(&left_type, &right_type, span)
                }
            }
            "-" | "*" | "/" | "%" | "**" => self.numeric_binary(&left_type, &right_type, span),
            "==" | "!=" | "<" | "<=" | ">" | ">=" => {
                self.ensure_assignable(&right_type, &left_type, right.span(), "comparison operand");
                Type::Bool
            }
            "in" | "is" => {
                self.error(
                    "SEL-T003",
                    format!("operator `{operator}` is not typed in E3"),
                    span,
                );
                Type::Error
            }
            _ => Type::Error,
        }
    }

    fn type_of_pipeline(&mut self, input: Type, right: &Expr, span: Span) -> Type {
        let Expr::Name { name, .. } = right else {
            self.error(
                "SEL-T005",
                "pipeline RHS must be a named function in E3",
                span,
            );
            self.type_of_expr(right);
            return Type::Error;
        };
        let Some(Symbol::Function(signature)) = self.lookup(name) else {
            self.error(
                "SEL-M002",
                format!("undefined pipeline function `{name}`"),
                right.span(),
            );
            return Type::Error;
        };
        if signature.parameters.len() != 1 {
            self.error(
                "SEL-T005",
                format!("pipeline function `{name}` must take exactly one argument"),
                right.span(),
            );
            return Type::Error;
        }
        self.ensure_assignable(
            &input,
            &signature.parameters[0],
            right.span(),
            "pipeline input",
        );
        signature.return_type
    }

    fn type_of_call(&mut self, callee: &Expr, arguments: &[Expr], span: Span) -> Type {
        if let Expr::Name { name, .. } = callee {
            if self.stdlib_imports.contains(name) && name != "print" {
                return self.type_of_standard_call(name, arguments, span);
            }
            if name == "print" && self.io_print_imported {
                if arguments.len() != 1 {
                    self.error("SEL-T005", "`print` expects one argument", span);
                    for argument in arguments {
                        self.type_of_expr(argument);
                    }
                    return Type::Error;
                }
                self.ensure_expr_assignable(&arguments[0], &Type::Text, "print value");
                return Type::Unit;
            }
            if let Some(builtin) = builtins::lookup(name) {
                return self.type_of_builtin_call(builtin, arguments, span);
            }
            match name.as_str() {
                "Ok" => {
                    if arguments.len() != 1 {
                        self.error("SEL-T005", "`Ok` expects one argument", span);
                        return Type::Error;
                    }
                    return Type::Result(
                        Box::new(self.type_of_expr(&arguments[0])),
                        Box::new(Type::Unknown),
                    );
                }
                "Err" => {
                    if arguments.len() != 1 {
                        self.error("SEL-T005", "`Err` expects one argument", span);
                        return Type::Error;
                    }
                    return Type::Result(
                        Box::new(Type::Unknown),
                        Box::new(self.type_of_expr(&arguments[0])),
                    );
                }
                "Some" => {
                    if arguments.len() != 1 {
                        self.error("SEL-T005", "`Some` expects one argument", span);
                        return Type::Error;
                    }
                    return Type::Option(Box::new(self.type_of_expr(&arguments[0])));
                }
                _ => {}
            }
            if let Some(record) = self.records.get(name).cloned() {
                if record.fields.len() != arguments.len() {
                    self.error(
                        "SEL-T005",
                        format!(
                            "record `{name}` expects {} argument(s)",
                            record.fields.len()
                        ),
                        span,
                    );
                }
                for (argument, (_, field_type, _)) in arguments.iter().zip(record.fields.iter()) {
                    let actual = self.type_of_expr(argument);
                    self.ensure_assignable(
                        &actual,
                        field_type,
                        argument.span(),
                        "constructor argument",
                    );
                }
                return Type::Record(name.clone());
            }
            if let Some(Symbol::Function(signature)) = self.lookup(name) {
                self.check_arguments(arguments, &signature.parameters, span, name);
                return signature.return_type;
            }
        }
        if let Expr::Member { object, name, .. } = callee
            && let Expr::Name {
                name: enum_name, ..
            } = object.as_ref()
            && let Some(definition) = self.enums.get(enum_name).cloned()
            && let Some(variant) = definition
                .variants
                .iter()
                .find(|variant| variant.name == *name)
        {
            if variant.fields.len() != arguments.len() {
                self.error(
                    "SEL-T005",
                    format!(
                        "variant `{name}` expects {} argument(s)",
                        variant.fields.len()
                    ),
                    span,
                );
            }
            for (argument, (field_type, _)) in arguments.iter().zip(&variant.fields) {
                let actual = self.type_of_expr(argument);
                self.ensure_assignable(&actual, field_type, argument.span(), "variant argument");
            }
            return Type::Enum(enum_name.clone());
        }
        let callee_type = self.type_of_expr(callee);
        if let Type::Function(parameters, return_type) = callee_type {
            self.check_arguments(arguments, &parameters, span, "callable");
            *return_type
        } else {
            self.error("SEL-T005", "expression is not callable", callee.span());
            Type::Error
        }
    }

    fn type_of_builtin_call(&mut self, builtin: Builtin, arguments: &[Expr], span: Span) -> Type {
        let name = builtin_name(builtin);
        let expected_arguments = match builtin {
            Builtin::TensorF32 | Builtin::TensorI64 | Builtin::Reshape => 2,
            Builtin::ZerosF32 | Builtin::ZerosI64 => 1,
            Builtin::TensorShape
            | Builtin::TensorRank
            | Builtin::TensorElements
            | Builtin::TensorDtype => 1,
            Builtin::Sum | Builtin::Mean => 0,
            Builtin::Matmul | Builtin::Softmax | Builtin::LogSumExp | Builtin::SumToShape => 2,
            Builtin::Transpose | Builtin::Relu | Builtin::Exp | Builtin::Log => 1,
        };
        let valid_arity = if matches!(builtin, Builtin::Sum | Builtin::Mean) {
            arguments.len() == 1 || arguments.len() == 2
        } else {
            arguments.len() == expected_arguments
        };
        if !valid_arity {
            let expected_description = if matches!(builtin, Builtin::Sum | Builtin::Mean) {
                "1 or 2".to_owned()
            } else {
                expected_arguments.to_string()
            };
            self.error(
                "SEL-T005",
                format!(
                    "builtin `{name}` expects {} argument(s), got {}",
                    expected_description,
                    arguments.len()
                ),
                span,
            );
            for argument in arguments {
                self.type_of_expr(argument);
            }
            return Type::Error;
        }

        match builtin {
            Builtin::TensorF32 => {
                self.ensure_expr_assignable(
                    &arguments[0],
                    &Type::List(Box::new(Type::F64)),
                    "tensor_f32 values",
                );
                self.ensure_expr_assignable(
                    &arguments[1],
                    &Type::List(Box::new(Type::I64)),
                    "tensor_f32 shape",
                );
                Type::Tensor(TensorDtype::F32)
            }
            Builtin::TensorI64 => {
                self.ensure_expr_assignable(
                    &arguments[0],
                    &Type::List(Box::new(Type::I64)),
                    "tensor_i64 values",
                );
                self.ensure_expr_assignable(
                    &arguments[1],
                    &Type::List(Box::new(Type::I64)),
                    "tensor_i64 shape",
                );
                Type::Tensor(TensorDtype::I64)
            }
            Builtin::ZerosF32 => {
                self.ensure_expr_assignable(
                    &arguments[0],
                    &Type::List(Box::new(Type::I64)),
                    "tensor_zeros_f32 shape",
                );
                Type::Tensor(TensorDtype::F32)
            }
            Builtin::ZerosI64 => {
                self.ensure_expr_assignable(
                    &arguments[0],
                    &Type::List(Box::new(Type::I64)),
                    "tensor_zeros_i64 shape",
                );
                Type::Tensor(TensorDtype::I64)
            }
            Builtin::TensorShape => {
                self.ensure_tensor_argument(&arguments[0], "tensor_shape");
                Type::List(Box::new(Type::I64))
            }
            Builtin::TensorRank | Builtin::TensorElements => {
                self.ensure_tensor_argument(&arguments[0], name);
                Type::I64
            }
            Builtin::TensorDtype => {
                self.ensure_tensor_argument(&arguments[0], name);
                Type::Text
            }
            Builtin::Reshape => {
                let tensor_type = self.type_of_expr(&arguments[0]);
                self.ensure_tensor_type(&tensor_type, arguments[0].span(), "reshape tensor");
                self.ensure_expr_assignable(
                    &arguments[1],
                    &Type::List(Box::new(Type::I64)),
                    "reshape shape",
                );
                tensor_type
            }
            Builtin::Sum => {
                let tensor_type = self.type_of_expr(&arguments[0]);
                self.ensure_tensor_type(&tensor_type, arguments[0].span(), "tensor_sum tensor");
                if let Some(axis) = arguments.get(1) {
                    self.ensure_expr_assignable(axis, &Type::I64, "tensor_sum axis");
                }
                tensor_type
            }
            Builtin::Mean => {
                self.ensure_expr_assignable(
                    &arguments[0],
                    &Type::Tensor(TensorDtype::F32),
                    "tensor_mean tensor",
                );
                if let Some(axis) = arguments.get(1) {
                    self.ensure_expr_assignable(axis, &Type::I64, "tensor_mean axis");
                }
                Type::Tensor(TensorDtype::F32)
            }
            Builtin::Matmul => {
                for argument in arguments {
                    self.ensure_expr_assignable(
                        argument,
                        &Type::Tensor(TensorDtype::F32),
                        "matmul operand",
                    );
                }
                Type::Tensor(TensorDtype::F32)
            }
            Builtin::Transpose => self.ensure_tensor_argument(&arguments[0], "transpose"),
            Builtin::Relu | Builtin::Exp | Builtin::Log => {
                self.ensure_expr_assignable(&arguments[0], &Type::Tensor(TensorDtype::F32), name);
                Type::Tensor(TensorDtype::F32)
            }
            Builtin::Softmax | Builtin::LogSumExp => {
                self.ensure_expr_assignable(&arguments[0], &Type::Tensor(TensorDtype::F32), name);
                self.ensure_expr_assignable(&arguments[1], &Type::I64, "reduction axis");
                Type::Tensor(TensorDtype::F32)
            }
            Builtin::SumToShape => {
                self.ensure_expr_assignable(&arguments[0], &Type::Tensor(TensorDtype::F32), name);
                self.ensure_expr_assignable(
                    &arguments[1],
                    &Type::List(Box::new(Type::I64)),
                    "reduction target shape",
                );
                Type::Tensor(TensorDtype::F32)
            }
        }
    }

    fn type_of_standard_call(&mut self, name: &str, arguments: &[Expr], span: Span) -> Type {
        let result = |value: Type| Type::Result(Box::new(value), Box::new(Type::JsonIoError));
        match name {
            "read_file" => {
                self.check_standard_arity(name, arguments, 1, span);
                if let Some(path) = arguments.first() {
                    self.ensure_expr_assignable(path, &Type::Text, "JSON path");
                }
                result(Type::Json)
            }
            "parse" => {
                self.check_standard_arity(name, arguments, 1, span);
                if let Some(text) = arguments.first() {
                    self.ensure_expr_assignable(text, &Type::Text, "JSON text");
                }
                result(Type::Json)
            }
            "encode" => {
                self.check_standard_arity(name, arguments, 1, span);
                if let Some(value) = arguments.first() {
                    self.ensure_expr_assignable(value, &Type::Json, "JSON value");
                }
                result(Type::Text)
            }
            "get" => {
                self.check_standard_arity(name, arguments, 2, span);
                if let Some(value) = arguments.first() {
                    self.ensure_expr_assignable(value, &Type::Json, "JSON value");
                }
                if let Some(path) = arguments.get(1) {
                    self.ensure_expr_assignable(
                        path,
                        &Type::List(Box::new(Type::Text)),
                        "JSON path",
                    );
                }
                result(Type::Json)
            }
            "as_text" | "as_i64" | "as_f32" | "as_bool" => {
                self.check_standard_arity(name, arguments, 1, span);
                if let Some(value) = arguments.first() {
                    self.ensure_expr_assignable(value, &Type::Json, "JSON value");
                }
                let value = match name {
                    "as_text" => Type::Text,
                    "as_i64" => Type::I64,
                    "as_f32" => Type::F64,
                    "as_bool" => Type::Bool,
                    _ => Type::Error,
                };
                result(value)
            }
            "ModelRequest" => {
                self.check_standard_arity(name, arguments, 4, span);
                Type::ModelRequest
            }
            "fixture_provider" => {
                self.check_standard_arity(name, arguments, 1, span);
                if let Some(request) = arguments.first() {
                    self.ensure_expr_assignable(request, &Type::ModelRequest, "model request");
                }
                Type::Result(Box::new(Type::ModelResponse), Box::new(Type::ModelError))
            }
            "slow_fixture_provider" => {
                self.check_standard_arity(name, arguments, 2, span);
                if let Some(request) = arguments.first() {
                    self.ensure_expr_assignable(request, &Type::ModelRequest, "model request");
                }
                if let Some(timeout) = arguments.get(1) {
                    self.ensure_expr_assignable(timeout, &Type::I64, "model timeout");
                }
                Type::Result(Box::new(Type::Text), Box::new(Type::ModelError))
            }
            "gradcheck" => {
                self.check_standard_arity(name, arguments, 2, span);
                Type::F32
            }
            "backward" => {
                self.check_standard_arity(name, arguments, 2, span);
                Type::Tensor(TensorDtype::F32)
            }
            "train_linear" | "resume_and_compare" => {
                if name == "train_linear" {
                    self.check_standard_arity(name, arguments, 4, span);
                } else {
                    self.check_standard_arity(name, arguments, 2, span);
                }
                Type::Json
            }
            "load_checkpoint" => {
                self.check_standard_arity(name, arguments, 1, span);
                Type::Result(Box::new(Type::Json), Box::new(Type::CheckpointError))
            }
            _ => {
                self.error(
                    "SEL-M002",
                    format!("standard function `{name}` is unavailable"),
                    span,
                );
                Type::Error
            }
        }
    }

    fn check_standard_arity(
        &mut self,
        name: &str,
        arguments: &[Expr],
        expected: usize,
        span: Span,
    ) {
        if arguments.len() != expected {
            self.error(
                "SEL-T005",
                format!("standard function `{name}` expects {expected} argument(s)"),
                span,
            );
        }
    }

    fn ensure_tensor_argument(&mut self, argument: &Expr, context: &str) -> Type {
        let actual = self.type_of_expr(argument);
        self.ensure_tensor_type(&actual, argument.span(), context);
        actual
    }

    fn ensure_tensor_type(&mut self, actual: &Type, span: Span, context: &str) {
        if !matches!(actual, Type::Tensor(_) | Type::Unknown | Type::Error) {
            self.error(
                "SEL-T002",
                format!(
                    "{context} expects a Tensor[F32] or Tensor[I64], got {}",
                    display_type(actual)
                ),
                span,
            );
        }
    }

    fn ensure_expr_assignable(&mut self, expression: &Expr, expected: &Type, context: &str) {
        let actual = self.type_of_expr(expression);
        self.ensure_assignable(&actual, expected, expression.span(), context);
    }

    fn check_arguments(&mut self, arguments: &[Expr], expected: &[Type], span: Span, name: &str) {
        if arguments.len() != expected.len() {
            self.error(
                "SEL-T005",
                format!(
                    "{name} expects {} argument(s), got {}",
                    expected.len(),
                    arguments.len()
                ),
                span,
            );
        }
        for (argument, expected_type) in arguments.iter().zip(expected) {
            let actual = self.type_of_expr(argument);
            self.ensure_assignable(&actual, expected_type, argument.span(), "call argument");
        }
    }

    fn type_of_member(&mut self, object: &Expr, name: &str, optional: bool, span: Span) -> Type {
        if optional {
            self.error(
                "SEL-T007",
                "optional member access is reserved until nullable types are implemented",
                span,
            );
            return Type::Error;
        }
        if let Expr::Name {
            name: enum_name, ..
        } = object
            && let Some(definition) = self.enums.get(enum_name)
            && let Some(variant) = definition
                .variants
                .iter()
                .find(|variant| variant.name == name)
        {
            if !variant.fields.is_empty() {
                self.error(
                    "SEL-T005",
                    format!("variant `{enum_name}.{name}` requires constructor arguments"),
                    span,
                );
                return Type::Error;
            }
            return Type::Enum(enum_name.clone());
        }
        let object_type = self.type_of_expr(object);
        if let Type::Record(record_name) = object_type {
            if let Some(record) = self.records.get(&record_name)
                && let Some((_, field_type, _)) =
                    record.fields.iter().find(|(field, _, _)| field == name)
            {
                return field_type.clone();
            }
            self.error(
                "SEL-T007",
                format!("record `{record_name}` has no field `{name}`"),
                span,
            );
            return Type::Error;
        }
        if matches!(&object_type, Type::ModelResponse) {
            return match name {
                "status" | "text" => Type::Text,
                _ => {
                    self.error(
                        "SEL-T007",
                        format!("ModelResponse has no field `{name}`"),
                        span,
                    );
                    Type::Error
                }
            };
        }
        self.error("SEL-T007", format!("cannot access member `{name}`"), span);
        Type::Error
    }

    fn type_from_expr(&mut self, expression: &TypeExpr) -> Type {
        match expression.name.as_str() {
            "Unit" if expression.arguments.is_empty() => Type::Unit,
            "Bool" if expression.arguments.is_empty() => Type::Bool,
            "I64" if expression.arguments.is_empty() => Type::I64,
            "F64" if expression.arguments.is_empty() => Type::F64,
            "F32" if expression.arguments.is_empty() => Type::F32,
            "Text" if expression.arguments.is_empty() => Type::Text,
            "Tensor" if expression.arguments.len() == 1 => {
                match expression.arguments[0].name.as_str() {
                    "F32" if expression.arguments[0].arguments.is_empty() => {
                        Type::Tensor(TensorDtype::F32)
                    }
                    "I64" if expression.arguments[0].arguments.is_empty() => {
                        Type::Tensor(TensorDtype::I64)
                    }
                    _ => {
                        self.error(
                            "SEL-T001",
                            "Tensor type requires F32 or I64 dtype",
                            expression.arguments[0].span,
                        );
                        Type::Error
                    }
                }
            }
            "List" if expression.arguments.len() == 1 => {
                Type::List(Box::new(self.type_from_expr(&expression.arguments[0])))
            }
            "Option" if expression.arguments.len() == 1 => {
                Type::Option(Box::new(self.type_from_expr(&expression.arguments[0])))
            }
            "Result" if expression.arguments.len() == 2 => Type::Result(
                Box::new(self.type_from_expr(&expression.arguments[0])),
                Box::new(self.type_from_expr(&expression.arguments[1])),
            ),
            "Json" if expression.arguments.is_empty() => Type::Json,
            "JsonError" if expression.arguments.is_empty() => Type::JsonError,
            "JsonIoError" if expression.arguments.is_empty() => Type::JsonIoError,
            "ModelRequest" if expression.arguments.is_empty() => Type::ModelRequest,
            "ModelResponse" if expression.arguments.is_empty() => Type::ModelResponse,
            "ModelError" if expression.arguments.is_empty() => Type::ModelError,
            "CheckpointError" if expression.arguments.is_empty() => Type::CheckpointError,
            name if expression.arguments.is_empty() && self.records.contains_key(name) => {
                Type::Record(name.to_owned())
            }
            name if expression.arguments.is_empty() && self.enums.contains_key(name) => {
                Type::Enum(name.to_owned())
            }
            name => {
                self.error(
                    "SEL-T001",
                    format!("unknown or malformed type `{name}`"),
                    expression.span,
                );
                Type::Error
            }
        }
    }

    fn lookup(&self, name: &str) -> Option<Symbol> {
        for scope in self.scopes.iter().rev() {
            if let Some(symbol) = scope.values.get(name) {
                return Some(symbol.clone());
            }
        }
        self.globals.get(name).cloned()
    }

    fn declare_local(&mut self, name: String, symbol: Symbol, span: Span) {
        let scope = self.scopes.last_mut().expect("a local scope is active");
        let message = format!("duplicate declaration `{name}`");
        let duplicate = match scope.values.entry(name) {
            Entry::Occupied(_) => true,
            Entry::Vacant(entry) => {
                entry.insert(symbol);
                false
            }
        };
        if duplicate {
            self.error("SEL-M003", message, span);
        }
    }

    fn numeric_binary(&mut self, left: &Type, right: &Type, span: Span) -> Type {
        if left == &Type::I64 && right == &Type::I64 {
            return Type::I64;
        }
        if left == &Type::F64 && right == &Type::F64 {
            return Type::F64;
        }
        if left == &Type::F32 && right == &Type::F32 {
            return Type::F32;
        }
        if let (Type::Tensor(left), Type::Tensor(right)) = (left, right)
            && left == right
        {
            return Type::Tensor(*left);
        }
        if matches!(left, Type::Unknown | Type::Error)
            || matches!(right, Type::Unknown | Type::Error)
        {
            return Type::Error;
        }
        self.error(
            "SEL-T003",
            "numeric operator requires matching I64 or F64 operands",
            span,
        );
        Type::Error
    }

    fn ensure_bool(&mut self, actual: &Type, span: Span, context: &str) {
        self.ensure_type(actual, &Type::Bool, span, context);
    }

    fn ensure_type(&mut self, actual: &Type, expected: &Type, span: Span, context: &str) {
        if !assignable(actual, expected) {
            self.error(
                "SEL-T004",
                format!(
                    "{context} must have type {}, got {}",
                    display_type(expected),
                    display_type(actual)
                ),
                span,
            );
        }
    }

    fn ensure_assignable(&mut self, actual: &Type, expected: &Type, span: Span, context: &str) {
        if !assignable(actual, expected) {
            self.error(
                "SEL-T002",
                format!(
                    "{context} expects {}, got {}",
                    display_type(expected),
                    display_type(actual)
                ),
                span,
            );
        }
    }

    fn ensure_annotated_assignable(
        &mut self,
        actual: &Type,
        expected: &Type,
        span: Span,
        context: &str,
    ) {
        if !assignable(actual, expected) {
            let diagnostic = Diagnostic::with_help(
                "SEL-T102",
                format!(
                    "{context} expects {}, got {}",
                    display_type(expected),
                    display_type(actual)
                ),
                span,
                "change the annotation or the value so their types agree",
            );
            self.diagnostics.push(diagnostic);
        }
    }

    fn error(&mut self, code: &'static str, message: impl Into<String>, span: Span) {
        self.diagnostics.push(Diagnostic::new(code, message, span));
    }
}

fn assignable(actual: &Type, expected: &Type) -> bool {
    if matches!(actual, Type::Unknown | Type::Error)
        || matches!(expected, Type::Unknown | Type::Error)
    {
        return true;
    }
    match (actual, expected) {
        (Type::F64, Type::F32) => true,
        (Type::List(actual), Type::List(expected))
        | (Type::Option(actual), Type::Option(expected)) => assignable(actual, expected),
        (
            Type::Result(actual_value, actual_error),
            Type::Result(expected_value, expected_error),
        ) => assignable(actual_value, expected_value) && assignable(actual_error, expected_error),
        (
            Type::Function(actual_params, actual_return),
            Type::Function(expected_params, expected_return),
        ) => {
            actual_params.len() == expected_params.len()
                && actual_params
                    .iter()
                    .zip(expected_params)
                    .all(|(actual, expected)| assignable(actual, expected))
                && assignable(actual_return, expected_return)
        }
        (Type::Tensor(actual), Type::Tensor(expected)) => actual == expected,
        _ => actual == expected,
    }
}

fn is_supported_standard_directive(text: &str) -> bool {
    let mut parts = text.split_whitespace();
    let Some(module) = parts.next() else {
        return false;
    };
    if parts.next() != Some("import") {
        return false;
    }
    let symbols = parts
        .map(|part| part.trim_end_matches(','))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if symbols.is_empty() {
        return false;
    }
    let allowed = match module {
        "io" => ["print"].as_slice(),
        "json" => [
            "parse",
            "read_file",
            "encode",
            "get",
            "as_text",
            "as_i64",
            "as_f32",
            "as_bool",
        ]
        .as_slice(),
        "models" => [
            "ModelRequest",
            "ModelResponse",
            "fixture_provider",
            "slow_fixture_provider",
        ]
        .as_slice(),
        "autograd" => ["backward", "gradcheck"].as_slice(),
        "training" => ["train_linear", "resume_and_compare", "load_checkpoint"].as_slice(),
        _ => return false,
    };
    symbols.iter().all(|symbol| allowed.contains(symbol))
}

fn display_type(ty: &Type) -> String {
    match ty {
        Type::Unit => "Unit".to_owned(),
        Type::Bool => "Bool".to_owned(),
        Type::I64 => "I64".to_owned(),
        Type::F64 => "F64".to_owned(),
        Type::F32 => "F32".to_owned(),
        Type::Text => "Text".to_owned(),
        Type::List(inner) => format!("List[{}]", display_type(inner)),
        Type::Option(inner) => format!("Option[{}]", display_type(inner)),
        Type::Result(value, error) => {
            format!("Result[{}, {}]", display_type(value), display_type(error))
        }
        Type::Tensor(dtype) => format!("Tensor[{}]", dtype),
        Type::Json
        | Type::JsonError
        | Type::JsonIoError
        | Type::ModelRequest
        | Type::ModelResponse
        | Type::ModelError
        | Type::CheckpointError => match ty {
            Type::Json => "Json".to_owned(),
            Type::JsonError => "JsonError".to_owned(),
            Type::JsonIoError => "JsonIoError".to_owned(),
            Type::ModelRequest => "ModelRequest".to_owned(),
            Type::ModelResponse => "ModelResponse".to_owned(),
            Type::ModelError => "ModelError".to_owned(),
            Type::CheckpointError => "CheckpointError".to_owned(),
            _ => unreachable!(),
        },
        Type::Record(name) | Type::Enum(name) => name.clone(),
        Type::Function(parameters, return_type) => format!(
            "fn({}) -> {}",
            parameters
                .iter()
                .map(display_type)
                .collect::<Vec<_>>()
                .join(", "),
            display_type(return_type)
        ),
        Type::Unknown => "unknown".to_owned(),
        Type::Error => "error".to_owned(),
    }
}

fn builtin_name(builtin: Builtin) -> &'static str {
    match builtin {
        Builtin::TensorF32 => "tensor_f32",
        Builtin::TensorI64 => "tensor_i64",
        Builtin::ZerosF32 => "tensor_zeros_f32",
        Builtin::ZerosI64 => "tensor_zeros_i64",
        Builtin::TensorShape => "tensor_shape",
        Builtin::TensorRank => "tensor_rank",
        Builtin::TensorElements => "tensor_elements",
        Builtin::TensorDtype => "tensor_dtype",
        Builtin::Reshape => "reshape",
        Builtin::Sum => "tensor_sum",
        Builtin::Mean => "tensor_mean",
        Builtin::Matmul => "matmul",
        Builtin::Transpose => "transpose",
        Builtin::Relu => "tensor_relu",
        Builtin::Exp => "tensor_exp",
        Builtin::Log => "tensor_log",
        Builtin::Softmax => "tensor_softmax",
        Builtin::LogSumExp => "tensor_logsumexp",
        Builtin::SumToShape => "tensor_sum_to_shape",
    }
}

#[cfg(test)]
mod tests {
    use super::check;
    use crate::parser::parse;

    fn diagnostics(source: &str) -> Vec<String> {
        let parsed = parse(source);
        assert!(
            parsed.diagnostics.is_empty(),
            "parser diagnostics: {:?}",
            parsed.diagnostics
        );
        check(&parsed.program.expect("program"))
            .into_iter()
            .map(|diagnostic| diagnostic.code.to_owned())
            .collect()
    }

    #[test]
    fn resolves_names_and_checks_scalar_types() {
        let source = "fn add(left: I64, right: I64) -> I64:\n    return left + right\nfn main() -> Unit:\n    let value: I64 = add(1, 2)\n    if value > 0:\n        return\n";
        assert!(diagnostics(source).is_empty());
        assert!(
            diagnostics("fn main() -> Unit:\n    return missing\n")
                .contains(&"SEL-M002".to_owned())
        );
        assert!(
            diagnostics("fn main() -> Unit:\n    if 1:\n        return\n")
                .contains(&"SEL-T004".to_owned())
        );
    }

    #[test]
    fn checks_result_require_propagation_and_public_signatures() {
        let valid = "pub fn parse(value: I64) -> Result[I64, Text]:\n    require value > 0 else \"bad\"\n    return Ok(value)\nfn wrapper(value: I64) -> Result[I64, Text]:\n    return Ok(parse(value)?)\n";
        assert!(diagnostics(valid).is_empty());
        assert!(
            diagnostics(
                "fn bad(value: I64) -> I64:\n    require value > 0 else \"bad\"\n    return value\n"
            )
            .contains(&"SEL-R001".to_owned())
        );
        assert!(
            diagnostics("pub fn bad(value) -> I64:\n    return 1\n")
                .contains(&"SEL-T001".to_owned())
        );
    }

    #[test]
    fn checks_match_exhaustiveness_and_guards() {
        let valid = "enum Flag:\n    On\n    Off\nfn main(flag: Flag) -> Unit:\n    match flag:\n        case On:\n            return\n        case Off:\n            return\n";
        assert!(diagnostics(valid).is_empty());
        let missing = "enum Flag:\n    On\n    Off\nfn main(flag: Flag) -> Unit:\n    match flag:\n        case On if true:\n            return\n";
        assert!(diagnostics(missing).contains(&"SEL-R003".to_owned()));
    }

    #[test]
    fn checks_records_options_results_and_member_types() {
        let source = "record Input:\n    text: Text\n    limit: I64\nfn validate(input: Input) -> Result[Text, Text]:\n    require input.limit > 0 else \"limit\"\n    return Ok(input.text)\nfn choose(value: Option[I64]) -> I64:\n    match value:\n        case Some(number):\n            return number\n        case None:\n            return 0\nfn main(input: Input) -> Unit:\n    let result: Result[Text, Text] = validate(input)\n    match result:\n        case Ok(text):\n            return\n        case Err(error):\n            return\n";
        assert!(diagnostics(source).is_empty());
    }

    #[test]
    fn checks_collection_iteration_indexing_and_mutability() {
        let valid = "fn main() -> I64:\n    var total: I64 = 0\n    for value in [1, 2, 3]:\n        total = total + value\n    return [total][0]\n";
        assert!(diagnostics(valid).is_empty());
        assert!(
            diagnostics("fn main() -> Unit:\n    let value: I64 = 1\n    value = 2\n    return\n")
                .contains(&"SEL-T003".to_owned())
        );
        assert!(
            diagnostics("fn main() -> Unit:\n    for value in 1:\n        return\n")
                .contains(&"SEL-T003".to_owned())
        );
        let invalid_index = diagnostics("fn main() -> I64:\n    return [true][0]\n");
        assert!(
            invalid_index.contains(&"SEL-T002".to_owned()),
            "{invalid_index:?}"
        );
    }

    #[test]
    fn rejects_duplicates_bad_calls_unimplemented_imports_and_unreachable_cases() {
        let duplicate = "fn add(value: I64) -> I64:\n    return value\nfn main() -> Unit:\n    let value: I64 = add(true)\n    let value: I64 = 1\n    return\n";
        let duplicate_diagnostics = diagnostics(duplicate);
        assert!(duplicate_diagnostics.contains(&"SEL-T002".to_owned()));
        assert!(duplicate_diagnostics.contains(&"SEL-M003".to_owned()));

        let unsupported = "import seloryn.tensor\nfn main() -> Unit:\n    return\n";
        assert!(diagnostics(unsupported).contains(&"SEL-M001".to_owned()));

        let unreachable = "fn main(flag: Bool) -> Unit:\n    match flag:\n        case _:\n            return\n        case true:\n            return\n";
        assert!(diagnostics(unreachable).contains(&"SEL-R004".to_owned()));

        let wrong_qualifier = "enum Flag:\n    On\n    Off\nenum Other:\n    On\nfn main(flag: Flag) -> Unit:\n    match flag:\n        case Other.On:\n            return\n        case Off:\n            return\n";
        assert!(diagnostics(wrong_qualifier).contains(&"SEL-R002".to_owned()));

        let missing_constructor_argument =
            "enum Flag:\n    On(I64)\nfn main() -> Flag:\n    return Flag.On\n";
        assert!(diagnostics(missing_constructor_argument).contains(&"SEL-T005".to_owned()));
    }

    #[test]
    fn malformed_source_corpus_does_not_panic() {
        let corpus = [
            "",
            ")(",
            "fn",
            "fn main(",
            "match value:",
            "match value:\n    case Ok(:\n        return\n",
            "record User:\n    name:\n",
            "fn main() -> Result[I64, Text]:\n    return Ok(\n",
            "fn main() -> Unit:\n    if:\n        return\n",
            "let value = [1, 2,",
            "pub import std\n",
            "fn main() -> I64:\n    return 1e_\n",
        ];
        for source in corpus {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let parsed = parse(source);
                if parsed.diagnostics.is_empty() {
                    let _ = check(&parsed.program.expect("program"));
                }
            }));
            assert!(result.is_ok(), "checker panicked for source: {source:?}");
        }
    }
}
