//! Narrow scalar reverse-mode automatic differentiation.
//!
//! This module is deliberately smaller than a tensor autograd engine. It
//! records a real operation tape for one scalar input. F32/F64 annotations are
//! accepted by the checker, while the current scalar runtime representation is
//! f64; independent F32 rounding semantics are not claimed. It supports the
//! arithmetic expression subset documented by the public runtime, and fails
//! explicitly for unsupported constructs. It is used by `autograd.backward`
//! and cross-checked by the finite-difference `gradcheck` builtin.

use crate::language::ir::{Expr, Function, IfBranch, Item, Program, Statement};
use std::collections::HashMap;

const MAX_CALL_DEPTH: usize = 32;
const MAX_TAPE_NODES: usize = 1_000_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    pub message: String,
}

impl Error {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Scalar {
    value: f64,
    node: Option<usize>,
}

#[derive(Clone, Copy, Debug)]
enum TracedValue {
    Scalar(Scalar),
    Bool(bool),
    Unit,
}

#[derive(Clone, Debug)]
struct Node {
    parents: Vec<(usize, f64)>,
}

#[derive(Debug)]
struct Tape {
    nodes: Vec<Node>,
    input: usize,
}

impl Tape {
    fn new(value: f64) -> (Self, Scalar) {
        let mut tape = Self {
            nodes: Vec::new(),
            input: 0,
        };
        let input = tape.push_input(value);
        (tape, input)
    }

    fn push_input(&mut self, value: f64) -> Scalar {
        let node = self.nodes.len();
        self.nodes.push(Node {
            parents: Vec::new(),
        });
        self.input = node;
        Scalar {
            value,
            node: Some(node),
        }
    }

    fn constant(value: f64) -> Scalar {
        Scalar { value, node: None }
    }

    fn operation(&mut self, value: f64, parents: Vec<(usize, f64)>) -> Result<Scalar, Error> {
        if !value.is_finite()
            || parents
                .iter()
                .any(|(_, derivative)| !derivative.is_finite())
        {
            return Err(Error::new("autograd operation produced a non-finite value"));
        }
        if parents.is_empty() {
            return Ok(Self::constant(value));
        }
        if self.nodes.len() >= MAX_TAPE_NODES {
            return Err(Error::new(format!(
                "autograd tape exceeds maximum node count {MAX_TAPE_NODES}"
            )));
        }
        let node = self.nodes.len();
        self.nodes.push(Node { parents });
        Ok(Scalar {
            value,
            node: Some(node),
        })
    }

    fn unary(&mut self, value: Scalar, derivative: f64, output: f64) -> Result<Scalar, Error> {
        let parents = value
            .node
            .into_iter()
            .map(|node| (node, derivative))
            .collect();
        self.operation(output, parents)
    }

    fn binary(
        &mut self,
        left: Scalar,
        right: Scalar,
        left_derivative: f64,
        right_derivative: f64,
        output: f64,
    ) -> Result<Scalar, Error> {
        let mut parents = Vec::with_capacity(2);
        if let Some(node) = left.node {
            parents.push((node, left_derivative));
        }
        if let Some(node) = right.node {
            parents.push((node, right_derivative));
        }
        self.operation(output, parents)
    }

    fn gradient(&self, output: Scalar) -> Result<f64, Error> {
        let Some(output_node) = output.node else {
            return Ok(0.0);
        };
        let mut gradients = vec![0.0; self.nodes.len()];
        gradients[output_node] = 1.0;
        for node in (0..self.nodes.len()).rev() {
            let gradient = gradients[node];
            if gradient == 0.0 {
                continue;
            }
            for (parent, local_derivative) in &self.nodes[node].parents {
                gradients[*parent] += gradient * local_derivative;
                if !gradients[*parent].is_finite() {
                    return Err(Error::new(
                        "autograd gradient accumulation became non-finite",
                    ));
                }
            }
        }
        let gradient = gradients[self.input];
        gradient
            .is_finite()
            .then_some(gradient)
            .ok_or_else(|| Error::new("autograd gradient became non-finite"))
    }
}

struct Evaluator<'a> {
    functions: HashMap<&'a str, &'a Function>,
    tape: Tape,
}

