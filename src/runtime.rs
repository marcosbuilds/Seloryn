use crate::autograd;
use crate::builtins::{self, Builtin};
use crate::diagnostic::{Diagnostic, Span};
use crate::gpu;
use crate::ir::{
    Binding, EnumDecl, Expr, Function, IfBranch, Item, Pattern, Program, Record, Statement,
};
use crate::json::{self, JsonValue};
use crate::parser as source_parser;
use crate::tensor::{Tensor, TensorError};
use crate::tensor_autograd;
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

// Keep the logical recursion guard below the smallest stack budget used by
// supported test runners. The interpreter is recursive today, so a limit
// that is safe on a large Windows thread stack can still abort before the
// guard on macOS runners with smaller test-thread stacks.
const MAX_CALL_DEPTH: usize = 32;
const MAX_LOOP_ITERATIONS: usize = 1_000_000;
const MAX_LIST_ELEMENTS: usize = 1_000_000;
const MAX_RENDER_VALUES: usize = 4096;
const MAX_TRAINING_STEPS: i64 = 1_000_000;
const MAX_TRAINING_UPDATES: usize = 10_000_000;
const MAX_MLP_WORK_UNITS: usize = 100_000_000;
const MAX_TRAINING_DATASET_BYTES: usize = 1_048_576;
const MAX_TRAINING_SAMPLES: usize = 100_000;
const MAX_CHECKPOINT_BYTES: usize = 1_048_576;
const MAX_TRAINING_INPUTS: usize = 64;
const MAX_TRAINING_OUTPUTS: usize = 32;
const MAX_TRAINING_HIDDEN: usize = 256;
const MAX_TRAINING_PARAMETERS: usize = 1_000_000;
const MLP_RNG_SEED_MIX: u64 = 0x9E37_79B9_7F4A_7C15;
const MLP_RNG_NONZERO_FALLBACK: u64 = 0xD1B5_4A32_D192_ED03;
const LINEAR_DEVICE: &str = "cpu";
const LINEAR_DTYPE: &str = "f64";
const LINEAR_OPTIMIZER: &str = "sgd";
const LINEAR_LEARNING_RATE: f64 = 0.02;
const LINEAR_PARAMETER_COUNT: usize = 2;