impl<'a> Evaluator<'a> {
    fn new(program: &'a Program, input: f64) -> (Self, Scalar) {
        let functions = program
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Function(function) => Some((function.name.as_str(), function)),
                _ => None,
            })
            .collect();
        let (tape, input) = Tape::new(input);
        (Self { functions, tape }, input)
    }

    fn call(&mut self, name: &str, arguments: Vec<Scalar>, depth: usize) -> Result<Scalar, Error> {
        if depth >= MAX_CALL_DEPTH {
            return Err(Error::new(format!(
                "autograd call depth exceeds maximum {MAX_CALL_DEPTH}"
            )));
        }
        let function = self
            .functions
            .get(name)
            .copied()
            .ok_or_else(|| Error::new(format!("autograd function `{name}` is unavailable")))?;
        if function.parameters.len() != arguments.len() {
            return Err(Error::new(format!(
                "autograd function `{name}` expects {} argument(s), got {}",
                function.parameters.len(),
                arguments.len()
            )));
        }
        let mut locals = HashMap::new();
        for (parameter, argument) in function.parameters.iter().zip(arguments) {
            locals.insert(parameter.name.clone(), TracedValue::Scalar(argument));
        }
        for statement in &function.body {
            if let Some(value) = self.statement(statement, &mut locals, depth + 1)? {
                return match value {
                    TracedValue::Scalar(value) => Ok(value),
                    _ => Err(Error::new(format!(
                        "autograd function `{name}` must return a scalar"
                    ))),
                };
            }
        }
        Ok(Tape::constant(0.0))
    }

    fn statement(
        &mut self,
        statement: &Statement,
        locals: &mut HashMap<String, TracedValue>,
        depth: usize,
    ) -> Result<Option<TracedValue>, Error> {
        match statement {
            Statement::Binding(binding) => {
                let value = self.expression(&binding.value, locals, depth)?;
                locals.insert(binding.name.clone(), value);
                Ok(None)
            }
            Statement::Assignment { name, value, .. } => {
                if !locals.contains_key(name) {
                    return Err(Error::new(format!(
                        "autograd assignment references unknown name `{name}`"
                    )));
                }
                let value = self.expression(value, locals, depth)?;
                locals.insert(name.clone(), value);
                Ok(None)
            }
            Statement::Return { value, .. } => value
                .as_ref()
                .map(|value| self.expression(value, locals, depth).map(Some))
                .unwrap_or_else(|| Ok(Some(TracedValue::Unit))),
            Statement::If {
                branches,
                otherwise,
                ..
            } => self.if_statement(branches, otherwise.as_deref(), locals, depth),
            Statement::Expression(expression) => {
                let _ = self.expression(expression, locals, depth)?;
                Ok(None)
            }
            Statement::Require { condition, .. } => {
                let condition = self.expression(condition, locals, depth)?;
                match condition {
                    TracedValue::Bool(true) => Ok(None),
                    TracedValue::Bool(false) => {
                        Err(Error::new("autograd `require` condition was false"))
                    }
                    _ => Err(Error::new("autograd `require` condition must be Bool")),
                }
            }
            Statement::For { .. } | Statement::Match(_) => Err(Error::new(
                "autograd scalar evaluator does not support loops or match",
            )),
        }
    }

    fn if_statement(
        &mut self,
        branches: &[IfBranch],
        otherwise: Option<&[Statement]>,
        locals: &mut HashMap<String, TracedValue>,
        depth: usize,
    ) -> Result<Option<TracedValue>, Error> {
        for branch in branches {
            let condition = self.expression(&branch.condition, locals, depth)?;
            let TracedValue::Bool(condition) = condition else {
                return Err(Error::new("autograd `if` condition must be Bool"));
            };
            if condition {
                for statement in &branch.body {
                    if let Some(value) = self.statement(statement, locals, depth)? {
                        return Ok(Some(value));
                    }
                }
                return Ok(None);
            }
        }
        if let Some(body) = otherwise {
            for statement in body {
                if let Some(value) = self.statement(statement, locals, depth)? {
                    return Ok(Some(value));
                }
            }
        }
        Ok(None)
    }

    fn expression(
        &mut self,
        expression: &Expr,
        locals: &HashMap<String, TracedValue>,
        depth: usize,
    ) -> Result<TracedValue, Error> {
        match expression {
            Expr::Name { name, .. } => locals
                .get(name)
                .copied()
                .ok_or_else(|| Error::new(format!("autograd name `{name}` is unavailable"))),
            Expr::Integer { text, .. } => text
                .parse::<i64>()
                .map(|value| TracedValue::Scalar(Tape::constant(value as f64)))
                .map_err(|_| Error::new("autograd integer literal is invalid")),
            Expr::Float { text, .. } => text
                .parse::<f64>()
                .map_err(|_| Error::new("autograd floating literal is invalid"))
                .and_then(|value| {
                    value
                        .is_finite()
                        .then_some(TracedValue::Scalar(Tape::constant(value)))
                        .ok_or_else(|| Error::new("autograd floating literal is non-finite"))
                }),
            Expr::Group { value, .. } => self.expression(value, locals, depth),
            Expr::Unary {
                operator, value, ..
            } => {
                let evaluated = self.expression(value, locals, depth)?;
                let value = self.scalar(evaluated)?;
                match operator.as_str() {
                    "-" => self
                        .tape
                        .unary(value, -1.0, -value.value)
                        .map(TracedValue::Scalar),
                    _ => Err(Error::new(format!(
                        "autograd unary operator `{operator}` is unsupported"
                    ))),
                }
            }
            Expr::Binary {
                left,
                operator,
                right,
                ..
            } => self.binary(left, operator, right, locals, depth),
            Expr::Call {
                callee, arguments, ..
            } => {
                let Expr::Name { name, .. } = callee.as_ref() else {
                    return Err(Error::new("autograd call target must be a named function"));
                };
                let mut evaluated_arguments = Vec::with_capacity(arguments.len());
                for argument in arguments {
                    let evaluated = self.expression(argument, locals, depth)?;
                    evaluated_arguments.push(self.scalar(evaluated)?);
                }
                self.call(name, evaluated_arguments, depth)
                    .map(TracedValue::Scalar)
            }
            Expr::NamedArgument { value, .. } => self.expression(value, locals, depth),
            _ => Err(Error::new(
                "autograd scalar evaluator does not support this expression",
            )),
        }
    }

    fn binary(
        &mut self,
        left: &Expr,
        operator: &str,
        right: &Expr,
        locals: &HashMap<String, TracedValue>,
        depth: usize,
    ) -> Result<TracedValue, Error> {
        if matches!(operator, "and" | "or") {
            let evaluated_left = self.expression(left, locals, depth)?;
            let left = self.boolean(evaluated_left)?;
            if (operator == "and" && !left) || (operator == "or" && left) {
                return Ok(TracedValue::Bool(left));
            }
            let evaluated_right = self.expression(right, locals, depth)?;
            let right = self.boolean(evaluated_right)?;
            return Ok(TracedValue::Bool(right));
        }
        if matches!(operator, "==" | "!=" | "<" | "<=" | ">" | ">=") {
            let evaluated_left = self.expression(left, locals, depth)?;
            let left = self.scalar(evaluated_left)?;
            let evaluated_right = self.expression(right, locals, depth)?;
            let right = self.scalar(evaluated_right)?;
            let result = match operator {
                "==" => left.value == right.value,
                "!=" => left.value != right.value,
                "<" => left.value < right.value,
                "<=" => left.value <= right.value,
                ">" => left.value > right.value,
                ">=" => left.value >= right.value,
                _ => unreachable!(),
            };
            return Ok(TracedValue::Bool(result));
        }
        let evaluated_left = self.expression(left, locals, depth)?;
        let left = self.scalar(evaluated_left)?;
        let evaluated_right = self.expression(right, locals, depth)?;
        let right = self.scalar(evaluated_right)?;
        let result = match operator {
            "+" => self
                .tape
                .binary(left, right, 1.0, 1.0, left.value + right.value),
            "-" => self
                .tape
                .binary(left, right, 1.0, -1.0, left.value - right.value),
            "*" => self.tape.binary(
                left,
                right,
                right.value,
                left.value,
                left.value * right.value,
            ),
            "/" if right.value != 0.0 => self.tape.binary(
                left,
                right,
                1.0 / right.value,
                -left.value / (right.value * right.value),
                left.value / right.value,
            ),
            "/" => return Err(Error::new("autograd division by zero")),
            "**" if right.node.is_none() => {
                let output = left.value.powf(right.value);
                let derivative = if left.value == 0.0 && right.value < 1.0 {
                    f64::NAN
                } else {
                    right.value * left.value.powf(right.value - 1.0)
                };
                self.tape.unary(left, derivative, output)
            }
            "**" => {
                return Err(Error::new(
                    "autograd variable exponents are not supported in the scalar gate",
                ));
            }
            _ => {
                return Err(Error::new(format!(
                    "autograd binary operator `{operator}` is unsupported"
                )));
            }
        }?;
        Ok(TracedValue::Scalar(result))
    }

    fn scalar(&self, value: TracedValue) -> Result<Scalar, Error> {
        match value {
            TracedValue::Scalar(value) => Ok(value),
            _ => Err(Error::new("autograd expression expected a scalar")),
        }
    }

    fn boolean(&self, value: TracedValue) -> Result<bool, Error> {
        match value {
            TracedValue::Bool(value) => Ok(value),
            _ => Err(Error::new("autograd expression expected Bool")),
        }
    }
}