/// Values that can be produced by the pure E4 interpreter.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Unit,
    Bool(bool),
    I64(i64),
    F64(f64),
    Text(String),
    Json(JsonValue),
    ModelRequest {
        model: String,
        prompt: String,
        max_output_tokens: i64,
        seed: i64,
    },
    ModelResponse {
        status: String,
        text: String,
    },
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
        let mut remaining = MAX_RENDER_VALUES;
        self.render_with_budget(&mut remaining)
    }

    fn render_with_budget(&self, remaining: &mut usize) -> String {
        match self {
            Self::Unit => "unit".to_owned(),
            Self::Bool(value) => value.to_string(),
            Self::I64(value) => value.to_string(),
            Self::F64(value) => format_f64(*value),
            Self::Text(value) => render_text(value, remaining),
            Self::Json(value) => json::encode(value),
            Self::ModelRequest { model, .. } => format!("ModelRequest({model})"),
            Self::ModelResponse { status, text } => format!(
                "{{\"status\":{},\"text\":{}}}",
                json_string(status),
                json_string(text)
            ),
            Self::List(values) => {
                let mut rendered = Vec::new();
                for value in values {
                    if *remaining == 0 {
                        break;
                    }
                    *remaining -= 1;
                    rendered.push(value.render_with_budget(remaining));
                }
                if rendered.len() < values.len() {
                    rendered.push(format!("... ({} more)", values.len() - rendered.len()));
                }
                format!("[{}]", rendered.join(", "))
            }
            Self::Tensor(tensor) => {
                let (rendered, used) = if *remaining == MAX_RENDER_VALUES {
                    (
                        tensor.render(),
                        MAX_RENDER_VALUES.min(tensor.element_count()),
                    )
                } else {
                    tensor.render_with_limit(*remaining)
                };
                *remaining = remaining.saturating_sub(used);
                rendered
            }
            Self::Record { name, fields } => {
                let mut rendered = Vec::new();
                for (field, value) in fields {
                    if *remaining == 0 {
                        break;
                    }
                    *remaining -= 1;
                    rendered.push(format!("{field}: {}", value.render_with_budget(remaining)));
                }
                if rendered.len() < fields.len() {
                    rendered.push(format!("... ({} more)", fields.len() - rendered.len()));
                }
                format!("{name}({})", rendered.join(", "))
            }
            Self::Enum {
                name,
                variant,
                fields,
            } => {
                if fields.is_empty() {
                    format!("{name}.{variant}")
                } else {
                    let mut rendered = Vec::new();
                    for value in fields {
                        if *remaining == 0 {
                            break;
                        }
                        *remaining -= 1;
                        rendered.push(value.render_with_budget(remaining));
                    }
                    if rendered.len() < fields.len() {
                        rendered.push(format!("... ({} more)", fields.len() - rendered.len()));
                    }
                    format!("{name}.{variant}({})", rendered.join(", "))
                }
            }
            Self::Option(Some(value)) => format!("Some({})", value.render_with_budget(remaining)),
            Self::Option(None) => "None".to_owned(),
            Self::Result { ok: true, value } => {
                format!("Ok({})", value.render_with_budget(remaining))
            }
            Self::Result { ok: false, value } => {
                format!("Err({})", value.render_with_budget(remaining))
            }
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
        let mut diagnostic = Diagnostic::new(self.code, self.message.clone(), self.span);
        diagnostic.stack = self.stack.clone();
        diagnostic
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExecutionOutput {
    pub value: Value,
    pub stdout: String,
}

/// Observable progress emitted by an explicitly enabled runtime event sink.
///
/// Events expose the bounded CPU training profiles implemented by the runtime.
/// They are a stable CLI stream, not a general distributed telemetry protocol.
#[derive(Clone, Debug, PartialEq)]
pub enum RuntimeEvent {
    Started {
        model: String,
        train_path: String,
        test_path: String,
        train_sha256: String,
        test_sha256: String,
        train_samples: usize,
        test_samples: usize,
        steps: i64,
        seed: i64,
        device: String,
        dtype: String,
        optimizer: String,
        learning_rate: f64,
        parameter_count: usize,
    },
    Step {
        model: String,
        step: i64,
        steps: i64,
        updates: usize,
        loss: f64,
        loss_delta: f64,
    },
    Completed {
        model: String,
        steps: i64,
        updates: usize,
        seed: i64,
        device: String,
        dtype: String,
        optimizer: String,
        learning_rate: f64,
        parameter_count: usize,
        initial_loss: f64,
        final_loss: f64,
        loss_delta: f64,
        loss_decreased: bool,
        holdout_mae_observed: f64,
        weight: f64,
        bias: f64,
    },
    GpuStarted {
        model: String,
        train_path: String,
        test_path: String,
        train_sha256: String,
        test_sha256: String,
        train_samples: usize,
        steps: usize,
        seed: i64,
        device: String,
        dtype: String,
        optimizer: String,
        learning_rate: f32,
        parameter_count: usize,
        adapter: String,
        backend: String,
    },
    GpuCompleted {
        model: String,
        steps: usize,
        seed: i64,
        device: String,
        dtype: String,
        optimizer: String,
        learning_rate: f32,
        parameter_count: usize,
        initial_loss: f32,
        final_loss: f32,
        loss_decreased: bool,
        holdout_mae_observed: f32,
        weight: f32,
        bias: f32,
        adapter: String,
        backend: String,
        dispatches: usize,
        readbacks: usize,
    },
    MlpStarted {
        model: String,
        train_path: String,
        test_path: String,
        checkpoint_path: String,
        train_sha256: String,
        test_sha256: String,
        checkpoint_before_sha256: Option<String>,
        train_samples: usize,
        test_samples: usize,
        steps: i64,
        seed: i64,
        device: String,
        dtype: String,
        optimizer: String,
        learning_rate: f32,
        batch_size: usize,
        hidden_size: usize,
        parameter_count: usize,
        resumed_from: i64,
    },
    MlpStep {
        model: String,
        step: i64,
        steps: i64,
        updates: usize,
        epoch: usize,
        loss: f32,
        evaluation_loss: f32,
    },
    MlpCompleted {
        model: String,
        steps: i64,
        updates: usize,
        seed: i64,
        device: String,
        dtype: String,
        optimizer: String,
        learning_rate: f32,
        batch_size: usize,
        hidden_size: usize,
        parameter_count: usize,
        initial_loss: f32,
        final_loss: f32,
        evaluation_loss: f32,
        loss_decreased: bool,
        resumed_from: i64,
        checkpoint_sha256: String,
    },
    InferenceStarted {
        model: String,
        checkpoint_path: String,
        input_path: String,
        checkpoint_sha256: String,
        input_sha256: String,
        samples: usize,
        input_size: usize,
        output_size: usize,
        checkpoint_step: i64,
    },
    InferenceCompleted {
        model: String,
        samples: usize,
        input_size: usize,
        output_size: usize,
        checkpoint_sha256: String,
        input_sha256: String,
        checkpoint_step: i64,
    },
    /// User-owned training progress emitted by a `.sely` loop.
    ///
    /// This is deliberately small: the model policy stays in source while the
    /// CLI owns delivery, flushing and optional receipt chaining.
    Progress {
        operation: String,
        step: i64,
        total: i64,
        metric: f64,
    },
    /// User-owned audit record emitted by a `.sely` workflow.
    ///
    /// The payload is structured JSON and is delivered through the same CLI
    /// sink as training events without granting any additional capability.
    Audit { kind: String, payload: JsonValue },
}

/// Receives runtime events while execution is in progress.
pub trait EventSink {
    fn emit(&mut self, event: RuntimeEvent) -> Result<(), String>;
}

struct NullEventSink;

impl EventSink for NullEventSink {
    fn emit(&mut self, _event: RuntimeEvent) -> Result<(), String> {
        Ok(())
    }
}

/// Execute a semantically valid program and return explicit program output separately
/// from the final value.
#[allow(dead_code)]
pub fn execute_with_output(
    program: &source_parser::Program,
) -> Result<ExecutionOutput, RuntimeError> {
    execute_with_context(program, Path::new("."), 128, 4)
}

pub fn execute_with_context(
    program: &source_parser::Program,
    base_dir: &Path,
    json_max_bytes: usize,
    json_max_depth: usize,
) -> Result<ExecutionOutput, RuntimeError> {
    let mut sink = NullEventSink;
    execute_with_context_and_events(program, base_dir, json_max_bytes, json_max_depth, &mut sink)
}

pub fn execute_with_context_and_events(
    program: &source_parser::Program,
    base_dir: &Path,
    json_max_bytes: usize,
    json_max_depth: usize,
    event_sink: &mut dyn EventSink,
) -> Result<ExecutionOutput, RuntimeError> {
    let lowered = crate::ir::lower(program);
    execute_ir_with_capabilities_and_training_limits(
        &lowered,
        base_dir,
        json_max_bytes,
        json_max_depth,
        MAX_TRAINING_DATASET_BYTES,
        32,
        &[],
        &[],
        event_sink,
    )
}

#[allow(dead_code)]
pub fn execute_with_capabilities_and_events(
    program: &source_parser::Program,
    base_dir: &Path,
    json_max_bytes: usize,
    json_max_depth: usize,
    read_roots: &[PathBuf],
    write_roots: &[PathBuf],
    event_sink: &mut dyn EventSink,
) -> Result<ExecutionOutput, RuntimeError> {
    let lowered = crate::ir::lower(program);
    execute_ir_with_capabilities_and_events(
        &lowered,
        base_dir,
        json_max_bytes,
        json_max_depth,
        read_roots,
        write_roots,
        event_sink,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn execute_with_capabilities_and_training_limits(
    program: &source_parser::Program,
    base_dir: &Path,
    json_max_bytes: usize,
    json_max_depth: usize,
    training_max_bytes: usize,
    training_max_depth: usize,
    read_roots: &[PathBuf],
    write_roots: &[PathBuf],
    event_sink: &mut dyn EventSink,
) -> Result<ExecutionOutput, RuntimeError> {
    let lowered = crate::ir::lower(program);
    execute_ir_with_capabilities_and_training_limits(
        &lowered,
        base_dir,
        json_max_bytes,
        json_max_depth,
        training_max_bytes,
        training_max_depth,
        read_roots,
        write_roots,
        event_sink,
    )
}

#[allow(dead_code)]
pub fn execute_ir_with_context(
    program: &Program,
    base_dir: &Path,
    json_max_bytes: usize,
    json_max_depth: usize,
) -> Result<ExecutionOutput, RuntimeError> {
    let mut sink = NullEventSink;
    execute_ir_with_context_and_events(program, base_dir, json_max_bytes, json_max_depth, &mut sink)
}

pub fn execute_ir_with_context_and_events(
    program: &Program,
    base_dir: &Path,
    json_max_bytes: usize,
    json_max_depth: usize,
    event_sink: &mut dyn EventSink,
) -> Result<ExecutionOutput, RuntimeError> {
    execute_ir_with_capabilities_and_training_limits(
        program,
        base_dir,
        json_max_bytes,
        json_max_depth,
        MAX_TRAINING_DATASET_BYTES,
        32,
        &[],
        &[],
        event_sink,
    )
}

#[allow(dead_code)]
pub fn execute_ir_with_capabilities_and_events(
    program: &Program,
    base_dir: &Path,
    json_max_bytes: usize,
    json_max_depth: usize,
    read_roots: &[PathBuf],
    write_roots: &[PathBuf],
    event_sink: &mut dyn EventSink,
) -> Result<ExecutionOutput, RuntimeError> {
    execute_ir_with_capabilities_and_training_limits(
        program,
        base_dir,
        json_max_bytes,
        json_max_depth,
        json_max_bytes,
        json_max_depth,
        read_roots,
        write_roots,
        event_sink,
    )
}

#[allow(clippy::too_many_arguments)]
fn execute_ir_with_capabilities_and_training_limits(
    program: &Program,
    base_dir: &Path,
    json_max_bytes: usize,
    json_max_depth: usize,
    training_max_bytes: usize,
    training_max_depth: usize,
    read_roots: &[PathBuf],
    write_roots: &[PathBuf],
    event_sink: &mut dyn EventSink,
) -> Result<ExecutionOutput, RuntimeError> {
    Interpreter::new(
        program,
        base_dir,
        json_max_bytes,
        json_max_depth,
        training_max_bytes,
        training_max_depth,
        read_roots,
        write_roots,
        event_sink,
    )
    .execute()
}

#[derive(Clone, Debug)]
enum Control {
    Return(Value),
    Runtime(RuntimeError),
}

type Eval<T> = Result<T, Control>;

struct Interpreter<'a, 'event> {
    program: &'a Program,
    functions: HashMap<String, Function>,
    records: HashMap<String, Record>,
    enums: HashMap<String, EnumDecl>,
    globals: HashMap<String, Value>,
    scopes: Vec<HashMap<String, Value>>,
    call_stack: Vec<String>,
    io_print_imported: bool,
    stdlib_imports: Vec<String>,
    output: String,
    base_dir: PathBuf,
    read_roots: Vec<PathBuf>,
    write_roots: Vec<PathBuf>,
    json_max_bytes: usize,
    json_max_depth: usize,
    training_max_bytes: usize,
    training_max_depth: usize,
    stdin_json: Option<Result<JsonValue, String>>,
    event_sink: &'event mut dyn EventSink,
}

impl<'a, 'event> Interpreter<'a, 'event> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        program: &'a Program,
        base_dir: &Path,
        json_max_bytes: usize,
        json_max_depth: usize,
        training_max_bytes: usize,
        training_max_depth: usize,
        read_roots: &[PathBuf],
        write_roots: &[PathBuf],
        event_sink: &'event mut dyn EventSink,
    ) -> Self {
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
        let io_print_imported = program.items.iter().any(|item| {
            matches!(
                item,
                Item::Directive(directive)
                    if directive.keyword == "from"
                        && directive.text.split_whitespace().collect::<Vec<_>>()
                            == ["io", "import", "print"]
            )
        });
        let stdlib_imports = program
            .items
            .iter()
            .filter_map(|item| {
                let Item::Directive(directive) = item else {
                    return None;
                };
                (directive.keyword == "from").then(|| directive.text.clone())
            })
            .flat_map(|text| {
                let mut parts = text.split_whitespace();
                let _module = parts.next();
                let _import = parts.next();
                parts
                    .map(|part| part.trim_end_matches(',').to_owned())
                    .collect::<Vec<_>>()
            })
            .collect();
        Self {
            program,
            functions,
            records,
            enums,
            globals: HashMap::new(),
            scopes: Vec::new(),
            call_stack: Vec::new(),
            io_print_imported,
            stdlib_imports,
            output: String::new(),
            base_dir: base_dir.to_path_buf(),
            read_roots: read_roots.to_vec(),
            write_roots: write_roots.to_vec(),
            json_max_bytes,
            json_max_depth,
            training_max_bytes,
            training_max_depth,
            stdin_json: None,
            event_sink,
        }
    }

    fn execute(mut self) -> Result<ExecutionOutput, RuntimeError> {
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
        let value = self
            .call_named("main", Vec::new(), self.program.span)
            .map_err(Self::runtime_error)?;
        Ok(ExecutionOutput {
            value,
            stdout: self.output,
        })
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

    fn eval_match(&mut self, statement: &crate::ir::MatchStatement) -> Eval<()> {
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
                    return Ok(self.constructor_binding_matches(name, value));
                }
                bindings.push((name.clone(), value.clone()));
                Ok(true)
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
        match value {
            Value::Option(_) => name == "None",
            Value::Enum {
                name: enum_name, ..
            } => self.enums.get(enum_name).is_some_and(|definition| {
                definition
                    .variants
                    .iter()
                    .any(|variant| variant.name == name && variant.fields.is_empty())
            }),
            _ => false,
        }
    }

    fn constructor_binding_matches(&self, name: &str, value: &Value) -> bool {
        matches!(value, Value::Option(None) if name == "None")
            || matches!(value, Value::Enum { variant, fields, .. } if variant == name && fields.is_empty())
    }

    fn eval_expr(&mut self, expression: &Expr) -> Eval<Value> {
        match expression {
            Expr::Name { name, .. } if name == "None" => Ok(Value::Option(None)),
            Expr::Name { name, span } => {
                if let Some(value) = self.lookup(name) {
                    return Ok(value);
                }
                let variants = self
                    .enums
                    .iter()
                    .filter_map(|(enum_name, definition)| {
                        definition
                            .variants
                            .iter()
                            .find(|variant| variant.name == *name)
                            .map(|variant| (enum_name.clone(), variant.fields.len()))
                    })
                    .collect::<Vec<_>>();
                if variants.len() == 1 && variants[0].1 == 0 {
                    return Ok(Value::Enum {
                        name: variants[0].0.clone(),
                        variant: name.clone(),
                        fields: Vec::new(),
                    });
                }
                let message = if variants.len() > 1 {
                    format!("enum variant `{name}` is ambiguous; qualify it")
                } else if variants.len() == 1 {
                    format!("enum variant `{name}` requires constructor arguments")
                } else {
                    format!("runtime name `{name}` is unavailable")
                };
                Err(Control::Runtime(self.runtime("SEL-R105", message, *span)))
            }
            Expr::Integer { text, span } => self.parse_i64(text, *span).map(Value::I64),
            Expr::Float { text, span } => self.parse_f64(text, *span).map(Value::F64),
            Expr::String { value, .. } => Ok(Value::Text(value.clone())),
            Expr::Boolean { value, .. } => Ok(Value::Bool(*value)),
            Expr::Null(_) => Ok(Value::Option(None)),
            Expr::List { values, span } => {
                if values.len() > MAX_LIST_ELEMENTS {
                    return Err(Control::Runtime(self.runtime(
                        "SEL-R133",
                        format!(
                            "list contains {} elements; maximum is {MAX_LIST_ELEMENTS}",
                            values.len()
                        ),
                        *span,
                    )));
                }
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
            Expr::NamedArgument { value, .. } => self.eval_expr(value),
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
            if name == "print" && self.io_print_imported {
                if arguments.len() != 1 {
                    return Err(Control::Runtime(self.runtime(
                        "SEL-R103",
                        "`print` expects one argument",
                        span,
                    )));
                }
                let value = self.eval_expr(&arguments[0])?;
                let Value::Text(value) = value else {
                    return Err(Control::Runtime(self.runtime(
                        "SEL-R203",
                        "print expects a Text value",
                        arguments[0].span(),
                    )));
                };
                self.output.push_str(&value);
                self.output.push('\n');
                return Ok(Value::Unit);
            }
            if self.stdlib_imports.iter().any(|import| import == name) {
                return self.eval_standard(name, arguments, span);
            }
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
            if is_named_error_constructor(name) {
                if arguments.len() != 1 {
                    return Err(Control::Runtime(self.runtime(
                        "SEL-R103",
                        format!("`{name}` expects one argument"),
                        span,
                    )));
                }
                return self.eval_expr(&arguments[0]);
            }
            if self.records.contains_key(name) {
                return self.construct_record(name, arguments, span);
            }
            let enum_names = self
                .enums
                .iter()
                .filter(|(_, definition)| {
                    definition
                        .variants
                        .iter()
                        .any(|variant| variant.name == *name)
                })
                .map(|(enum_name, _)| enum_name.clone())
                .collect::<Vec<_>>();
            if enum_names.len() == 1 {
                return self.construct_enum(&enum_names[0], name, arguments, span);
            }
            if enum_names.len() > 1 {
                return Err(Control::Runtime(self.runtime(
                    "SEL-R103",
                    format!("enum variant `{name}` is ambiguous; qualify it"),
                    span,
                )));
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

    fn eval_standard(&mut self, name: &str, arguments: &[Expr], span: Span) -> Eval<Value> {
        let values = self.eval_standard_arguments(name, arguments, span)?;
        match name {
            "read_stdin" => {
                if self.stdin_json.is_none() {
                    self.stdin_json =
                        Some(read_stdin_json(self.json_max_bytes, self.json_max_depth));
                }
                match self.stdin_json.as_ref() {
                    Some(Ok(value)) => Ok(Value::Result {
                        ok: true,
                        value: Box::new(Value::Json(value.clone())),
                    }),
                    Some(Err(message)) => Ok(json_error_result(message.clone())),
                    None => unreachable!("stdin result is initialized before reading"),
                }
            }
            "read_file" => {
                let Value::Text(relative) = values.first().cloned().unwrap_or(Value::Unit) else {
                    return Err(Control::Runtime(self.runtime(
                        "SEL-S004",
                        "json.read_file requires a Text path",
                        span,
                    )));
                };
                let path = self.resolve_read_path(&relative, span)?;
                match read_bounded_utf8(&path, self.json_max_bytes, "JSON input") {
                    Ok(source) => {
                        match json::parse(&source, self.json_max_bytes, self.json_max_depth) {
                            Ok(value) => Ok(Value::Result {
                                ok: true,
                                value: Box::new(Value::Json(value)),
                            }),
                            Err(error) => Ok(json_error_result(error.message)),
                        }
                    }
                    Err(error) => Ok(json_error_result(format!("failed to read JSON: {error}"))),
                }
            }
            "parse" => {
                let Value::Text(source) = values.first().cloned().unwrap_or(Value::Unit) else {
                    return Ok(json_error_result("JSON parse requires Text"));
                };
                match json::parse(&source, self.json_max_bytes, self.json_max_depth) {
                    Ok(value) => Ok(Value::Result {
                        ok: true,
                        value: Box::new(Value::Json(value)),
                    }),
                    Err(error) => Ok(json_error_result(error.message)),
                }
            }
            "encode" => match values.first() {
                Some(Value::Json(value)) => Ok(Value::Result {
                    ok: true,
                    value: Box::new(Value::Text(json::encode(value))),
                }),
                _ => Ok(json_error_result("encode requires a Json value")),
            },
            "get" => {
                let (Some(Value::Json(value)), Some(Value::List(path))) =
                    (values.first(), values.get(1))
                else {
                    return Ok(json_error_result("get requires Json and List[Text]"));
                };
                let path = path
                    .iter()
                    .filter_map(|value| match value {
                        Value::Text(value) => Some(value.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                match json::get(value, &path) {
                    Ok(value) => Ok(Value::Result {
                        ok: true,
                        value: Box::new(Value::Json(value.clone())),
                    }),
                    Err(error) => Ok(json_error_result(error.message)),
                }
            }
            "as_text" | "as_i64" | "as_f32" | "as_f64" | "as_bool" => {
                let Some(Value::Json(value)) = values.first() else {
                    return Ok(json_error_result("accessor requires a Json value"));
                };
                let converted = match (name, value) {
                    ("as_text", JsonValue::String(value)) => Some(Value::Text(value.clone())),
                    ("as_i64", JsonValue::Number(value)) => {
                        value.parse::<i64>().ok().map(Value::I64)
                    }
                    ("as_f32", JsonValue::Number(value)) => {
                        value.parse::<f64>().ok().map(Value::F64)
                    }
                    ("as_f64", JsonValue::Number(value)) => {
                        value.parse::<f64>().ok().map(Value::F64)
                    }
                    ("as_bool", JsonValue::Bool(value)) => Some(Value::Bool(*value)),
                    _ => None,
                };
                converted
                    .map(|value| Value::Result {
                        ok: true,
                        value: Box::new(value),
                    })
                    .map_or_else(
                        || {
                            Ok(json_error_result(format!(
                                "{name} received an incompatible JSON value"
                            )))
                        },
                        Ok,
                    )
            }
            "as_i64_list" | "as_f64_list" | "as_text_list" => {
                let Some(Value::Json(JsonValue::Array(values))) = values.first() else {
                    return Ok(json_error_result(format!("{name} requires a JSON array")));
                };
                if values.len() > MAX_LIST_ELEMENTS {
                    return Ok(json_error_result(format!(
                        "{name} exceeds the list element limit {MAX_LIST_ELEMENTS}"
                    )));
                }
                let mut converted = Vec::with_capacity(values.len());
                for value in values {
                    let converted_value = if name == "as_text_list" {
                        match value {
                            JsonValue::String(text) => Some(Value::Text(text.clone())),
                            _ => None,
                        }
                    } else {
                        let JsonValue::Number(number) = value else {
                            return Ok(json_error_result(format!(
                                "{name} requires an array of numbers"
                            )));
                        };
                        if name == "as_i64_list" {
                            number.parse::<i64>().ok().map(Value::I64)
                        } else {
                            number.parse::<f64>().ok().map(Value::F64)
                        }
                    };
                    let Some(converted_value) = converted_value else {
                        return Ok(json_error_result(format!(
                            "{name} received an incompatible JSON value"
                        )));
                    };
                    converted.push(converted_value);
                }
                Ok(Value::Result {
                    ok: true,
                    value: Box::new(Value::List(converted)),
                })
            }
            "len" => {
                let Some(value) = values.first() else {
                    return Err(Control::Runtime(self.runtime(
                        "SEL-R203",
                        "len requires a List or Text",
                        span,
                    )));
                };
                let length = match value {
                    Value::List(values) => values.len(),
                    Value::Text(text) => text.chars().count(),
                    _ => {
                        return Err(Control::Runtime(self.runtime(
                            "SEL-R203",
                            "len requires a List or Text",
                            span,
                        )));
                    }
                };
                let length = i64::try_from(length).map_err(|_| {
                    Control::Runtime(self.runtime("SEL-R203", "len result exceeds I64", span))
                })?;
                Ok(Value::I64(length))
            }
            "to_f64" => {
                let Some(value) = values.first() else {
                    return Err(Control::Runtime(self.runtime(
                        "SEL-R203",
                        "to_f64 requires a numeric value",
                        span,
                    )));
                };
                match value {
                    Value::I64(value) => Ok(Value::F64(*value as f64)),
                    Value::F64(value) if value.is_finite() => Ok(Value::F64(*value)),
                    _ => Err(Control::Runtime(self.runtime(
                        "SEL-R203",
                        "to_f64 requires a finite numeric value",
                        span,
                    ))),
                }
            }
            "from_text" => {
                let Some(Value::Text(value)) = values.first() else {
                    return Ok(json_error_result("from_text requires Text"));
                };
                Ok(Value::Json(JsonValue::String(value.clone())))
            }
            "from_i64" => {
                let Some(Value::I64(value)) = values.first() else {
                    return Ok(json_error_result("from_i64 requires I64"));
                };
                Ok(Value::Json(JsonValue::Number(value.to_string())))
            }
            "from_f64" => {
                let Some(Value::F64(value)) = values.first() else {
                    return Ok(json_error_result("from_f64 requires F64"));
                };
                if !value.is_finite() {
                    return Ok(json_error_result("from_f64 requires a finite value"));
                }
                Ok(Value::Json(JsonValue::Number(format_f64(*value))))
            }
            "from_f64_list" => {
                let Some(Value::List(values)) = values.first() else {
                    return Ok(json_error_result("from_f64_list requires List[F64]"));
                };
                let mut result = Vec::with_capacity(values.len());
                for value in values {
                    let Value::F64(value) = value else {
                        return Ok(json_error_result("from_f64_list requires List[F64]"));
                    };
                    if !value.is_finite() {
                        return Ok(json_error_result("from_f64_list requires finite values"));
                    }
                    result.push(JsonValue::Number(format_f64(*value)));
                }
                Ok(Value::Json(JsonValue::Array(result)))
            }
            "sha256" => {
                let Some(Value::Json(value)) = values.first() else {
                    return Ok(json_error_result("sha256 requires a Json value"));
                };
                Ok(Value::Text(sha256_hex(json::encode(value).as_bytes())))
            }
            "object" => {
                let (Some(Value::List(keys)), Some(Value::List(values))) =
                    (values.first(), values.get(1))
                else {
                    return Ok(json_error_result(
                        "object requires List[Text] keys and List[Json] values",
                    ));
                };
                if keys.len() != values.len() {
                    return Ok(json_error_result(
                        "object keys and values must have the same length",
                    ));
                }
                let mut object = BTreeMap::new();
                for (key, value) in keys.iter().zip(values) {
                    let Value::Text(key) = key else {
                        return Ok(json_error_result("object keys must be Text"));
                    };
                    let Value::Json(value) = value else {
                        return Ok(json_error_result("object values must be Json"));
                    };
                    if object.insert(key.clone(), value.clone()).is_some() {
                        return Ok(json_error_result(format!(
                            "object contains duplicate key `{key}`"
                        )));
                    }
                }
                Ok(Value::Result {
                    ok: true,
                    value: Box::new(Value::Json(JsonValue::Object(object))),
                })
            }
            "write_file" => {
                let (Some(Value::Text(relative)), Some(Value::Json(value))) =
                    (values.first(), values.get(1))
                else {
                    return Ok(json_error_result(
                        "write_file requires Text path and Json value",
                    ));
                };
                let path = self.resolve_write_path(relative, span)?;
                match write_json_atomic(&path, &json::encode(value)) {
                    Ok(()) => Ok(Value::Result {
                        ok: true,
                        value: Box::new(Value::Unit),
                    }),
                    Err(message) => Ok(json_error_result(message)),
                }
            }
            "values" => {
                let Some(Value::Tensor(tensor)) = values.first() else {
                    return Err(Control::Runtime(self.runtime(
                        "SEL-R203",
                        "values requires a Tensor[F32]",
                        span,
                    )));
                };
                let values = tensor
                    .values_f32()
                    .map_err(|error| Control::Runtime(self.tensor_error(error, span)))?
                    .into_iter()
                    .map(|value| Value::F64(value as f64))
                    .collect();
                Ok(Value::List(values))
            }
            "progress" => {
                let [
                    Value::Text(operation),
                    Value::I64(step),
                    Value::I64(total),
                    metric,
                ] = values.as_slice()
                else {
                    return Err(Control::Runtime(self.runtime(
                        "SEL-R203",
                        "progress requires Text, I64, I64 and F32/F64",
                        span,
                    )));
                };
                let metric = match metric {
                    Value::F64(value) => *value,
                    _ => {
                        return Err(Control::Runtime(self.runtime(
                            "SEL-R203",
                            "progress metric must be F32/F64",
                            span,
                        )));
                    }
                };
                if !metric.is_finite() {
                    return Err(Control::Runtime(self.runtime(
                        "SEL-R203",
                        "progress metric must be finite",
                        span,
                    )));
                }
                self.event_sink
                    .emit(RuntimeEvent::Progress {
                        operation: operation.clone(),
                        step: *step,
                        total: *total,
                        metric,
                    })
                    .map_err(|message| Control::Runtime(self.runtime("SEL-R203", message, span)))?;
                Ok(Value::Unit)
            }
            "emit" => {
                let [Value::Text(kind), Value::Json(payload)] = values.as_slice() else {
                    return Err(Control::Runtime(self.runtime(
                        "SEL-R203",
                        "audit.emit requires Text and Json",
                        span,
                    )));
                };
                if kind.is_empty() || kind.len() > 128 {
                    return Err(Control::Runtime(self.runtime(
                        "SEL-R203",
                        "audit.emit kind must contain 1 to 128 bytes",
                        span,
                    )));
                }
                self.event_sink
                    .emit(RuntimeEvent::Audit {
                        kind: kind.clone(),
                        payload: payload.clone(),
                    })
                    .map_err(|message| Control::Runtime(self.runtime("SEL-R203", message, span)))?;
                Ok(Value::Unit)
            }
            "ModelRequest" => {
                let [
                    Value::Text(model),
                    Value::Text(prompt),
                    Value::I64(max_output_tokens),
                    Value::I64(seed),
                ] = values.as_slice()
                else {
                    return Err(Control::Runtime(self.runtime(
                        "SEL-A001",
                        "ModelRequest received incompatible arguments",
                        span,
                    )));
                };
                Ok(Value::ModelRequest {
                    model: model.clone(),
                    prompt: prompt.clone(),
                    max_output_tokens: *max_output_tokens,
                    seed: *seed,
                })
            }
            "fixture_provider" => {
                if !matches!(values.first(), Some(Value::ModelRequest { .. })) {
                    return Err(Control::Runtime(self.runtime(
                        "SEL-A001",
                        "fixture_provider requires a ModelRequest",
                        span,
                    )));
                }
                Ok(Value::Result {
                    ok: true,
                    value: Box::new(Value::ModelResponse {
                        status: "ok".to_owned(),
                        text: "fixture response".to_owned(),
                    }),
                })
            }
            "slow_fixture_provider" => {
                let timeout = values.get(1).and_then(|value| match value {
                    Value::I64(value) => Some(*value),
                    _ => None,
                });
                if timeout.is_some_and(|timeout| timeout <= 1) {
                    return Err(Control::Runtime(self.runtime(
                        "SEL-A001",
                        "model request timeout",
                        span,
                    )));
                }
                Ok(Value::Result {
                    ok: true,
                    value: Box::new(Value::Text("fixture response".to_owned())),
                })
            }
            "gradcheck" => {
                let Some(Value::Function(function_name)) = values.first().cloned() else {
                    return Err(Control::Runtime(self.runtime(
                        "SEL-R203",
                        "gradcheck requires a callable function",
                        span,
                    )));
                };
                let Some(Value::F64(point)) = values.get(1) else {
                    return Err(Control::Runtime(self.runtime(
                        "SEL-R203",
                        "gradcheck requires an F64 point",
                        span,
                    )));
                };
                let analytic = autograd::differentiate_scalar(self.program, &function_name, *point)
                    .map_err(|error| {
                        Control::Runtime(self.runtime("SEL-R203", error.message, span))
                    })?;
                let epsilon = 1.0e-4;
                let plus =
                    self.call_named(&function_name, vec![Value::F64(*point + epsilon)], span)?;
                let minus =
                    self.call_named(&function_name, vec![Value::F64(*point - epsilon)], span)?;
                let (Value::F64(plus), Value::F64(minus)) = (plus, minus) else {
                    return Err(Control::Runtime(self.runtime(
                        "SEL-R203",
                        "gradcheck function must return F32",
                        span,
                    )));
                };
                let derivative = (plus - minus) / (2.0 * epsilon);
                if !derivative.is_finite() {
                    return Err(Control::Runtime(self.runtime(
                        "SEL-R203",
                        "gradcheck produced a non-finite finite-difference derivative",
                        span,
                    )));
                }
                let tolerance = 1.0e-3_f64.max(analytic.abs() * 1.0e-3);
                if (analytic - derivative).abs() > tolerance {
                    return Err(Control::Runtime(self.runtime(
                        "SEL-R203",
                        format!(
                            "gradcheck mismatch: reverse={analytic}, finite_difference={derivative}, tolerance={tolerance}"
                        ),
                        span,
                    )));
                }
                finite_f64(
                    analytic,
                    "gradcheck produced a non-finite derivative",
                    span,
                    self,
                )
            }
            "backward" => {
                if let Some(Value::F64(point)) = values.get(1) {
                    let Some(Value::Function(function_name)) = values.first().cloned() else {
                        return Err(Control::Runtime(self.runtime(
                            "SEL-R203",
                            "backward requires a callable function",
                            span,
                        )));
                    };
                    return autograd::differentiate_scalar(self.program, &function_name, *point)
                        .map(Value::F64)
                        .map_err(|error| {
                            Control::Runtime(self.runtime("SEL-R203", error.message, span))
                        });
                }
                let Some(Value::Tensor(tensor)) = values.get(1) else {
                    return Err(Control::Runtime(self.runtime(
                        "SEL-R203",
                        "backward requires an F64 scalar point or Tensor[F32] input",
                        span,
                    )));
                };
                let Some(Value::Function(function_name)) = values.first().cloned() else {
                    return Err(Control::Runtime(self.runtime(
                        "SEL-R203",
                        "backward requires a callable function",
                        span,
                    )));
                };
                tensor_autograd::differentiate_tensor(self.program, &function_name, tensor.clone())
                    .map(Value::Tensor)
                    .map_err(|error| {
                        Control::Runtime(self.runtime("SEL-R203", error.message, span))
                    })
            }
            "gradients" => {
                let Some(Value::Function(function_name)) = values.first().cloned() else {
                    return Err(Control::Runtime(self.runtime(
                        "SEL-R203",
                        "gradients requires a callable function",
                        span,
                    )));
                };
                let inputs = values
                    .iter()
                    .skip(1)
                    .map(|value| match value {
                        Value::Tensor(tensor) => Ok(tensor.clone()),
                        _ => Err("gradients requires Tensor[F32] inputs"),
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|message| Control::Runtime(self.runtime("SEL-R203", message, span)))?;
                let gradients =
                    tensor_autograd::differentiate_tensors(self.program, &function_name, inputs)
                        .map_err(|error| {
                            Control::Runtime(self.runtime("SEL-R203", error.message, span))
                        })?;
                Ok(Value::List(
                    gradients.into_iter().map(Value::Tensor).collect(),
                ))
            }
            "range" => {
                let Some(Value::I64(stop)) = values.first() else {
                    return Err(Control::Runtime(self.runtime(
                        "SEL-R203",
                        "range stop must be I64",
                        span,
                    )));
                };
                if *stop < 0 || *stop > 1_000_000 {
                    return Err(Control::Runtime(self.runtime(
                        "SEL-R203",
                        "range stop must be between 0 and 1000000",
                        span,
                    )));
                }
                Ok(Value::List((0..*stop).map(Value::I64).collect()))
            }
            "split_whitespace" => {
                let Some(Value::Text(text)) = values.first() else {
                    return Err(Control::Runtime(self.runtime(
                        "SEL-R203",
                        "split_whitespace requires Text",
                        span,
                    )));
                };
                let tokens = text
                    .split_whitespace()
                    .map(|token| Value::Text(token.to_owned()))
                    .collect::<Vec<_>>();
                if tokens.len() > MAX_LIST_ELEMENTS {
                    return Err(Control::Runtime(self.runtime(
                        "SEL-R203",
                        format!(
                            "split_whitespace exceeds the list element limit {MAX_LIST_ELEMENTS}"
                        ),
                        span,
                    )));
                }
                Ok(Value::List(tokens))
            }
            "train_linear" => {
                let (Some(Value::Text(train)), Some(Value::Text(test))) =
                    (values.first(), values.get(1))
                else {
                    return Err(Control::Runtime(self.runtime(
                        "SEL-R203",
                        "train_linear requires dataset paths",
                        span,
                    )));
                };
                let seed = values.get(2).and_then(|value| match value {
                    Value::I64(value) => Some(*value),
                    _ => None,
                });
                let steps = values.get(3).and_then(|value| match value {
                    Value::I64(value) => Some(*value),
                    _ => None,
                });
                let train_file = self.resolve_read_path(train, span)?;
                let test_file = self.resolve_read_path(test, span)?;
                let json_max_bytes = self.training_max_bytes;
                let json_max_depth = self.training_max_depth;
                let result = train_linear_json(
                    &train_file,
                    &test_file,
                    train,
                    test,
                    seed.unwrap_or(0),
                    steps.unwrap_or(100),
                    json_max_bytes,
                    json_max_depth,
                    &mut *self.event_sink,
                )
                .map(Value::Json)
                .map_err(|message| Control::Runtime(self.runtime("SEL-R203", message, span)))?;
                Ok(result)
            }
            "gpu" | "train_linear_gpu" => {
                let (Some(Value::Text(train)), Some(Value::Text(test))) =
                    (values.first(), values.get(1))
                else {
                    return Err(Control::Runtime(self.runtime(
                        "SEL-R203",
                        "train_linear_gpu requires dataset paths",
                        span,
                    )));
                };
                let seed = values.get(2).and_then(|value| match value {
                    Value::I64(value) => Some(*value),
                    _ => None,
                });
                let steps = values.get(3).and_then(|value| match value {
                    Value::I64(value) => Some(*value),
                    _ => None,
                });
                let train_file = self.resolve_read_path(train, span)?;
                let test_file = self.resolve_read_path(test, span)?;
                let result = train_linear_gpu_json(
                    &train_file,
                    &test_file,
                    train,
                    test,
                    seed.unwrap_or(0),
                    steps.unwrap_or(100),
                    self.training_max_bytes,
                    self.training_max_depth,
                    &mut *self.event_sink,
                )
                .map(Value::Json)
                .map_err(|message| Control::Runtime(self.runtime("SEL-R203", message, span)))?;
                Ok(result)
            }
            "train_mlp" => {
                let strings = values
                    .iter()
                    .take(3)
                    .map(|value| match value {
                        Value::Text(value) => Ok(value.as_str()),
                        _ => Err("train_mlp dataset and checkpoint paths must be Text"),
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|message| Control::Runtime(self.runtime("SEL-R203", message, span)))?;
                let number = |index: usize, label: &str| -> Result<i64, String> {
                    match values.get(index) {
                        Some(Value::I64(value)) => Ok(*value),
                        _ => Err(format!("train_mlp {label} must be I64")),
                    }
                };
                let learning_rate = match values.get(7) {
                    Some(Value::F64(value)) if value.is_finite() => *value as f32,
                    Some(Value::I64(value)) => *value as f32,
                    _ => {
                        return Err(Control::Runtime(self.runtime(
                            "SEL-R203",
                            "train_mlp learning_rate must be a finite F64",
                            span,
                        )));
                    }
                };
                if !learning_rate.is_finite() || learning_rate <= 0.0 {
                    return Err(Control::Runtime(self.runtime(
                        "SEL-R203",
                        "train_mlp learning_rate must be positive and finite",
                        span,
                    )));
                }
                let seed = number(3, "seed")
                    .map_err(|message| Control::Runtime(self.runtime("SEL-R203", message, span)))?;
                let steps = number(4, "steps")
                    .map_err(|message| Control::Runtime(self.runtime("SEL-R203", message, span)))?;
                let batch_size = number(5, "batch_size")
                    .map_err(|message| Control::Runtime(self.runtime("SEL-R203", message, span)))?;
                let hidden_size = number(6, "hidden_size")
                    .map_err(|message| Control::Runtime(self.runtime("SEL-R203", message, span)))?;
                let train_file = self.resolve_read_path(strings[0], span)?;
                let test_file = self.resolve_read_path(strings[1], span)?;
                let checkpoint_file = self.resolve_write_path(strings[2], span)?;
                let json_max_bytes = self.training_max_bytes;
                let json_max_depth = self.training_max_depth;
                let result = train_mlp_json(
                    MlpTrainingRequest {
                        train_file: &train_file,
                        test_file: &test_file,
                        checkpoint_file: &checkpoint_file,
                        train_path: strings[0],
                        test_path: strings[1],
                        checkpoint_path: strings[2],
                        seed,
                        steps,
                        batch_size,
                        hidden_size,
                        learning_rate,
                    },
                    json_max_bytes,
                    json_max_depth,
                    &mut *self.event_sink,
                )
                .map(Value::Json)
                .map_err(|message| Control::Runtime(self.runtime("SEL-R203", message, span)))?;
                Ok(result)
            }
            "predict_mlp" => {
                let (Some(Value::Text(checkpoint)), Some(Value::Text(inputs))) =
                    (values.first(), values.get(1))
                else {
                    return Err(Control::Runtime(self.runtime(
                        "SEL-R203",
                        "predict_mlp requires checkpoint and input paths",
                        span,
                    )));
                };
                let checkpoint_file = self.resolve_read_path(checkpoint, span)?;
                let input_file = self.resolve_read_path(inputs, span)?;
                let result = predict_mlp_json(
                    &checkpoint_file,
                    &input_file,
                    checkpoint,
                    inputs,
                    self.training_max_bytes,
                    self.training_max_depth,
                    &mut *self.event_sink,
                )
                .map(Value::Json)
                .map_err(|message| Control::Runtime(self.runtime("SEL-R203", message, span)))?;
                Ok(result)
            }
            "load_checkpoint" => {
                let Some(Value::Text(checkpoint)) = values.first() else {
                    return Err(Control::Runtime(self.runtime(
                        "SEL-K001",
                        "checkpoint path must be Text",
                        span,
                    )));
                };
                let path = self.resolve_read_path(checkpoint, span)?;
                let max_bytes = self.json_max_bytes.min(MAX_CHECKPOINT_BYTES);
                let source = read_bounded_utf8(&path, max_bytes, "checkpoint")
                    .map_err(|error| Control::Runtime(self.runtime("SEL-K001", error, span)))?;
                validate_checkpoint(&source, max_bytes, self.json_max_depth)
                    .map(|value| Value::Result {
                        ok: true,
                        value: Box::new(Value::Json(value)),
                    })
                    .map_err(|message| Control::Runtime(self.runtime("SEL-K001", message, span)))
            }
            _ => Err(Control::Runtime(self.runtime(
                "SEL-R103",
                format!("standard function `{name}` is unavailable"),
                span,
            ))),
        }
    }

    fn eval_standard_arguments(
        &mut self,
        name: &str,
        arguments: &[Expr],
        span: Span,
    ) -> Eval<Vec<Value>> {
        let parameter_names: Option<&[&str]> = match name {
            "train_linear" | "train_linear_gpu" | "gpu" => {
                Some(&["train", "test", "seed", "steps"])
            }
            "train_mlp" => Some(&[
                "train",
                "test",
                "checkpoint",
                "seed",
                "steps",
                "batch_size",
                "hidden_size",
                "learning_rate",
            ]),
            "predict_mlp" => Some(&["checkpoint", "inputs"]),
            _ => None,
        };
        let Some(parameter_names) = parameter_names else {
            return arguments
                .iter()
                .map(|argument| self.eval_expr(argument))
                .collect();
        };
        let mut values: Vec<Option<Value>> = (0..parameter_names.len()).map(|_| None).collect();
        let mut next_positional = 0;
        for argument in arguments {
            let (index, value_expression) = match argument {
                Expr::NamedArgument {
                    name: parameter,
                    value,
                    ..
                } => {
                    let Some(index) = parameter_names
                        .iter()
                        .position(|candidate| candidate == parameter)
                    else {
                        return Err(Control::Runtime(self.runtime(
                            "SEL-R103",
                            format!(
                                "standard function `{name}` has no named parameter `{parameter}`"
                            ),
                            argument.span(),
                        )));
                    };
                    (index, value.as_ref())
                }
                _ => {
                    while next_positional < values.len() && values[next_positional].is_some() {
                        next_positional += 1;
                    }
                    if next_positional >= values.len() {
                        return Err(Control::Runtime(self.runtime(
                            "SEL-R103",
                            format!("standard function `{name}` received too many arguments"),
                            span,
                        )));
                    }
                    let index = next_positional;
                    next_positional += 1;
                    (index, argument)
                }
            };
            if values[index].is_some() {
                return Err(Control::Runtime(self.runtime(
                    "SEL-R103",
                    format!(
                        "standard function `{name}` received duplicate argument `{}`",
                        parameter_names[index]
                    ),
                    argument.span(),
                )));
            }
            values[index] = Some(self.eval_expr(value_expression)?);
        }
        if values.iter().any(Option::is_none) {
            return Err(Control::Runtime(self.runtime(
                "SEL-R103",
                format!("standard function `{name}` is missing an argument"),
                span,
            )));
        }
        Ok(values.into_iter().map(Option::unwrap).collect())
    }

    fn resolve_read_path(&self, relative: &str, span: Span) -> Eval<PathBuf> {
        let path = Path::new(relative);
        if path.is_absolute()
            || path
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(Control::Runtime(self.runtime(
                "SEL-S004",
                "path is outside the granted read root",
                span,
            )));
        }
        let candidate = self.base_dir.join(path);
        if !self
            .read_roots
            .iter()
            .any(|root| candidate.starts_with(root))
        {
            return Err(Control::Runtime(self.runtime(
                "SEL-S004",
                "path is not inside a declared read capability",
                span,
            )));
        }
        match std::fs::canonicalize(&candidate) {
            Ok(canonical)
                if self
                    .read_roots
                    .iter()
                    .any(|root| canonical.starts_with(root)) =>
            {
                Ok(canonical)
            }
            Ok(_) => Err(Control::Runtime(self.runtime(
                "SEL-S004",
                "path escapes a declared read capability through a symbolic link",
                span,
            ))),
            Err(_) => Err(Control::Runtime(self.runtime(
                "SEL-S004",
                "path could not be canonicalized within a declared read capability",
                span,
            ))),
        }
    }

    fn resolve_write_path(&self, relative: &str, span: Span) -> Eval<PathBuf> {
        let path = Path::new(relative);
        if path.as_os_str().is_empty()
            || path.is_absolute()
            || path
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(Control::Runtime(self.runtime(
                "SEL-S004",
                "path is outside the granted write root",
                span,
            )));
        }
        let candidate = self.base_dir.join(path);
        let Some(root) = self
            .write_roots
            .iter()
            .find(|root| candidate.starts_with(root))
        else {
            return Err(Control::Runtime(self.runtime(
                "SEL-S004",
                "path is not inside a declared write capability",
                span,
            )));
        };
        let parent = candidate.parent().unwrap_or(root);
        let canonical_parent = std::fs::canonicalize(parent).map_err(|_| {
            Control::Runtime(self.runtime(
                "SEL-S004",
                "write path parent could not be canonicalized within a declared capability",
                span,
            ))
        })?;
        if !canonical_parent.starts_with(root) {
            return Err(Control::Runtime(self.runtime(
                "SEL-S004",
                "write path escapes a declared capability through a symbolic link",
                span,
            )));
        }
        if candidate.exists() {
            let canonical = std::fs::canonicalize(&candidate).map_err(|_| {
                Control::Runtime(self.runtime(
                    "SEL-S004",
                    "existing write path could not be canonicalized",
                    span,
                ))
            })?;
            if !canonical.starts_with(root) {
                return Err(Control::Runtime(self.runtime(
                    "SEL-S004",
                    "write path escapes a declared capability through a symbolic link",
                    span,
                )));
            }
        }
        Ok(candidate)
    }

    fn eval_builtin(&mut self, builtin: Builtin, arguments: &[Expr], span: Span) -> Eval<Value> {
        let expected = match builtin {
            Builtin::TensorF32 | Builtin::TensorI64 | Builtin::Reshape | Builtin::Matmul => 2,
            Builtin::ZerosF32 | Builtin::ZerosI64 => 1,
            Builtin::TensorShape
            | Builtin::TensorRank
            | Builtin::TensorElements
            | Builtin::TensorDtype
            | Builtin::TensorToF32
            | Builtin::TensorToI64
            | Builtin::TensorItemF32 => 1,
            Builtin::TensorGetF32 | Builtin::TensorGetI64 => 2,
            Builtin::Sum | Builtin::Mean => 0,
            Builtin::Transpose | Builtin::Relu | Builtin::Exp | Builtin::Log => 1,
            Builtin::Softmax | Builtin::LogSumExp | Builtin::SumToShape => 2,
            Builtin::CrossEntropy => 3,
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
            Builtin::TensorToF32 => tensor_value(&values[0], span, self)?
                .to_f32()
                .map(Value::Tensor)
                .map_err(|error| Control::Runtime(self.tensor_error(error, span))),
            Builtin::TensorToI64 => tensor_value(&values[0], span, self)?
                .to_i64()
                .map(Value::Tensor)
                .map_err(|error| Control::Runtime(self.tensor_error(error, span))),
            Builtin::TensorItemF32 => {
                let tensor = tensor_value(&values[0], span, self)?;
                tensor
                    .scalar_f32()
                    .map(|value| Value::F64(value as f64))
                    .map_err(|error| Control::Runtime(self.tensor_error(error, span)))
            }
            Builtin::TensorGetF32 => {
                let tensor = tensor_value(&values[0], span, self)?;
                let indices = shape_values(&values[1], span, self)?;
                tensor
                    .get_f32(&indices)
                    .map(|value| Value::F64(value as f64))
                    .map_err(|error| Control::Runtime(self.tensor_error(error, span)))
            }
            Builtin::TensorGetI64 => {
                let tensor = tensor_value(&values[0], span, self)?;
                let indices = shape_values(&values[1], span, self)?;
                tensor
                    .get_i64(&indices)
                    .map(Value::I64)
                    .map_err(|error| Control::Runtime(self.tensor_error(error, span)))
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
            Builtin::Transpose => tensor_value(&values[0], span, self)?
                .transpose()
                .map(Value::Tensor)
                .map_err(|error| Control::Runtime(self.tensor_error(error, span))),
            Builtin::Relu => tensor_value(&values[0], span, self)?
                .relu()
                .map(Value::Tensor)
                .map_err(|error| Control::Runtime(self.tensor_error(error, span))),
            Builtin::Exp => tensor_value(&values[0], span, self)?
                .exp()
                .map(Value::Tensor)
                .map_err(|error| Control::Runtime(self.tensor_error(error, span))),
            Builtin::Log => tensor_value(&values[0], span, self)?
                .log()
                .map(Value::Tensor)
                .map_err(|error| Control::Runtime(self.tensor_error(error, span))),
            Builtin::Softmax => {
                let tensor = tensor_value(&values[0], span, self)?;
                let axis = self.reduction_axis(&values[1], tensor.rank(), span)?;
                tensor
                    .softmax(axis)
                    .map(Value::Tensor)
                    .map_err(|error| Control::Runtime(self.tensor_error(error, span)))
            }
            Builtin::LogSumExp => {
                let tensor = tensor_value(&values[0], span, self)?;
                let axis = self.reduction_axis(&values[1], tensor.rank(), span)?;
                tensor
                    .logsumexp(axis)
                    .map(Value::Tensor)
                    .map_err(|error| Control::Runtime(self.tensor_error(error, span)))
            }
            Builtin::CrossEntropy => {
                let logits = tensor_value(&values[0], span, self)?;
                let targets = tensor_value(&values[1], span, self)?;
                let axis = self.reduction_axis(&values[2], logits.rank(), span)?;
                logits
                    .cross_entropy(targets, axis)
                    .map(Value::Tensor)
                    .map_err(|error| Control::Runtime(self.tensor_error(error, span)))
            }
            Builtin::SumToShape => {
                let tensor = tensor_value(&values[0], span, self)?;
                let shape = shape_values(&values[1], span, self)?;
                tensor
                    .sum_to_shape(&shape)
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
        if let Value::ModelResponse { status, text } = object {
            return match name {
                "status" => Ok(Value::Text(status)),
                "text" => Ok(Value::Text(text)),
                _ => Err(Control::Runtime(self.runtime(
                    "SEL-R115",
                    format!("ModelResponse has no field `{name}`"),
                    span,
                ))),
            };
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
                let length = left.len().checked_add(right.len()).ok_or_else(|| {
                    Control::Runtime(self.runtime("SEL-R133", "list length overflowed", span))
                })?;
                if length > MAX_LIST_ELEMENTS {
                    return Err(Control::Runtime(self.runtime(
                        "SEL-R133",
                        format!("list contains {length} elements; maximum is {MAX_LIST_ELEMENTS}"),
                        span,
                    )));
                }
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

fn finite_f64(
    value: f64,
    message: &str,
    span: Span,
    interpreter: &Interpreter<'_, '_>,
) -> Eval<Value> {
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
    interpreter: &Interpreter<'_, '_>,
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

fn shape_values(value: &Value, span: Span, interpreter: &Interpreter<'_, '_>) -> Eval<Vec<i64>> {
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
    interpreter: &Interpreter<'_, '_>,
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
        Value::Json(_) => "Json",
        Value::ModelRequest { .. } => "ModelRequest",
        Value::ModelResponse { .. } => "ModelResponse",
        Value::List(_) => "List",
        Value::Tensor(_) => "Tensor",
        Value::Record { .. } => "Record",
        Value::Enum { .. } => "Enum",
        Value::Option(_) => "Option",
        Value::Result { .. } => "Result",
        Value::Function(_) => "Function",
    }
}

fn json_error_result(message: impl Into<String>) -> Value {
    Value::Result {
        ok: false,
        value: Box::new(Value::Text(message.into())),
    }
}

fn read_stdin_json(max_bytes: usize, max_depth: usize) -> Result<JsonValue, String> {
    let mut bytes = Vec::new();
    let limit = max_bytes.saturating_add(1) as u64;
    std::io::stdin()
        .take(limit)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("failed to read JSON from stdin: {error}"))?;
    if bytes.len() > max_bytes {
        return Err(format!(
            "JSON stdin input exceeds the byte limit of {max_bytes} bytes"
        ));
    }
    let source =
        String::from_utf8(bytes).map_err(|_| "JSON stdin input is not valid UTF-8".to_owned())?;
    json::parse(&source, max_bytes, max_depth)
        .map_err(|error| format!("invalid JSON stdin input: {}", error.message))
}

pub fn value_to_json(value: &Value) -> JsonValue {
    match value {
        Value::Unit => JsonValue::Null,
        Value::Bool(value) => JsonValue::Bool(*value),
        Value::I64(value) => JsonValue::Number(value.to_string()),
        Value::F64(value) => JsonValue::Number(format_f64(*value)),
        Value::Text(value) => JsonValue::String(value.clone()),
        Value::Json(value) => value.clone(),
        Value::ModelRequest {
            model,
            prompt,
            max_output_tokens,
            seed,
        } => JsonValue::Object(std::collections::BTreeMap::from([
            (
                "type".to_owned(),
                JsonValue::String("model_request".to_owned()),
            ),
            ("model".to_owned(), JsonValue::String(model.clone())),
            ("prompt".to_owned(), JsonValue::String(prompt.clone())),
            (
                "max_output_tokens".to_owned(),
                JsonValue::Number(max_output_tokens.to_string()),
            ),
            ("seed".to_owned(), JsonValue::Number(seed.to_string())),
        ])),
        Value::ModelResponse { status, text } => {
            JsonValue::Object(std::collections::BTreeMap::from([
                (
                    "type".to_owned(),
                    JsonValue::String("model_response".to_owned()),
                ),
                ("status".to_owned(), JsonValue::String(status.clone())),
                ("text".to_owned(), JsonValue::String(text.clone())),
            ]))
        }
        Value::List(values) => {
            JsonValue::Array(values.iter().map(value_to_json).collect::<Vec<_>>())
        }
        Value::Tensor(tensor) => JsonValue::Object(std::collections::BTreeMap::from([
            ("type".to_owned(), JsonValue::String("tensor".to_owned())),
            (
                "dtype".to_owned(),
                JsonValue::String(tensor.dtype().name().to_owned()),
            ),
            (
                "shape".to_owned(),
                JsonValue::Array(
                    tensor
                        .shape()
                        .iter()
                        .map(|value| JsonValue::Number(value.to_string()))
                        .collect(),
                ),
            ),
            (
                "elements".to_owned(),
                JsonValue::Number(tensor.element_count().to_string()),
            ),
        ])),
        Value::Record { name, fields } => JsonValue::Object(std::collections::BTreeMap::from([
            ("type".to_owned(), JsonValue::String("record".to_owned())),
            ("name".to_owned(), JsonValue::String(name.clone())),
            (
                "fields".to_owned(),
                JsonValue::Object(
                    fields
                        .iter()
                        .map(|(field, value)| (field.clone(), value_to_json(value)))
                        .collect(),
                ),
            ),
        ])),
        Value::Enum {
            name,
            variant,
            fields,
        } => JsonValue::Object(std::collections::BTreeMap::from([
            ("type".to_owned(), JsonValue::String("enum".to_owned())),
            ("name".to_owned(), JsonValue::String(name.clone())),
            ("variant".to_owned(), JsonValue::String(variant.clone())),
            (
                "fields".to_owned(),
                JsonValue::Array(fields.iter().map(value_to_json).collect()),
            ),
        ])),
        Value::Option(value) => value
            .as_deref()
            .map(value_to_json)
            .unwrap_or(JsonValue::Null),
        Value::Result { ok, value } => JsonValue::Object(std::collections::BTreeMap::from([
            ("ok".to_owned(), JsonValue::Bool(*ok)),
            ("value".to_owned(), value_to_json(value)),
        ])),
        Value::Function(name) => JsonValue::String(name.clone()),
    }
}

fn is_named_error_constructor(name: &str) -> bool {
    matches!(
        name,
        "JsonError" | "JsonIoError" | "ModelError" | "CheckpointError"
    )
}

#[allow(clippy::too_many_arguments)]
fn train_linear_json(
    train_file: &Path,
    test_file: &Path,
    train_path: &str,
    test_path: &str,
    seed: i64,
    steps: i64,
    json_max_bytes: usize,
    json_max_depth: usize,
    event_sink: &mut dyn EventSink,
) -> Result<JsonValue, String> {
    let train = read_xy_dataset(train_file, train_path, json_max_bytes, json_max_depth)?;
    let test = read_xy_dataset(test_file, test_path, json_max_bytes, json_max_depth)?;
    if steps < 0 {
        return Err("training steps must be non-negative".to_owned());
    }
    if steps > MAX_TRAINING_STEPS {
        return Err(format!(
            "training steps exceed maximum {MAX_TRAINING_STEPS}"
        ));
    }
    let total_updates = train
        .0
        .len()
        .checked_mul(steps as usize)
        .ok_or_else(|| "training update count overflowed".to_owned())?;
    if total_updates > MAX_TRAINING_UPDATES {
        return Err(format!(
            "training updates exceed maximum {MAX_TRAINING_UPDATES}"
        ));
    }
    let mut weight = 0.0;
    let mut bias = 0.0;
    let initial_loss = mean_squared_error(&train, weight, bias);
    if !initial_loss.is_finite() {
        return Err("training produced a non-finite initial loss".to_owned());
    }
    event_sink.emit(RuntimeEvent::Started {
        model: "linear".to_owned(),
        train_path: train_path.to_owned(),
        test_path: test_path.to_owned(),
        train_sha256: train.2.clone(),
        test_sha256: test.2.clone(),
        train_samples: train.0.len(),
        test_samples: test.0.len(),
        steps,
        seed,
        device: LINEAR_DEVICE.to_owned(),
        dtype: LINEAR_DTYPE.to_owned(),
        optimizer: LINEAR_OPTIMIZER.to_owned(),
        learning_rate: LINEAR_LEARNING_RATE,
        parameter_count: LINEAR_PARAMETER_COUNT,
    })?;
    let progress_interval = (steps / 10).max(1);
    for step in 1..=steps {
        let count = train.0.len() as f64;
        let (weight_gradient, bias_gradient) = train.0.iter().zip(&train.1).fold(
            (0.0, 0.0),
            |(weight_gradient, bias_gradient), (x, y)| {
                let error = weight * x + bias - y;
                (
                    weight_gradient + (2.0 * error * x) / count,
                    bias_gradient + (2.0 * error) / count,
                )
            },
        );
        weight -= LINEAR_LEARNING_RATE * weight_gradient;
        bias -= LINEAR_LEARNING_RATE * bias_gradient;
        if step == 1 || step == steps || step % progress_interval == 0 {
            let loss = mean_squared_error(&train, weight, bias);
            if !loss.is_finite() {
                return Err(format!(
                    "training produced a non-finite loss at step {step}"
                ));
            }
            event_sink.emit(RuntimeEvent::Step {
                model: "linear".to_owned(),
                step,
                steps,
                updates: (step as usize) * train.0.len(),
                loss,
                loss_delta: initial_loss - loss,
            })?;
        }
    }
    let final_loss = mean_squared_error(&train, weight, bias);
    if !final_loss.is_finite() {
        return Err("training produced a non-finite final loss".to_owned());
    }
    let holdout_mae_observed = test
        .0
        .iter()
        .zip(&test.1)
        .map(|(x, y)| (weight * x + bias - y).abs())
        .fold(0.0, f64::max);
    if !holdout_mae_observed.is_finite() {
        return Err("training produced a non-finite holdout error".to_owned());
    }
    const HOLDOUT_MAE_MAX: f64 = 0.05;
    if holdout_mae_observed > HOLDOUT_MAE_MAX {
        return Err(format!(
            "holdout MAE {holdout_mae_observed:.8} exceeds maximum {HOLDOUT_MAE_MAX:.2}"
        ));
    }
    event_sink.emit(RuntimeEvent::Completed {
        model: "linear".to_owned(),
        steps,
        updates: total_updates,
        seed,
        device: LINEAR_DEVICE.to_owned(),
        dtype: LINEAR_DTYPE.to_owned(),
        optimizer: LINEAR_OPTIMIZER.to_owned(),
        learning_rate: LINEAR_LEARNING_RATE,
        parameter_count: LINEAR_PARAMETER_COUNT,
        initial_loss,
        final_loss,
        loss_delta: initial_loss - final_loss,
        loss_decreased: final_loss < initial_loss,
        holdout_mae_observed,
        weight,
        bias,
    })?;
    Ok(JsonValue::Object(std::collections::BTreeMap::from([
        (
            "loss_decreased".to_owned(),
            JsonValue::Bool(final_loss < initial_loss),
        ),
        (
            "holdout_mae_max".to_owned(),
            JsonValue::Number(format!("{HOLDOUT_MAE_MAX:.2}")),
        ),
        (
            "holdout_mae_observed".to_owned(),
            JsonValue::Number(format!("{holdout_mae_observed:.8}")),
        ),
        ("seed".to_owned(), JsonValue::Number(seed.to_string())),
    ])))
}

#[allow(clippy::too_many_arguments)]
fn train_linear_gpu_json(
    train_file: &Path,
    test_file: &Path,
    train_path: &str,
    test_path: &str,
    seed: i64,
    steps: i64,
    json_max_bytes: usize,
    json_max_depth: usize,
    event_sink: &mut dyn EventSink,
) -> Result<JsonValue, String> {
    const LEARNING_RATE: f32 = 0.02;
    let train = read_xy_dataset(train_file, train_path, json_max_bytes, json_max_depth)?;
    let test = read_xy_dataset(test_file, test_path, json_max_bytes, json_max_depth)?;
    let steps =
        usize::try_from(steps).map_err(|_| "GPU linear steps must be a positive I64".to_owned())?;
    let train_x = train
        .0
        .iter()
        .map(|value| *value as f32)
        .collect::<Vec<_>>();
    let train_y = train
        .1
        .iter()
        .map(|value| *value as f32)
        .collect::<Vec<_>>();
    event_sink.emit(RuntimeEvent::GpuStarted {
        model: "linear_gpu".to_owned(),
        train_path: train_path.to_owned(),
        test_path: test_path.to_owned(),
        train_sha256: train.2.clone(),
        test_sha256: test.2.clone(),
        train_samples: train_x.len(),
        steps,
        seed,
        device: "gpu".to_owned(),
        dtype: "f32".to_owned(),
        optimizer: "sgd".to_owned(),
        learning_rate: LEARNING_RATE,
        parameter_count: 2,
        adapter: "selected_physical_adapter".to_owned(),
        backend: "wgpu".to_owned(),
    })?;
    let report = gpu::train_linear(&train_x, &train_y, steps, LEARNING_RATE)
        .map_err(|error| error.message)?;
    let holdout_mae_observed = test
        .0
        .iter()
        .zip(&test.1)
        .map(|(x, y)| (f64::from(report.weight) * x + f64::from(report.bias) - y).abs())
        .fold(0.0, f64::max) as f32;
    if !holdout_mae_observed.is_finite() {
        return Err("GPU training produced a non-finite holdout error".to_owned());
    }
    let loss_decreased = report.final_loss < report.initial_loss;
    event_sink.emit(RuntimeEvent::GpuCompleted {
        model: "linear_gpu".to_owned(),
        steps: report.steps,
        seed,
        device: "gpu".to_owned(),
        dtype: "f32".to_owned(),
        optimizer: "sgd".to_owned(),
        learning_rate: LEARNING_RATE,
        parameter_count: 2,
        initial_loss: report.initial_loss,
        final_loss: report.final_loss,
        loss_decreased,
        holdout_mae_observed,
        weight: report.weight,
        bias: report.bias,
        adapter: report.adapter.name.clone(),
        backend: report.adapter.backend.clone(),
        dispatches: report.dispatches,
        readbacks: report.readbacks,
    })?;
    Ok(JsonValue::Object(std::collections::BTreeMap::from([
        (
            "operation".to_owned(),
            JsonValue::String("train_linear_gpu".to_owned()),
        ),
        ("device".to_owned(), JsonValue::String("gpu".to_owned())),
        ("fallback".to_owned(), JsonValue::Bool(false)),
        ("dtype".to_owned(), JsonValue::String("f32".to_owned())),
        ("optimizer".to_owned(), JsonValue::String("sgd".to_owned())),
        (
            "steps".to_owned(),
            JsonValue::Number(report.steps.to_string()),
        ),
        (
            "dispatches".to_owned(),
            JsonValue::Number(report.dispatches.to_string()),
        ),
        (
            "readbacks".to_owned(),
            JsonValue::Number(report.readbacks.to_string()),
        ),
        (
            "initial_loss".to_owned(),
            JsonValue::Number(format_f32(report.initial_loss)),
        ),
        (
            "final_loss".to_owned(),
            JsonValue::Number(format_f32(report.final_loss)),
        ),
        ("loss_decreased".to_owned(), JsonValue::Bool(loss_decreased)),
        (
            "holdout_mae_observed".to_owned(),
            JsonValue::Number(format_f32(holdout_mae_observed)),
        ),
        (
            "weight".to_owned(),
            JsonValue::Number(format_f32(report.weight)),
        ),
        (
            "bias".to_owned(),
            JsonValue::Number(format_f32(report.bias)),
        ),
        ("adapter".to_owned(), JsonValue::String(report.adapter.name)),
        (
            "backend".to_owned(),
            JsonValue::String(report.adapter.backend),
        ),
        ("seed".to_owned(), JsonValue::Number(seed.to_string())),
    ])))
}

struct MlpDataset {
    inputs: Vec<Vec<f32>>,
    targets: Vec<Vec<f32>>,
    input_size: usize,
    output_size: usize,
    sha256: String,
}

struct MlpModel {
    input_size: usize,
    hidden_size: usize,
    output_size: usize,
    first_weights: Vec<f32>,
    first_bias: Vec<f32>,
    second_weights: Vec<f32>,
    second_bias: Vec<f32>,
}

struct MlpState {
    model: MlpModel,
    step: i64,
    cursor: usize,
    epoch: usize,
    order: Vec<usize>,
    rng_state: u64,
}

struct MlpTrainingRequest<'a> {
    train_file: &'a Path,
    test_file: &'a Path,
    checkpoint_file: &'a Path,
    train_path: &'a str,
    test_path: &'a str,
    checkpoint_path: &'a str,
    seed: i64,
    steps: i64,
    batch_size: i64,
    hidden_size: i64,
    learning_rate: f32,
}

fn train_mlp_json(
    request: MlpTrainingRequest<'_>,
    json_max_bytes: usize,
    json_max_depth: usize,
    event_sink: &mut dyn EventSink,
) -> Result<JsonValue, String> {
    let MlpTrainingRequest {
        train_file,
        test_file,
        checkpoint_file,
        train_path,
        test_path,
        checkpoint_path,
        seed,
        steps,
        batch_size,
        hidden_size,
        learning_rate,
    } = request;
    let train = read_mlp_dataset(train_file, train_path, json_max_bytes, json_max_depth)?;
    let test = read_mlp_dataset(test_file, test_path, json_max_bytes, json_max_depth)?;
    if train.input_size != test.input_size || train.output_size != test.output_size {
        return Err(
            "train and test datasets must have identical input/output dimensions".to_owned(),
        );
    }
    if !(0..=MAX_TRAINING_STEPS).contains(&steps) {
        return Err(format!(
            "training steps must be between 0 and {MAX_TRAINING_STEPS}"
        ));
    }
    let batch_size = usize::try_from(batch_size)
        .map_err(|_| "training batch_size must be positive".to_owned())?;
    if batch_size == 0 || batch_size > train.inputs.len() {
        return Err(format!(
            "training batch_size must be between 1 and {}",
            train.inputs.len()
        ));
    }
    let sample_updates = usize::try_from(steps)
        .ok()
        .and_then(|value| value.checked_mul(batch_size))
        .ok_or_else(|| "MLP sample update count overflowed".to_owned())?;
    if sample_updates > MAX_TRAINING_UPDATES {
        return Err(format!(
            "MLP sample updates exceed maximum {MAX_TRAINING_UPDATES}"
        ));
    }
    let hidden_size = usize::try_from(hidden_size)
        .map_err(|_| "training hidden_size must be positive".to_owned())?;
    if hidden_size == 0 || hidden_size > MAX_TRAINING_HIDDEN {
        return Err(format!(
            "training hidden_size must be between 1 and {MAX_TRAINING_HIDDEN}"
        ));
    }
    let parameter_count = train
        .input_size
        .checked_mul(hidden_size)
        .and_then(|value| value.checked_add(hidden_size))
        .and_then(|value| value.checked_add(hidden_size.checked_mul(train.output_size)?))
        .and_then(|value| value.checked_add(train.output_size))
        .ok_or_else(|| "MLP parameter count overflowed".to_owned())?;
    if parameter_count > MAX_TRAINING_PARAMETERS {
        return Err(format!(
            "MLP parameter count exceeds maximum {MAX_TRAINING_PARAMETERS}"
        ));
    }
    let work_units = sample_updates
        .checked_mul(parameter_count)
        .ok_or_else(|| "MLP work unit count overflowed".to_owned())?;
    if work_units > MAX_MLP_WORK_UNITS {
        return Err(format!(
            "MLP work units exceed maximum {MAX_MLP_WORK_UNITS}"
        ));
    }
    if !learning_rate.is_finite() || learning_rate <= 0.0 {
        return Err("training learning_rate must be positive and finite".to_owned());
    }

    let mut resumed_from = 0_i64;
    let mut checkpoint_before_sha256 = None;
    let mut state = if checkpoint_file.is_file() {
        let checkpoint_max_bytes = json_max_bytes.min(MAX_CHECKPOINT_BYTES);
        let checkpoint_max_depth = json_max_depth.min(32);
        let source = read_bounded_utf8(checkpoint_file, checkpoint_max_bytes, "MLP checkpoint")?;
        checkpoint_before_sha256 = Some(sha256_hex(source.as_bytes()));
        let value = json::parse(&source, checkpoint_max_bytes, checkpoint_max_depth)
            .map_err(|error| error.message)?;
        let loaded = parse_mlp_checkpoint(
            &value,
            &train,
            &test,
            seed,
            batch_size,
            hidden_size,
            learning_rate,
        )?;
        resumed_from = loaded.step;
        loaded
    } else {
        let mut rng_state = initial_mlp_rng_state(seed);
        let model = initialize_mlp(
            train.input_size,
            hidden_size,
            train.output_size,
            &mut rng_state,
        );
        let mut state = MlpState {
            model,
            step: 0,
            cursor: 0,
            epoch: 0,
            order: (0..train.inputs.len()).collect(),
            rng_state,
        };
        shuffle_order(&mut state.order, &mut state.rng_state);
        state
    };
    if state.step > steps {
        return Err(format!(
            "checkpoint step {} is ahead of requested target {steps}",
            state.step
        ));
    }
    let initial_loss = mlp_loss(&state.model, &train)?;
    event_sink.emit(RuntimeEvent::MlpStarted {
        model: "mlp".to_owned(),
        train_path: train_path.to_owned(),
        test_path: test_path.to_owned(),
        checkpoint_path: checkpoint_path.to_owned(),
        train_sha256: train.sha256.clone(),
        test_sha256: test.sha256.clone(),
        checkpoint_before_sha256,
        train_samples: train.inputs.len(),
        test_samples: test.inputs.len(),
        steps,
        seed,
        device: "cpu".to_owned(),
        dtype: "f32".to_owned(),
        optimizer: "sgd".to_owned(),
        learning_rate,
        batch_size,
        hidden_size,
        parameter_count,
        resumed_from,
    })?;

    let progress_interval = (steps / 10).max(1);
    while state.step < steps {
        if state.cursor >= state.order.len() {
            state.cursor = 0;
            state.epoch = state.epoch.saturating_add(1);
            shuffle_order(&mut state.order, &mut state.rng_state);
        }
        let end = (state.cursor + batch_size).min(state.order.len());
        let batch = state.order[state.cursor..end].to_vec();
        state.cursor = end;
        mlp_update(&mut state.model, &train, &batch, learning_rate)?;
        state.step += 1;
        if state.step == 1 || state.step == steps || state.step % progress_interval == 0 {
            let loss = mlp_loss(&state.model, &train)?;
            let evaluation_loss = mlp_loss(&state.model, &test)?;
            event_sink.emit(RuntimeEvent::MlpStep {
                model: "mlp".to_owned(),
                step: state.step,
                steps,
                updates: state.step as usize,
                epoch: state.epoch,
                loss,
                evaluation_loss,
            })?;
            let checkpoint =
                encode_mlp_checkpoint(&state, &train, &test, seed, batch_size, learning_rate);
            write_checkpoint_atomic(checkpoint_file, &checkpoint)?;
        }
    }
    let final_loss = mlp_loss(&state.model, &train)?;
    let evaluation_loss = mlp_loss(&state.model, &test)?;
    let checkpoint = encode_mlp_checkpoint(&state, &train, &test, seed, batch_size, learning_rate);
    let checkpoint_sha256 = write_checkpoint_atomic(checkpoint_file, &checkpoint)?;
    let loss_decreased = final_loss < initial_loss;
    event_sink.emit(RuntimeEvent::MlpCompleted {
        model: "mlp".to_owned(),
        steps,
        updates: state.step as usize,
        seed,
        device: "cpu".to_owned(),
        dtype: "f32".to_owned(),
        optimizer: "sgd".to_owned(),
        learning_rate,
        batch_size,
        hidden_size,
        parameter_count,
        initial_loss,
        final_loss,
        evaluation_loss,
        loss_decreased,
        resumed_from,
        checkpoint_sha256: checkpoint_sha256.clone(),
    })?;
    Ok(JsonValue::Object(std::collections::BTreeMap::from([
        (
            "operation".to_owned(),
            JsonValue::String("train_mlp".to_owned()),
        ),
        ("model".to_owned(), JsonValue::String("mlp".to_owned())),
        ("device".to_owned(), JsonValue::String("cpu".to_owned())),
        ("dtype".to_owned(), JsonValue::String("f32".to_owned())),
        ("optimizer".to_owned(), JsonValue::String("sgd".to_owned())),
        (
            "steps".to_owned(),
            JsonValue::Number(state.step.to_string()),
        ),
        (
            "batch_size".to_owned(),
            JsonValue::Number(batch_size.to_string()),
        ),
        (
            "hidden_size".to_owned(),
            JsonValue::Number(hidden_size.to_string()),
        ),
        (
            "parameter_count".to_owned(),
            JsonValue::Number(parameter_count.to_string()),
        ),
        (
            "initial_loss".to_owned(),
            JsonValue::Number(format_f32(initial_loss)),
        ),
        (
            "final_loss".to_owned(),
            JsonValue::Number(format_f32(final_loss)),
        ),
        (
            "evaluation_loss".to_owned(),
            JsonValue::Number(format_f32(evaluation_loss)),
        ),
        ("loss_decreased".to_owned(), JsonValue::Bool(loss_decreased)),
        (
            "resumed_from".to_owned(),
            JsonValue::Number(resumed_from.to_string()),
        ),
        (
            "checkpoint_sha256".to_owned(),
            JsonValue::String(checkpoint_sha256),
        ),
    ])))
}

fn read_mlp_dataset(
    path: &Path,
    label: &str,
    json_max_bytes: usize,
    json_max_depth: usize,
) -> Result<MlpDataset, String> {
    let dataset_max_bytes = json_max_bytes.min(MAX_TRAINING_DATASET_BYTES);
    let dataset_max_depth = json_max_depth.min(32);
    let source = read_bounded_utf8(
        path,
        dataset_max_bytes,
        &format!("training dataset `{label}`"),
    )?;
    let sha256 = sha256_hex(source.as_bytes());
    let value = json::parse(&source, dataset_max_bytes, dataset_max_depth)
        .map_err(|error| error.message)?;
    let JsonValue::Object(values) = value else {
        return Err("MLP dataset must be a JSON object".to_owned());
    };
    let Some(JsonValue::Array(input_values)) = values.get("inputs") else {
        return Err("MLP dataset is missing inputs".to_owned());
    };
    let Some(JsonValue::Array(target_values)) = values.get("targets") else {
        return Err("MLP dataset is missing targets".to_owned());
    };
    if input_values.is_empty() || input_values.len() != target_values.len() {
        return Err("MLP dataset inputs/targets lengths are invalid".to_owned());
    }
    if input_values.len() > MAX_TRAINING_SAMPLES {
        return Err(format!(
            "MLP dataset exceeds maximum {MAX_TRAINING_SAMPLES} samples"
        ));
    }
    let inputs = input_values
        .iter()
        .map(|value| json_vector(value, "inputs"))
        .collect::<Result<Vec<_>, _>>()?;
    let targets = target_values
        .iter()
        .map(|value| json_vector(value, "targets"))
        .collect::<Result<Vec<_>, _>>()?;
    let input_size = inputs[0].len();
    let output_size = targets[0].len();
    if input_size == 0 || input_size > MAX_TRAINING_INPUTS {
        return Err(format!(
            "MLP input size must be between 1 and {MAX_TRAINING_INPUTS}"
        ));
    }
    if output_size == 0 || output_size > MAX_TRAINING_OUTPUTS {
        return Err(format!(
            "MLP output size must be between 1 and {MAX_TRAINING_OUTPUTS}"
        ));
    }
    if inputs.iter().any(|row| row.len() != input_size)
        || targets.iter().any(|row| row.len() != output_size)
    {
        return Err("MLP dataset rows must have uniform dimensions".to_owned());
    }
    Ok(MlpDataset {
        inputs,
        targets,
        input_size,
        output_size,
        sha256,
    })
}

struct MlpInputBatch {
    inputs: Vec<Vec<f32>>,
    input_size: usize,
    sha256: String,
}

fn predict_mlp_json(
    checkpoint_file: &Path,
    input_file: &Path,
    checkpoint_path: &str,
    input_path: &str,
    json_max_bytes: usize,
    json_max_depth: usize,
    event_sink: &mut dyn EventSink,
) -> Result<JsonValue, String> {
    let checkpoint_max_bytes = json_max_bytes.min(MAX_CHECKPOINT_BYTES);
    let checkpoint_source =
        read_bounded_utf8(checkpoint_file, checkpoint_max_bytes, "MLP checkpoint")?;
    let checkpoint_sha256 = sha256_hex(checkpoint_source.as_bytes());
    let checkpoint = json::parse(
        &checkpoint_source,
        checkpoint_max_bytes,
        json_max_depth.min(32),
    )
    .map_err(|error| error.message)?;
    let (model, checkpoint_step) = parse_mlp_model_checkpoint(&checkpoint)?;
    let inputs = read_mlp_inputs(input_file, input_path, json_max_bytes, json_max_depth)?;
    if inputs.input_size != model.input_size {
        return Err(format!(
            "MLP inference input dimension {} does not match checkpoint dimension {}",
            inputs.input_size, model.input_size
        ));
    }

    event_sink.emit(RuntimeEvent::InferenceStarted {
        model: "mlp".to_owned(),
        checkpoint_path: checkpoint_path.to_owned(),
        input_path: input_path.to_owned(),
        checkpoint_sha256: checkpoint_sha256.clone(),
        input_sha256: inputs.sha256.clone(),
        samples: inputs.inputs.len(),
        input_size: model.input_size,
        output_size: model.output_size,
        checkpoint_step,
    })?;

    let mut pre_activation = vec![0.0; model.hidden_size];
    let mut hidden = vec![0.0; model.hidden_size];
    let mut output = vec![0.0; model.output_size];
    let mut predictions = Vec::with_capacity(inputs.inputs.len());
    for input in &inputs.inputs {
        forward_mlp(&model, input, &mut pre_activation, &mut hidden, &mut output);
        predictions.push(json_f32_values(&output));
    }
    event_sink.emit(RuntimeEvent::InferenceCompleted {
        model: "mlp".to_owned(),
        samples: inputs.inputs.len(),
        input_size: model.input_size,
        output_size: model.output_size,
        checkpoint_sha256: checkpoint_sha256.clone(),
        input_sha256: inputs.sha256.clone(),
        checkpoint_step,
    })?;

    Ok(JsonValue::Object(std::collections::BTreeMap::from([
        (
            "operation".to_owned(),
            JsonValue::String("predict_mlp".to_owned()),
        ),
        ("model".to_owned(), JsonValue::String("mlp".to_owned())),
        ("device".to_owned(), JsonValue::String("cpu".to_owned())),
        ("dtype".to_owned(), JsonValue::String("f32".to_owned())),
        (
            "checkpoint_sha256".to_owned(),
            JsonValue::String(checkpoint_sha256),
        ),
        ("input_sha256".to_owned(), JsonValue::String(inputs.sha256)),
        (
            "checkpoint_step".to_owned(),
            JsonValue::Number(checkpoint_step.to_string()),
        ),
        (
            "samples".to_owned(),
            JsonValue::Number(inputs.inputs.len().to_string()),
        ),
        (
            "input_size".to_owned(),
            JsonValue::Number(model.input_size.to_string()),
        ),
        (
            "output_size".to_owned(),
            JsonValue::Number(model.output_size.to_string()),
        ),
        ("predictions".to_owned(), JsonValue::Array(predictions)),
    ])))
}

fn read_mlp_inputs(
    path: &Path,
    label: &str,
    json_max_bytes: usize,
    json_max_depth: usize,
) -> Result<MlpInputBatch, String> {
    let input_max_bytes = json_max_bytes.min(MAX_TRAINING_DATASET_BYTES);
    let source = read_bounded_utf8(path, input_max_bytes, &format!("MLP input `{label}`"))?;
    let sha256 = sha256_hex(source.as_bytes());
    let value = json::parse(&source, input_max_bytes, json_max_depth.min(32))
        .map_err(|error| error.message)?;
    let JsonValue::Object(values) = value else {
        return Err("MLP inference input must be a JSON object".to_owned());
    };
    let Some(JsonValue::Array(input_values)) = values.get("inputs") else {
        return Err("MLP inference input is missing inputs".to_owned());
    };
    if input_values.is_empty() {
        return Err("MLP inference input must contain at least one row".to_owned());
    }
    if input_values.len() > MAX_TRAINING_SAMPLES {
        return Err(format!(
            "MLP inference input exceeds maximum {MAX_TRAINING_SAMPLES} samples"
        ));
    }
    let inputs = input_values
        .iter()
        .map(|value| json_vector(value, "inputs"))
        .collect::<Result<Vec<_>, _>>()?;
    let input_size = inputs[0].len();
    if input_size == 0 || input_size > MAX_TRAINING_INPUTS {
        return Err(format!(
            "MLP inference input size must be between 1 and {MAX_TRAINING_INPUTS}"
        ));
    }
    if inputs.iter().any(|row| row.len() != input_size) {
        return Err("MLP inference input rows must have uniform dimensions".to_owned());
    }
    Ok(MlpInputBatch {
        inputs,
        input_size,
        sha256,
    })
}

fn parse_mlp_model_checkpoint(value: &JsonValue) -> Result<(MlpModel, i64), String> {
    let JsonValue::Object(values) = value else {
        return Err("MLP checkpoint root must be an object".to_owned());
    };
    if !matches!(values.get("schema"), Some(JsonValue::String(value)) if value == "seloryn.training.checkpoint.v2")
    {
        return Err("MLP checkpoint schema is unsupported".to_owned());
    }
    if !matches!(values.get("model"), Some(JsonValue::String(value)) if value == "mlp") {
        return Err("MLP checkpoint model is unsupported".to_owned());
    }
    if !matches!(
        values.get("optimizer"),
        Some(JsonValue::Object(optimizer))
            if matches!(optimizer.get("name"), Some(JsonValue::String(value)) if value == "sgd")
                && matches!(optimizer.get("state"), Some(JsonValue::String(value)) if value == "stateless")
    ) {
        return Err("MLP checkpoint optimizer is unsupported".to_owned());
    }
    let step = json_i64_field(values, "step")?;
    if step < 0 {
        return Err("MLP checkpoint step cannot be negative".to_owned());
    }
    let input_size = json_usize_field(values, "input_size")?;
    let hidden_size = json_usize_field(values, "hidden_size")?;
    let output_size = json_usize_field(values, "output_size")?;
    if input_size == 0 || input_size > MAX_TRAINING_INPUTS {
        return Err(format!(
            "MLP checkpoint input size must be between 1 and {MAX_TRAINING_INPUTS}"
        ));
    }
    if hidden_size == 0 || hidden_size > MAX_TRAINING_HIDDEN {
        return Err(format!(
            "MLP checkpoint hidden size must be between 1 and {MAX_TRAINING_HIDDEN}"
        ));
    }
    if output_size == 0 || output_size > MAX_TRAINING_OUTPUTS {
        return Err(format!(
            "MLP checkpoint output size must be between 1 and {MAX_TRAINING_OUTPUTS}"
        ));
    }
    let parameter_count = input_size
        .checked_mul(hidden_size)
        .and_then(|value| value.checked_add(hidden_size))
        .and_then(|value| value.checked_add(hidden_size.checked_mul(output_size)?))
        .and_then(|value| value.checked_add(output_size))
        .ok_or_else(|| "MLP checkpoint parameter count overflowed".to_owned())?;
    if parameter_count > MAX_TRAINING_PARAMETERS {
        return Err(format!(
            "MLP checkpoint parameters exceed maximum {MAX_TRAINING_PARAMETERS}"
        ));
    }
    let model_values = match values.get("weights") {
        Some(JsonValue::Object(value)) => value,
        _ => return Err("MLP checkpoint weights are missing".to_owned()),
    };
    let actual_weights_hash =
        sha256_hex(json::encode(&JsonValue::Object(model_values.clone())).as_bytes());
    if !matches!(values.get("weights_sha256"), Some(JsonValue::String(value)) if value == &actual_weights_hash)
    {
        return Err("MLP checkpoint weights hash does not match the payload".to_owned());
    }
    let model = MlpModel {
        input_size,
        hidden_size,
        output_size,
        first_weights: json_f32_array(model_values, "first_weights")?,
        first_bias: json_f32_array(model_values, "first_bias")?,
        second_weights: json_f32_array(model_values, "second_weights")?,
        second_bias: json_f32_array(model_values, "second_bias")?,
    };
    if model.first_weights.len() != input_size * hidden_size
        || model.first_bias.len() != hidden_size
        || model.second_weights.len() != hidden_size * output_size
        || model.second_bias.len() != output_size
    {
        return Err("MLP checkpoint parameter shapes are invalid".to_owned());
    }
    let data = match values.get("data") {
        Some(JsonValue::Object(value)) => value,
        _ => return Err("MLP checkpoint data state is missing".to_owned()),
    };
    let actual_data_hash = sha256_hex(json::encode(&JsonValue::Object(data.clone())).as_bytes());
    if !matches!(values.get("data_sha256"), Some(JsonValue::String(value)) if value == &actual_data_hash)
    {
        return Err("MLP checkpoint data hash does not match the payload".to_owned());
    }
    let state_value = JsonValue::Object(std::collections::BTreeMap::from([
        ("step".to_owned(), JsonValue::Number(step.to_string())),
        (
            "weights".to_owned(),
            JsonValue::Object(model_values.clone()),
        ),
        ("data".to_owned(), JsonValue::Object(data.clone())),
    ]));
    let actual_state_hash = sha256_hex(json::encode(&state_value).as_bytes());
    if !matches!(values.get("state_sha256"), Some(JsonValue::String(value)) if value == &actual_state_hash)
    {
        return Err("MLP checkpoint training state hash does not match the payload".to_owned());
    }
    Ok((model, step))
}

fn json_vector(value: &JsonValue, label: &str) -> Result<Vec<f32>, String> {
    let JsonValue::Array(values) = value else {
        return Err(format!("MLP dataset {label} rows must be arrays"));
    };
    values
        .iter()
        .map(|value| {
            let JsonValue::Number(number) = value else {
                return Err(format!("MLP dataset {label} contains a non-number"));
            };
            let value = number
                .parse::<f32>()
                .map_err(|_| format!("MLP dataset {label} contains an invalid number"))?;
            value
                .is_finite()
                .then_some(value)
                .ok_or_else(|| format!("MLP dataset {label} contains a non-finite number"))
        })
        .collect()
}

fn initialize_mlp(
    input_size: usize,
    hidden_size: usize,
    output_size: usize,
    rng_state: &mut u64,
) -> MlpModel {
    let first_scale = (2.0 / input_size as f32).sqrt();
    let second_scale = (2.0 / hidden_size as f32).sqrt();
    let first_weights = (0..input_size * hidden_size)
        .map(|_| (next_unit(rng_state) * 2.0 - 1.0) * first_scale)
        .collect();
    let second_weights = (0..hidden_size * output_size)
        .map(|_| (next_unit(rng_state) * 2.0 - 1.0) * second_scale)
        .collect();
    MlpModel {
        input_size,
        hidden_size,
        output_size,
        first_weights,
        first_bias: vec![0.0; hidden_size],
        second_weights,
        second_bias: vec![0.0; output_size],
    }
}

fn mlp_update(
    model: &mut MlpModel,
    dataset: &MlpDataset,
    batch: &[usize],
    learning_rate: f32,
) -> Result<(), String> {
    let mut first_gradients = vec![0.0; model.first_weights.len()];
    let mut first_bias_gradients = vec![0.0; model.first_bias.len()];
    let mut second_gradients = vec![0.0; model.second_weights.len()];
    let mut second_bias_gradients = vec![0.0; model.second_bias.len()];
    let mut hidden = vec![0.0; model.hidden_size];
    let mut pre_activation = vec![0.0; model.hidden_size];
    let mut output = vec![0.0; model.output_size];
    let scale = 1.0 / (batch.len() * model.output_size) as f32;
    for &sample in batch {
        forward_mlp(
            model,
            &dataset.inputs[sample],
            &mut pre_activation,
            &mut hidden,
            &mut output,
        );
        let mut output_gradient = vec![0.0; model.output_size];
        for index in 0..model.output_size {
            output_gradient[index] = (output[index] - dataset.targets[sample][index]) * 2.0 * scale;
            second_bias_gradients[index] += output_gradient[index];
            for hidden_index in 0..model.hidden_size {
                second_gradients[hidden_index * model.output_size + index] +=
                    hidden[hidden_index] * output_gradient[index];
            }
        }
        for hidden_index in 0..model.hidden_size {
            let hidden_gradient = if pre_activation[hidden_index] > 0.0 {
                (0..model.output_size)
                    .map(|index| {
                        output_gradient[index]
                            * model.second_weights[hidden_index * model.output_size + index]
                    })
                    .sum::<f32>()
            } else {
                0.0
            };
            first_bias_gradients[hidden_index] += hidden_gradient;
            for input_index in 0..model.input_size {
                first_gradients[input_index * model.hidden_size + hidden_index] +=
                    dataset.inputs[sample][input_index] * hidden_gradient;
            }
        }
    }
    for (value, gradient) in model.first_weights.iter_mut().zip(first_gradients) {
        *value -= learning_rate * gradient;
    }
    for (value, gradient) in model.first_bias.iter_mut().zip(first_bias_gradients) {
        *value -= learning_rate * gradient;
    }
    for (value, gradient) in model.second_weights.iter_mut().zip(second_gradients) {
        *value -= learning_rate * gradient;
    }
    for (value, gradient) in model.second_bias.iter_mut().zip(second_bias_gradients) {
        *value -= learning_rate * gradient;
    }
    if model
        .first_weights
        .iter()
        .chain(&model.first_bias)
        .chain(&model.second_weights)
        .chain(&model.second_bias)
        .any(|value| !value.is_finite())
    {
        return Err("MLP update produced a non-finite parameter".to_owned());
    }
    Ok(())
}

fn forward_mlp(
    model: &MlpModel,
    input: &[f32],
    pre_activation: &mut [f32],
    hidden: &mut [f32],
    output: &mut [f32],
) {
    for hidden_index in 0..model.hidden_size {
        let value = model.first_bias[hidden_index]
            + input
                .iter()
                .enumerate()
                .map(|(input_index, value)| {
                    value * model.first_weights[input_index * model.hidden_size + hidden_index]
                })
                .sum::<f32>();
        pre_activation[hidden_index] = value;
        hidden[hidden_index] = value.max(0.0);
    }
    for (output_index, output_value) in output.iter_mut().enumerate().take(model.output_size) {
        *output_value = model.second_bias[output_index]
            + (0..model.hidden_size)
                .map(|hidden_index| {
                    hidden[hidden_index]
                        * model.second_weights[hidden_index * model.output_size + output_index]
                })
                .sum::<f32>();
    }
}

fn mlp_loss(model: &MlpModel, dataset: &MlpDataset) -> Result<f32, String> {
    let mut pre_activation = vec![0.0; model.hidden_size];
    let mut hidden = vec![0.0; model.hidden_size];
    let mut output = vec![0.0; model.output_size];
    let mut loss = 0.0;
    for (input, target) in dataset.inputs.iter().zip(&dataset.targets) {
        forward_mlp(model, input, &mut pre_activation, &mut hidden, &mut output);
        loss += output
            .iter()
            .zip(target)
            .map(|(predicted, target)| (predicted - target).powi(2))
            .sum::<f32>();
    }
    let loss = loss / (dataset.inputs.len() * model.output_size) as f32;
    loss.is_finite()
        .then_some(loss)
        .ok_or_else(|| "MLP loss is non-finite".to_owned())
}

fn parse_mlp_checkpoint(
    value: &JsonValue,
    train: &MlpDataset,
    test: &MlpDataset,
    seed: i64,
    batch_size: usize,
    hidden_size: usize,
    learning_rate: f32,
) -> Result<MlpState, String> {
    let JsonValue::Object(values) = value else {
        return Err("MLP checkpoint root must be an object".to_owned());
    };
    if !matches!(values.get("schema"), Some(JsonValue::String(value)) if value == "seloryn.training.checkpoint.v2")
    {
        return Err("MLP checkpoint schema is unsupported".to_owned());
    }
    if !matches!(values.get("model"), Some(JsonValue::String(value)) if value == "mlp") {
        return Err("MLP checkpoint model is unsupported".to_owned());
    }
    if !matches!(
        values.get("optimizer"),
        Some(JsonValue::Object(optimizer))
            if matches!(optimizer.get("name"), Some(JsonValue::String(value)) if value == "sgd")
                && matches!(optimizer.get("state"), Some(JsonValue::String(value)) if value == "stateless")
    ) {
        return Err("MLP checkpoint optimizer is unsupported".to_owned());
    }
    if !matches!(values.get("train_sha256"), Some(JsonValue::String(value)) if value == &train.sha256)
        || !matches!(values.get("test_sha256"), Some(JsonValue::String(value)) if value == &test.sha256)
    {
        return Err("MLP checkpoint dataset hash does not match the current data".to_owned());
    }
    let checkpoint_seed = json_i64_field(values, "seed")?;
    if checkpoint_seed != seed {
        return Err("MLP checkpoint seed does not match the requested seed".to_owned());
    }
    let step = json_i64_field(values, "step")?;
    let checkpoint_batch = json_usize_field(values, "batch_size")?;
    let checkpoint_hidden = json_usize_field(values, "hidden_size")?;
    let checkpoint_learning_rate = json_f32_field(values, "learning_rate")?;
    if checkpoint_batch != batch_size
        || checkpoint_hidden != hidden_size
        || (checkpoint_learning_rate - learning_rate).abs() > f32::EPSILON
    {
        return Err("MLP checkpoint hyperparameters do not match the requested run".to_owned());
    }
    if step < 0 {
        return Err("MLP checkpoint step cannot be negative".to_owned());
    }
    let input_size = json_usize_field(values, "input_size")?;
    if input_size != train.input_size
        || json_usize_field(values, "output_size")? != train.output_size
    {
        return Err("MLP checkpoint dimensions do not match the current data".to_owned());
    }
    let model_values = match values.get("weights") {
        Some(JsonValue::Object(value)) => value,
        _ => return Err("MLP checkpoint weights are missing".to_owned()),
    };
    let actual_weights_hash =
        sha256_hex(json::encode(&JsonValue::Object(model_values.clone())).as_bytes());
    if !matches!(values.get("weights_sha256"), Some(JsonValue::String(value)) if value == &actual_weights_hash)
    {
        return Err("MLP checkpoint weights hash does not match the payload".to_owned());
    }
    let model = MlpModel {
        input_size,
        hidden_size,
        output_size: train.output_size,
        first_weights: json_f32_array(model_values, "first_weights")?,
        first_bias: json_f32_array(model_values, "first_bias")?,
        second_weights: json_f32_array(model_values, "second_weights")?,
        second_bias: json_f32_array(model_values, "second_bias")?,
    };
    if model.first_weights.len() != input_size * hidden_size
        || model.first_bias.len() != hidden_size
        || model.second_weights.len() != hidden_size * train.output_size
        || model.second_bias.len() != train.output_size
    {
        return Err("MLP checkpoint parameter shapes are invalid".to_owned());
    }
    let data = match values.get("data") {
        Some(JsonValue::Object(value)) => value,
        _ => return Err("MLP checkpoint data state is missing".to_owned()),
    };
    let state_value = JsonValue::Object(std::collections::BTreeMap::from([
        ("step".to_owned(), JsonValue::Number(step.to_string())),
        (
            "weights".to_owned(),
            JsonValue::Object(model_values.clone()),
        ),
        ("data".to_owned(), JsonValue::Object(data.clone())),
    ]));
    let actual_state_hash = sha256_hex(json::encode(&state_value).as_bytes());
    if !matches!(values.get("state_sha256"), Some(JsonValue::String(value)) if value == &actual_state_hash)
    {
        return Err("MLP checkpoint training state hash does not match the payload".to_owned());
    }
    let actual_data_hash = sha256_hex(json::encode(&JsonValue::Object(data.clone())).as_bytes());
    if !matches!(values.get("data_sha256"), Some(JsonValue::String(value)) if value == &actual_data_hash)
    {
        return Err("MLP checkpoint data hash does not match the payload".to_owned());
    }
    let order = json_usize_array(data, "order")?;
    if order.len() != train.inputs.len()
        || order.iter().any(|index| *index >= train.inputs.len())
        || {
            let mut sorted = order.clone();
            sorted.sort_unstable();
            sorted != (0..train.inputs.len()).collect::<Vec<_>>()
        }
    {
        return Err("MLP checkpoint sampler order is invalid".to_owned());
    }
    let cursor = json_usize_field(data, "cursor")?;
    if cursor > order.len() {
        return Err("MLP checkpoint sampler cursor is invalid".to_owned());
    }
    Ok(MlpState {
        model,
        step,
        cursor,
        epoch: json_usize_field(data, "epoch")?,
        order,
        rng_state: json_u64_field(data, "rng_state")?,
    })
}

fn encode_mlp_checkpoint(
    state: &MlpState,
    train: &MlpDataset,
    test: &MlpDataset,
    seed: i64,
    batch_size: usize,
    learning_rate: f32,
) -> String {
    let model = &state.model;
    let weights = std::collections::BTreeMap::from([
        (
            "first_weights".to_owned(),
            json_f32_values(&model.first_weights),
        ),
        ("first_bias".to_owned(), json_f32_values(&model.first_bias)),
        (
            "second_weights".to_owned(),
            json_f32_values(&model.second_weights),
        ),
        (
            "second_bias".to_owned(),
            json_f32_values(&model.second_bias),
        ),
    ]);
    let data = std::collections::BTreeMap::from([
        (
            "cursor".to_owned(),
            JsonValue::Number(state.cursor.to_string()),
        ),
        (
            "epoch".to_owned(),
            JsonValue::Number(state.epoch.to_string()),
        ),
        (
            "order".to_owned(),
            JsonValue::Array(
                state
                    .order
                    .iter()
                    .map(|value| JsonValue::Number(value.to_string()))
                    .collect(),
            ),
        ),
        (
            "rng_state".to_owned(),
            JsonValue::Number(state.rng_state.to_string()),
        ),
    ]);
    let weights_value = JsonValue::Object(weights);
    let data_value = JsonValue::Object(data);
    let weights_sha256 = sha256_hex(json::encode(&weights_value).as_bytes());
    let data_sha256 = sha256_hex(json::encode(&data_value).as_bytes());
    let state_value = JsonValue::Object(std::collections::BTreeMap::from([
        ("step".to_owned(), JsonValue::Number(state.step.to_string())),
        ("weights".to_owned(), weights_value.clone()),
        ("data".to_owned(), data_value.clone()),
    ]));
    let state_sha256 = sha256_hex(json::encode(&state_value).as_bytes());
    let checkpoint = JsonValue::Object(std::collections::BTreeMap::from([
        (
            "schema".to_owned(),
            JsonValue::String("seloryn.training.checkpoint.v2".to_owned()),
        ),
        ("model".to_owned(), JsonValue::String("mlp".to_owned())),
        ("step".to_owned(), JsonValue::Number(state.step.to_string())),
        ("seed".to_owned(), JsonValue::Number(seed.to_string())),
        (
            "input_size".to_owned(),
            JsonValue::Number(train.input_size.to_string()),
        ),
        (
            "hidden_size".to_owned(),
            JsonValue::Number(model.hidden_size.to_string()),
        ),
        (
            "output_size".to_owned(),
            JsonValue::Number(train.output_size.to_string()),
        ),
        (
            "batch_size".to_owned(),
            JsonValue::Number(batch_size.to_string()),
        ),
        (
            "learning_rate".to_owned(),
            JsonValue::Number(format_f32(learning_rate)),
        ),
        (
            "train_sha256".to_owned(),
            JsonValue::String(train.sha256.clone()),
        ),
        (
            "test_sha256".to_owned(),
            JsonValue::String(test.sha256.clone()),
        ),
        (
            "optimizer".to_owned(),
            JsonValue::Object(std::collections::BTreeMap::from([
                ("name".to_owned(), JsonValue::String("sgd".to_owned())),
                (
                    "state".to_owned(),
                    JsonValue::String("stateless".to_owned()),
                ),
            ])),
        ),
        (
            "weights_sha256".to_owned(),
            JsonValue::String(weights_sha256),
        ),
        ("data_sha256".to_owned(), JsonValue::String(data_sha256)),
        ("state_sha256".to_owned(), JsonValue::String(state_sha256)),
        ("weights".to_owned(), weights_value),
        ("data".to_owned(), data_value),
    ]));
    json::encode(&checkpoint)
}

fn write_checkpoint_atomic(path: &Path, source: &str) -> Result<String, String> {
    let parent = path
        .parent()
        .ok_or_else(|| "checkpoint path has no parent directory".to_owned())?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "checkpoint path has no valid file name".to_owned())?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "checkpoint temporary file clock is invalid".to_owned())?
        .as_nanos();
    let temporary = parent.join(format!(".{file_name}.tmp-{}-{nonce}", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| format!("checkpoint temporary file creation failed: {error}"))?;
    let write_result = file
        .write_all(source.as_bytes())
        .and_then(|()| file.flush())
        .and_then(|()| file.sync_all());
    if let Err(error) = write_result {
        drop(file);
        let _ = std::fs::remove_file(&temporary);
        return Err(format!("checkpoint temporary file flush failed: {error}"));
    }
    drop(file);
    if let Err(error) = replace_checkpoint_atomically(&temporary, path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    Ok(sha256_hex(source.as_bytes()))
}

/// Persist a program-produced JSON value without exposing arbitrary file I/O.
///
/// The interpreter has already resolved `path` against the manifest's write
/// capability before this helper runs. The same-file temporary/flush/rename
/// discipline used by native checkpoints is retained for source-owned model
/// artifacts.
fn write_json_atomic(path: &Path, source: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "JSON output path has no parent directory".to_owned())?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "JSON output path has no valid file name".to_owned())?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "JSON output temporary file clock is invalid".to_owned())?
        .as_nanos();
    let temporary = parent.join(format!(".{file_name}.tmp-{}-{nonce}", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| format!("JSON output temporary file creation failed: {error}"))?;
    let write_result = file
        .write_all(source.as_bytes())
        .and_then(|()| file.flush())
        .and_then(|()| file.sync_all());
    if let Err(error) = write_result {
        drop(file);
        let _ = std::fs::remove_file(&temporary);
        return Err(format!("JSON output temporary file flush failed: {error}"));
    }
    drop(file);
    if let Err(error) = replace_checkpoint_atomically(&temporary, path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(format!("JSON output atomic replacement failed: {error}"));
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_checkpoint_atomically(temporary: &Path, destination: &Path) -> Result<(), String> {
    std::fs::rename(temporary, destination)
        .map_err(|error| format!("checkpoint atomic rename failed: {error}"))
}

#[cfg(windows)]
fn replace_checkpoint_atomically(temporary: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };
    let temporary = temporary
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        MoveFileExW(
            temporary.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        return Err(format!(
            "checkpoint atomic replacement failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

fn json_f32_values(values: &[f32]) -> JsonValue {
    JsonValue::Array(
        values
            .iter()
            .map(|value| JsonValue::Number(format_f32(*value)))
            .collect(),
    )
}

fn json_f32_array(
    values: &std::collections::BTreeMap<String, JsonValue>,
    field: &str,
) -> Result<Vec<f32>, String> {
    let Some(JsonValue::Array(values)) = values.get(field) else {
        return Err(format!("MLP checkpoint field `{field}` is missing"));
    };
    values
        .iter()
        .map(|value| match value {
            JsonValue::Number(value) => {
                let parsed = value.parse::<f32>().map_err(|_| {
                    format!("MLP checkpoint field `{field}` contains an invalid number")
                })?;
                parsed.is_finite().then_some(parsed).ok_or_else(|| {
                    format!("MLP checkpoint field `{field}` contains a non-finite number")
                })
            }
            _ => Err(format!(
                "MLP checkpoint field `{field}` contains a non-number"
            )),
        })
        .collect()
}

fn json_usize_array(
    values: &std::collections::BTreeMap<String, JsonValue>,
    field: &str,
) -> Result<Vec<usize>, String> {
    let Some(JsonValue::Array(values)) = values.get(field) else {
        return Err(format!("MLP checkpoint field `{field}` is missing"));
    };
    values
        .iter()
        .map(|value| match value {
            JsonValue::Number(value) => value
                .parse::<usize>()
                .map_err(|_| format!("MLP checkpoint field `{field}` contains an invalid integer")),
            _ => Err(format!(
                "MLP checkpoint field `{field}` contains a non-integer"
            )),
        })
        .collect()
}

fn json_i64_field(
    values: &std::collections::BTreeMap<String, JsonValue>,
    field: &str,
) -> Result<i64, String> {
    match values.get(field) {
        Some(JsonValue::Number(value)) => value
            .parse()
            .map_err(|_| format!("MLP checkpoint field `{field}` is not an integer")),
        _ => Err(format!("MLP checkpoint field `{field}` is missing")),
    }
}

fn json_u64_field(
    values: &std::collections::BTreeMap<String, JsonValue>,
    field: &str,
) -> Result<u64, String> {
    match values.get(field) {
        Some(JsonValue::Number(value)) => value
            .parse::<u64>()
            .map_err(|_| format!("MLP checkpoint field `{field}` is not an unsigned integer")),
        _ => Err(format!("MLP checkpoint field `{field}` is missing")),
    }
}

fn json_usize_field(
    values: &std::collections::BTreeMap<String, JsonValue>,
    field: &str,
) -> Result<usize, String> {
    json_u64_field(values, field).and_then(|value| {
        usize::try_from(value).map_err(|_| format!("MLP checkpoint field `{field}` is too large"))
    })
}

fn json_f32_field(
    values: &std::collections::BTreeMap<String, JsonValue>,
    field: &str,
) -> Result<f32, String> {
    match values.get(field) {
        Some(JsonValue::Number(value)) => {
            let parsed = value
                .parse::<f32>()
                .map_err(|_| format!("MLP checkpoint field `{field}` is not a finite number"))?;
            parsed
                .is_finite()
                .then_some(parsed)
                .ok_or_else(|| format!("MLP checkpoint field `{field}` is not a finite number"))
        }
        _ => Err(format!("MLP checkpoint field `{field}` is missing")),
    }
}

fn shuffle_order(order: &mut [usize], state: &mut u64) {
    for index in (1..order.len()).rev() {
        let swap = (next_u64(state) as usize) % (index + 1);
        order.swap(index, swap);
    }
}

fn initial_mlp_rng_state(seed: i64) -> u64 {
    let state = (seed as u64) ^ MLP_RNG_SEED_MIX;
    if state == 0 {
        MLP_RNG_NONZERO_FALLBACK
    } else {
        state
    }
}

fn next_u64(state: &mut u64) -> u64 {
    let mut value = *state;
    value ^= value << 13;
    value ^= value >> 7;
    value ^= value << 17;
    *state = value;
    value
}

fn next_unit(state: &mut u64) -> f32 {
    (next_u64(state) as u32) as f32 / u32::MAX as f32
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn format_f32(value: f32) -> String {
    value.to_string()
}

fn read_xy_dataset(
    path: &Path,
    label: &str,
    json_max_bytes: usize,
    json_max_depth: usize,
) -> Result<(Vec<f64>, Vec<f64>, String), String> {
    let dataset_max_bytes = json_max_bytes.min(MAX_TRAINING_DATASET_BYTES);
    let dataset_max_depth = json_max_depth.min(16);
    let source = read_bounded_utf8(
        path,
        dataset_max_bytes,
        &format!("training dataset `{label}`"),
    )?;
    let value = json::parse(&source, dataset_max_bytes, dataset_max_depth)
        .map_err(|error| error.message)?;
    let JsonValue::Object(values) = value else {
        return Err("dataset must be a JSON object".to_owned());
    };
    let Some(JsonValue::Array(x_values)) = values.get("x") else {
        return Err("dataset is missing x".to_owned());
    };
    let Some(JsonValue::Array(y_values)) = values.get("y") else {
        return Err("dataset is missing y".to_owned());
    };
    let x = x_values
        .iter()
        .map(json_number)
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| "dataset x contains a non-number".to_owned())?;
    let y = y_values
        .iter()
        .map(json_number)
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| "dataset y contains a non-number".to_owned())?;
    if x.iter().chain(&y).any(|value| !value.is_finite()) {
        return Err("dataset contains a non-finite number".to_owned());
    }
    if x.len() > MAX_TRAINING_SAMPLES || y.len() > MAX_TRAINING_SAMPLES {
        return Err(format!(
            "dataset exceeds maximum {MAX_TRAINING_SAMPLES} samples"
        ));
    }
    if x.is_empty() || x.len() != y.len() {
        return Err("dataset x/y lengths are invalid".to_owned());
    }
    Ok((x, y, sha256_hex(source.as_bytes())))
}

fn read_bounded_utf8(path: &Path, max_bytes: usize, label: &str) -> Result<String, String> {
    let file =
        std::fs::File::open(path).map_err(|error| format!("{label} read failed: {error}"))?;
    if file
        .metadata()
        .map_err(|error| format!("{label} metadata read failed: {error}"))?
        .len()
        > max_bytes as u64
    {
        return Err(format!("{label} exceeds maximum {max_bytes} bytes"));
    }
    let mut bytes = Vec::new();
    file.take(max_bytes.saturating_add(1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("{label} read failed: {error}"))?;
    if bytes.len() > max_bytes {
        return Err(format!("{label} exceeds maximum {max_bytes} bytes"));
    }
    String::from_utf8(bytes).map_err(|_| format!("{label} is not valid UTF-8"))
}

fn json_number(value: &JsonValue) -> Option<f64> {
    match value {
        JsonValue::Number(value) => value.parse().ok(),
        _ => None,
    }
}

fn mean_squared_error(dataset: &(Vec<f64>, Vec<f64>, String), weight: f64, bias: f64) -> f64 {
    dataset
        .0
        .iter()
        .zip(&dataset.1)
        .map(|(x, y)| (weight * x + bias - y).powi(2))
        .sum::<f64>()
        / dataset.0.len() as f64
}

fn validate_checkpoint(
    source: &str,
    max_bytes: usize,
    max_depth: usize,
) -> Result<JsonValue, String> {
    let value = json::parse(source, max_bytes, max_depth).map_err(|error| error.message)?;
    let JsonValue::Object(ref values) = value else {
        return Err("checkpoint root must be an object".to_owned());
    };
    let valid_schema = matches!(
        values.get("schema"),
        Some(JsonValue::String(schema)) if schema == "seloryn.checkpoint.v1"
    );
    let valid_step =
        matches!(values.get("step"), Some(JsonValue::Number(step)) if step.parse::<i64>().is_ok());
    let valid_weights = matches!(values.get("weights"), Some(JsonValue::Object(_)));
    let valid_optimizer = matches!(values.get("optimizer"), Some(JsonValue::Object(_)));
    let valid_fixture = matches!(
        values.get("fixture"),
        Some(JsonValue::String(fixture)) if fixture == "schema-valid"
    );
    if valid_schema && valid_step && valid_weights && valid_optimizer && valid_fixture {
        Ok(value)
    } else {
        Err("checkpoint failed schema or type validation".to_owned())
    }
}

fn json_string(value: &str) -> String {
    json::encode(&JsonValue::String(value.to_owned()))
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

fn render_text(value: &str, remaining: &mut usize) -> String {
    let length = value.chars().count();
    let visible_length = length.min(*remaining);
    let visible = value.chars().take(visible_length).collect::<String>();
    *remaining -= visible_length;
    let suffix = if visible_length < length {
        format!("... ({} more chars)", length - visible_length)
    } else {
        String::new()
    };
    format!("\"{}{}\"", escape_text(&visible), suffix)
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_RENDER_VALUES, RuntimeError, Value, execute_ir_with_context, execute_with_output,
        initial_mlp_rng_state, next_u64,
    };
    use crate::parser::parse;
    use crate::semantic::check;
    use crate::tensor::Tensor;
    use std::path::Path;

    fn run(source: &str) -> Result<Value, RuntimeError> {
        let parsed = parse(source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let program = parsed.program.expect("program");
        let diagnostics = check(&program);
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        execute_with_output(&program).map(|result| result.value)
    }

    #[test]
    fn source_execution_matches_direct_ir_execution() {
        let source = "fn main() -> I64:\n    return 6 * 7\n";
        let parsed = parse(source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let program = parsed.program.expect("program");
        assert!(check(&program).is_empty());
        let ir = crate::ir::lower(&program);
        let from_source = execute_with_output(&program).expect("source execution");
        let from_ir =
            execute_ir_with_context(&ir, Path::new("."), 128, 4).expect("direct IR execution");
        assert_eq!(from_source, from_ir);
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
    fn bare_enum_constructor_does_not_match_a_different_variant() {
        let value = run(
            "enum Decision:\n    Accepted\n    Rejected(Text)\nfn classify(value: Decision) -> I64:\n    match value:\n        case Accepted:\n            return 1\n        case Rejected(reason):\n            return 2\nfn main() -> I64:\n    return classify(Rejected(\"below minimum\"))\n",
        )
        .expect("enum matching should execute");
        assert_eq!(value, Value::I64(2));
    }

    #[test]
    fn enum_variant_names_remain_bindings_for_other_value_types() {
        let value = run(
            "enum Marker:\n    Ready\nfn main() -> I64:\n    match 7:\n        case Ready:\n            return Ready\n",
        )
        .expect("a binding with a colliding name should execute");
        assert_eq!(value, Value::I64(7));
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

        let tokens = run(r#"from data import split_whitespace
fn main() -> List[Text]:
    return split_whitespace("  Seloryn   entende   texto  ")
"#)
        .expect("whitespace tokenization should execute");
        assert_eq!(tokens.render(), "[\"Seloryn\", \"entende\", \"texto\"]");
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

    #[test]
    fn gradcheck_evaluates_the_supplied_function() {
        let value = run(
            "from autograd import gradcheck\nfn f(x: F32) -> F32:\n    return x * x + x\nfn main() -> F32:\n    return gradcheck(f, 4.0)\n",
        )
        .expect("gradcheck must execute");
        let Value::F64(value) = value else {
            panic!("gradcheck must return a floating-point derivative");
        };
        assert!((value - 9.0).abs() < 0.01, "observed derivative {value}");
    }

    #[test]
    fn backward_executes_the_scalar_reverse_tape() {
        let value = run(
            "from autograd import backward\nfn f(x: F64) -> F64:\n    let square = x * x\n    return square + x / 2.0\nfn main() -> F64:\n    return backward(f, 4.0)\n",
        )
        .expect("backward must execute");
        let Value::F64(value) = value else {
            panic!("backward must return a scalar gradient");
        };
        assert!((value - 8.5).abs() < 1.0e-12, "observed gradient {value}");
    }

    #[test]
    fn backward_executes_the_tensor_reverse_tape() {
        let value = run(
            "from autograd import backward\nfn f(x: Tensor[F32]) -> Tensor[F32]:\n    return tensor_sum(x + x)\nfn main() -> Tensor[F32]:\n    let input: Tensor[F32] = tensor_f32([1.0, 2.0, 3.0], [1, 3])\n    return backward(f, input)\n",
        )
        .expect("tensor backward must execute");
        assert_eq!(
            value.render(),
            "Tensor[F32, shape=[1, 3], values=[2.0, 2.0, 2.0]]"
        );
    }

    #[test]
    fn truncates_large_list_rendering_deterministically() {
        let values = Value::List(
            (0..=MAX_RENDER_VALUES)
                .map(|value| Value::I64(value as i64))
                .collect(),
        );
        let rendered = values.render();
        assert!(rendered.starts_with("[0, 1, 2"));
        assert!(rendered.ends_with(", ... (1 more)]"));
        assert_eq!(rendered.matches(", ").count(), MAX_RENDER_VALUES);
    }

    #[test]
    fn truncates_large_record_rendering_deterministically() {
        let fields = (0..=MAX_RENDER_VALUES)
            .map(|index| (format!("field_{index}"), Value::I64(index as i64)))
            .collect();
        let record = Value::Record {
            name: "Wide".to_owned(),
            fields,
        };
        let rendered = record.render();
        assert!(rendered.starts_with("Wide(field_0: 0, field_1: 1"));
        assert!(rendered.ends_with(", ... (1 more))"));
        assert_eq!(rendered.matches(": ").count(), MAX_RENDER_VALUES);
    }

    #[test]
    fn shares_render_budget_across_text_and_nested_tensors() {
        let text = Value::Text("a".repeat(MAX_RENDER_VALUES + 1));
        assert!(text.render().ends_with("... (1 more chars)\""));

        let values = Value::List(vec![
            Value::Tensor(Tensor::zeros_f32(&[4097]).expect("first tensor")),
            Value::Tensor(Tensor::zeros_f32(&[1]).expect("second tensor")),
        ]);
        let rendered = values.render();
        assert!(rendered.contains("... (2 more)"));
        assert!(rendered.ends_with("... (1 more)]"));
    }

    #[test]
    fn mlp_rng_cannot_lock_to_the_zero_state() {
        let mut state = initial_mlp_rng_state(-7046029254386353131);
        assert_ne!(state, 0);
        let first = next_u64(&mut state);
        assert_ne!(first, 0);
        let second = next_u64(&mut state);
        assert_ne!(second, first);
    }
}