pub fn differentiate_scalar(
    program: &Program,
    function_name: &str,
    input: f64,
) -> Result<f64, Error> {
    if !input.is_finite() {
        return Err(Error::new("autograd input must be finite"));
    }
    let (mut evaluator, input) = Evaluator::new(program, input);
    let function = evaluator
        .functions
        .get(function_name)
        .copied()
        .ok_or_else(|| {
            Error::new(format!(
                "autograd function `{function_name}` is unavailable"
            ))
        })?;
    if function.parameters.len() != 1 {
        return Err(Error::new(
            "scalar backward requires exactly one function parameter",
        ));
    }
    let output = evaluator.call(function_name, vec![input], 0)?;
    evaluator.tape.gradient(output)
}

#[cfg(test)]
mod tests {
    use super::differentiate_scalar;
    use crate::language::ir;
    use crate::language::parser;

    fn gradient(source: &str) -> Result<f64, String> {
        let parsed = parser::parse(source);
        if !parsed.diagnostics.is_empty() {
            return Err(format!("parser diagnostics: {:?}", parsed.diagnostics));
        }
        let program = ir::lower(&parsed.program.expect("program"));
        differentiate_scalar(&program, "f", 4.0).map_err(|error| error.message)
    }

    #[test]
    fn differentiates_scalar_arithmetic_with_reverse_tape() {
        let result =
            gradient("fn f(x: F32) -> F32:\n    let square = x * x\n    return square + x / 2.0\n")
                .expect("reverse tape should evaluate");
        assert!((result - 8.5).abs() < 1.0e-12, "gradient={result}");
    }

    #[test]
    fn accumulates_shared_input_and_helper_calls() {
        let result = gradient(
            "fn twice(x: F64) -> F64:\n    return x + x\nfn f(x: F64) -> F64:\n    return twice(x) * x\n",
        )
        .expect("shared parents should accumulate");
        assert!((result - 16.0).abs() < 1.0e-12, "gradient={result}");
    }
}
