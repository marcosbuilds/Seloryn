//! Bounded reverse-mode automatic differentiation for F32 tensors.
//!
//! This is the first executable tensor-gradient gate. It deliberately shares
//! the existing CPU tensor kernels instead of pretending to be a general
//! device runtime: one F32 tensor input is traced through the implemented
//! pure tensor operations and the scalar (rank-zero) result is differentiated.
//! A call may expose several Tensor[F32] inputs and returns one gradient per
//! input in parameter order.
//! Every rule is explicit and the graph has a hard node limit.

use crate::ir::{Expr, Function, IfBranch, Item, Program, Statement};
use crate::tensor::{Tensor, TensorError};
use std::collections::HashMap;

const MAX_TAPE_NODES: usize = 100_000;
const MAX_CALL_DEPTH: usize = 32;

/// Explicit compatibility spellings for the tensor operations implemented by
/// this evaluator. New aliases are intentionally not invented here: a tensor
/// builtin name carries domain and dtype information, and shortening it must
/// not make a valid program harder to understand. `sum` remains registered
/// for backwards compatibility with the existing language spelling.
const TENSOR_ALIASES: &[(&str, &str)] = &[("sum", "tensor_sum")];

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

#[derive(Clone, Debug)]
enum TracedValue {
    Tensor { value: Tensor, node: Option<usize> },
    I64(i64),
    F64(f64),
    Bool(bool),
    List(Vec<TracedValue>),
    Unit,
}

impl TracedValue {
    fn tensor(&self) -> Result<(Tensor, Option<usize>), Error> {
        match self {
            Self::Tensor { value, node } => Ok((value.clone(), *node)),
            _ => Err(Error::new(
                "tensor autograd expression expected a Tensor[F32]",
            )),
        }
    }
}

#[derive(Clone, Debug)]
enum Rule {
    Leaf,
    Add {
        left: Option<usize>,
        right: Option<usize>,
        left_shape: Vec<i64>,
        right_shape: Vec<i64>,
    },
    Sub {
        left: Option<usize>,
        right: Option<usize>,
        left_shape: Vec<i64>,
        right_shape: Vec<i64>,
    },
    Mul {
        left: Option<usize>,
        right: Option<usize>,
        left_value: Tensor,
        right_value: Tensor,
        left_shape: Vec<i64>,
        right_shape: Vec<i64>,
    },
    Div {
        left: Option<usize>,
        right: Option<usize>,
        left_value: Tensor,
        right_value: Tensor,
        left_shape: Vec<i64>,
        right_shape: Vec<i64>,
    },
    Neg {
        input: Option<usize>,
    },
    Relu {
        input: Option<usize>,
        input_value: Tensor,
    },
    Exp {
        input: Option<usize>,
        output: Tensor,
    },
    Log {
        input: Option<usize>,
        input_value: Tensor,
    },
    Sum {
        input: Option<usize>,
        input_shape: Vec<i64>,
        axis: Option<usize>,
    },
    Mean {
        input: Option<usize>,
        input_shape: Vec<i64>,
        axis: Option<usize>,
        count: usize,
    },
    Matmul {
        left: Option<usize>,
        right: Option<usize>,
        left_value: Tensor,
        right_value: Tensor,
    },
    Transpose {
        input: Option<usize>,
    },
    Reshape {
        input: Option<usize>,
        input_shape: Vec<i64>,
    },
    Softmax {
        input: Option<usize>,
        output: Tensor,
        axis: usize,
    },
    LogSumExp {
        input: Option<usize>,
        input_value: Tensor,
        axis: usize,
    },
    CrossEntropy {
        logits: Option<usize>,
        targets: Option<usize>,
        targets_value: Tensor,
        probabilities: Tensor,
        log_probabilities: Tensor,
        rows: usize,
    },
    SumToShape {
        input: Option<usize>,
        input_shape: Vec<i64>,
    },
}

#[derive(Clone, Debug)]
struct Node {
    rule: Rule,
}

#[derive(Debug)]
struct Tape {
    nodes: Vec<Node>,
    inputs: Vec<usize>,
}

impl Tape {
    fn new_many(inputs: Vec<Tensor>) -> (Self, Vec<TracedValue>) {
        let mut tape = Self {
            nodes: Vec::new(),
            inputs: Vec::with_capacity(inputs.len()),
        };
        let traced = inputs
            .into_iter()
            .map(|input| {
                let node = tape
                    .push(input.clone(), Rule::Leaf)
                    .expect("leaf is bounded");
                tape.inputs.push(node);
                TracedValue::Tensor {
                    value: input,
                    node: Some(node),
                }
            })
            .collect();
        (tape, traced)
    }

    fn push(&mut self, _value: Tensor, rule: Rule) -> Result<usize, Error> {
        if self.nodes.len() >= MAX_TAPE_NODES {
            return Err(Error::new(format!(
                "tensor autograd tape exceeds maximum node count {MAX_TAPE_NODES}"
            )));
        }
        let node = self.nodes.len();
        self.nodes.push(Node { rule });
        Ok(node)
    }

    fn accumulate(
        gradients: &mut [Option<Tensor>],
        node: Option<usize>,
        gradient: Tensor,
    ) -> Result<(), Error> {
        let Some(node) = node else {
            return Ok(());
        };
        if let Some(existing) = &gradients[node] {
            gradients[node] = Some(add(existing, &gradient)?);
        } else {
            gradients[node] = Some(gradient);
        }
        Ok(())
    }

    fn gradients(&self, output: usize) -> Result<Vec<Tensor>, Error> {
        let mut gradients = vec![None; self.nodes.len()];
        let seed = scalar_one()?;
        gradients[output] = Some(seed);
        for node_index in (0..self.nodes.len()).rev() {
            let Some(gradient) = gradients[node_index].take() else {
                continue;
            };
            if !self.inputs.contains(&node_index) {
                self.apply_rule(&mut gradients, &self.nodes[node_index].rule, &gradient)?;
            } else if let Some(slot) = gradients.get_mut(node_index) {
                *slot = Some(gradient);
            }
        }
        self.inputs
            .iter()
            .map(|node| {
                gradients[*node]
                    .clone()
                    .ok_or_else(|| Error::new("tensor autograd input has no gradient"))
            })
            .collect()
    }

    fn apply_rule(
        &self,
        gradients: &mut [Option<Tensor>],
        rule: &Rule,
        gradient: &Tensor,
    ) -> Result<(), Error> {
        match rule {
            Rule::Leaf => Ok(()),
            Rule::Add {
                left,
                right,
                left_shape,
                right_shape,
            } => {
                Self::accumulate(gradients, *left, reduce_to(gradient, left_shape)?)?;
                Self::accumulate(gradients, *right, reduce_to(gradient, right_shape)?)
            }
            Rule::Sub {
                left,
                right,
                left_shape,
                right_shape,
            } => {
                Self::accumulate(gradients, *left, reduce_to(gradient, left_shape)?)?;
                let negative = multiply(gradient, &scalar_minus_one()?)?;
                Self::accumulate(gradients, *right, reduce_to(&negative, right_shape)?)
            }
            Rule::Mul {
                left,
                right,
                left_value,
                right_value,
                left_shape,
                right_shape,
            } => {
                let left_gradient = reduce_to(&multiply(gradient, right_value)?, left_shape)?;
                let right_gradient = reduce_to(&multiply(gradient, left_value)?, right_shape)?;
                Self::accumulate(gradients, *left, left_gradient)?;
                Self::accumulate(gradients, *right, right_gradient)
            }
            Rule::Div {
                left,
                right,
                left_value,
                right_value,
                left_shape,
                right_shape,
            } => {
                let left_gradient = reduce_to(&divide(gradient, right_value)?, left_shape)?;
                let denominator = multiply(right_value, right_value)?;
                let numerator = multiply(gradient, left_value)?;
                let right_gradient = reduce_to(
                    &multiply(&divide(&numerator, &denominator)?, &scalar_minus_one()?)?,
                    right_shape,
                )?;
                Self::accumulate(gradients, *left, left_gradient)?;
                Self::accumulate(gradients, *right, right_gradient)
            }
            Rule::Neg { input } => {
                Self::accumulate(gradients, *input, multiply(gradient, &scalar_minus_one()?)?)
            }
            Rule::Relu { input, input_value } => {
                let values = input_value.values_f32().map_err(tensor_error)?;
                let upstream = gradient.values_f32().map_err(tensor_error)?;
                let values = values
                    .into_iter()
                    .zip(upstream)
                    .map(|(value, gradient)| if value > 0.0 { gradient } else { 0.0 })
                    .collect::<Vec<_>>();
                let gradient =
                    Tensor::from_f32(values, &shape(input_value)?).map_err(tensor_error)?;
                Self::accumulate(gradients, *input, gradient)
            }
            Rule::Exp { input, output } => {
                Self::accumulate(gradients, *input, multiply(gradient, output)?)
            }
            Rule::Log { input, input_value } => {
                let ones = ones_like(input_value)?;
                Self::accumulate(
                    gradients,
                    *input,
                    multiply(gradient, &divide(&ones, input_value)?)?,
                )
            }
            Rule::Sum {
                input,
                input_shape,
                axis,
            } => Self::accumulate(
                gradients,
                *input,
                expand_reduction(gradient, input_shape, *axis)?,
            ),
            Rule::Mean {
                input,
                input_shape,
                axis,
                count,
            } => {
                let expanded = expand_reduction(gradient, input_shape, *axis)?;
                let scale =
                    Tensor::from_f32(vec![1.0 / *count as f32], &[]).map_err(tensor_error)?;
                Self::accumulate(gradients, *input, multiply(&expanded, &scale)?)
            }
            Rule::Matmul {
                left,
                right,
                left_value,
                right_value,
            } => {
                let left_gradient = matmul(gradient, &transpose(right_value)?)?;
                let right_gradient = matmul(&transpose(left_value)?, gradient)?;
                Self::accumulate(gradients, *left, left_gradient)?;
                Self::accumulate(gradients, *right, right_gradient)
            }
            Rule::Transpose { input } => Self::accumulate(gradients, *input, transpose(gradient)?),
            Rule::Reshape { input, input_shape } => {
                Self::accumulate(gradients, *input, reshape(gradient, input_shape)?)
            }
            Rule::Softmax {
                input,
                output,
                axis,
            } => {
                let product = multiply(gradient, output)?;
                let summed = sum(&product, Some(*axis))?;
                let expanded = expand_reduction(&summed, &shape(output)?, Some(*axis))?;
                let local = multiply(output, &subtract(gradient, &expanded)?)?;
                Self::accumulate(gradients, *input, local)
            }
            Rule::LogSumExp {
                input,
                input_value,
                axis,
            } => {
                let probabilities = input_value.softmax(*axis).map_err(tensor_error)?;
                let expanded = expand_reduction(gradient, &shape(input_value)?, Some(*axis))?;
                Self::accumulate(gradients, *input, multiply(&probabilities, &expanded)?)
            }
            Rule::CrossEntropy {
                logits,
                targets,
                targets_value,
                probabilities,
                log_probabilities,
                rows,
            } => {
                let expanded = broadcast(gradient, &shape(probabilities)?)?;
                let scale =
                    Tensor::from_f32(vec![1.0 / *rows as f32], &[]).map_err(tensor_error)?;
                let scaled = multiply(&expanded, &scale)?;
                let logits_gradient = multiply(&subtract(probabilities, targets_value)?, &scaled)?;
                let targets_gradient =
                    multiply(&multiply(log_probabilities, &scaled)?, &scalar_minus_one()?)?;
                Self::accumulate(gradients, *logits, logits_gradient)?;
                Self::accumulate(gradients, *targets, targets_gradient)
            }
            Rule::SumToShape { input, input_shape } => {
                Self::accumulate(gradients, *input, broadcast(gradient, input_shape)?)
            }
        }
    }
}

struct Evaluator<'a> {
    functions: HashMap<&'a str, &'a Function>,
    tape: Tape,
}

impl<'a> Evaluator<'a> {
    fn new_many(program: &'a Program, inputs: Vec<Tensor>) -> (Self, Vec<TracedValue>) {
        let functions = program
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Function(function) => Some((function.name.as_str(), function)),
                _ => None,
            })
            .collect();
        let (tape, inputs) = Tape::new_many(inputs);
        (Self { functions, tape }, inputs)
    }

    fn call(
        &mut self,
        name: &str,
        arguments: Vec<TracedValue>,
        depth: usize,
    ) -> Result<TracedValue, Error> {
        if depth >= MAX_CALL_DEPTH {
            return Err(Error::new(format!(
                "tensor autograd call depth exceeds maximum {MAX_CALL_DEPTH}"
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
            locals.insert(parameter.name.clone(), argument);
        }
        for statement in &function.body {
            if let Some(value) = self.statement(statement, &mut locals, depth + 1)? {
                return Ok(value);
            }
        }
        Ok(TracedValue::Unit)
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
                let value = self.expression(value, locals, depth)?;
                if !locals.contains_key(name) {
                    return Err(Error::new(format!(
                        "autograd assignment references unknown name `{name}`"
                    )));
                }
                locals.insert(name.clone(), value);
                Ok(None)
            }
            Statement::Return { value, .. } => Ok(Some(match value {
                Some(value) => self.expression(value, locals, depth)?,
                None => TracedValue::Unit,
            })),
            Statement::Require { condition, .. } => {
                if !matches!(
                    self.expression(condition, locals, depth)?,
                    TracedValue::Bool(true)
                ) {
                    return Err(Error::new("tensor autograd `require` condition was false"));
                }
                Ok(None)
            }
            Statement::If {
                branches,
                otherwise,
                ..
            } => self.if_statement(branches, otherwise.as_deref(), locals, depth),
            Statement::For { .. } | Statement::Match(_) => Err(Error::new(
                "tensor autograd evaluator does not support loops or match",
            )),
            Statement::Expression(expression) => {
                let _ = self.expression(expression, locals, depth)?;
                Ok(None)
            }
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
            if matches!(
                self.expression(&branch.condition, locals, depth)?,
                TracedValue::Bool(true)
            ) {
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
                .cloned()
                .ok_or_else(|| Error::new(format!("autograd name `{name}` is unavailable"))),
            Expr::Integer { text, .. } => text
                .parse::<i64>()
                .map(TracedValue::I64)
                .map_err(|_| Error::new("autograd integer literal is invalid")),
            Expr::Float { text, .. } => text
                .parse::<f64>()
                .map(TracedValue::F64)
                .map_err(|_| Error::new("autograd floating literal is invalid")),
            Expr::Boolean { value, .. } => Ok(TracedValue::Bool(*value)),
            Expr::List { values, .. } => Ok(TracedValue::List(
                values
                    .iter()
                    .map(|value| self.expression(value, locals, depth))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            Expr::Group { value, .. } => self.expression(value, locals, depth),
            Expr::Unary {
                operator, value, ..
            } => {
                let value = self.expression(value, locals, depth)?;
                if operator == "-" {
                    let (tensor, node) = value.tensor()?;
                    let output = tensor
                        .elementwise(&scalar_minus_one()?, "*")
                        .map_err(tensor_error)?;
                    let node = self.tape.push(output.clone(), Rule::Neg { input: node })?;
                    Ok(TracedValue::Tensor {
                        value: output,
                        node: Some(node),
                    })
                } else {
                    Err(Error::new(format!(
                        "tensor autograd unary operator `{operator}` is unsupported"
                    )))
                }
            }
            Expr::Binary {
                left,
                operator,
                right,
                ..
            } => {
                let left = self.expression(left, locals, depth)?;
                let right = self.expression(right, locals, depth)?;
                self.binary(left, operator, right)
            }
            Expr::Call {
                callee, arguments, ..
            } => {
                let Expr::Name { name, .. } = callee.as_ref() else {
                    return Err(Error::new(
                        "tensor autograd call target must be a named function",
                    ));
                };
                let values = arguments
                    .iter()
                    .map(|argument| self.expression(argument, locals, depth))
                    .collect::<Result<Vec<_>, _>>()?;
                let canonical_name = canonical_tensor_name(name)?;
                if is_tensor_builtin(canonical_name) {
                    self.builtin(canonical_name, values)
                } else {
                    self.call(name, values, depth)
                }
            }
            Expr::NamedArgument { value, .. } => self.expression(value, locals, depth),
            _ => Err(Error::new(
                "tensor autograd evaluator does not support this expression",
            )),
        }
    }

    fn binary(
        &mut self,
        left: TracedValue,
        operator: &str,
        right: TracedValue,
    ) -> Result<TracedValue, Error> {
        let (left_value, left_node) = left.tensor()?;
        let (right_value, right_node) = right.tensor()?;
        let output = match operator {
            "+" => add(&left_value, &right_value)?,
            "-" => subtract(&left_value, &right_value)?,
            "*" => multiply(&left_value, &right_value)?,
            "/" => divide(&left_value, &right_value)?,
            _ => {
                return Err(Error::new(format!(
                    "tensor autograd operator `{operator}` is unsupported"
                )));
            }
        };
        let left_shape = shape(&left_value)?;
        let right_shape = shape(&right_value)?;
        let rule = match operator {
            "+" => Rule::Add {
                left: left_node,
                right: right_node,
                left_shape,
                right_shape,
            },
            "-" => Rule::Sub {
                left: left_node,
                right: right_node,
                left_shape,
                right_shape,
            },
            "*" => Rule::Mul {
                left: left_node,
                right: right_node,
                left_value,
                right_value,
                left_shape,
                right_shape,
            },
            "/" => Rule::Div {
                left: left_node,
                right: right_node,
                left_value,
                right_value,
                left_shape,
                right_shape,
            },
            _ => unreachable!(),
        };
        let node = self.tape.push(output.clone(), rule)?;
        Ok(TracedValue::Tensor {
            value: output,
            node: Some(node),
        })
    }

    fn builtin(&mut self, name: &str, values: Vec<TracedValue>) -> Result<TracedValue, Error> {
        let tensor_arg = |index: usize| -> Result<(Tensor, Option<usize>), Error> {
            values
                .get(index)
                .ok_or_else(|| {
                    Error::new(format!("tensor builtin `{name}` is missing an argument"))
                })?
                .tensor()
        };
        let axis = |index: usize| -> Result<usize, Error> {
            match values.get(index) {
                Some(TracedValue::I64(value)) => usize::try_from(*value)
                    .map_err(|_| Error::new("tensor autograd axis must be nonnegative")),
                _ => Err(Error::new("tensor autograd axis must be I64")),
            }
        };
        match name {
            "tensor_sum" | "sum" => {
                let (input, input_node) = tensor_arg(0)?;
                let reduction_axis = if values.len() == 2 {
                    Some(axis(1)?)
                } else {
                    None
                };
                let output = input.sum_axis(reduction_axis).map_err(tensor_error)?;
                let node = self.tape.push(
                    output.clone(),
                    Rule::Sum {
                        input: input_node,
                        input_shape: shape(&input)?,
                        axis: reduction_axis,
                    },
                )?;
                Ok(TracedValue::Tensor {
                    value: output,
                    node: Some(node),
                })
            }
            "tensor_mean" => {
                let (input, input_node) = tensor_arg(0)?;
                let reduction_axis = if values.len() == 2 {
                    Some(axis(1)?)
                } else {
                    None
                };
                let count = reduction_count(&input, reduction_axis)?;
                let output = input.mean_axis(reduction_axis).map_err(tensor_error)?;
                let node = self.tape.push(
                    output.clone(),
                    Rule::Mean {
                        input: input_node,
                        input_shape: shape(&input)?,
                        axis: reduction_axis,
                        count,
                    },
                )?;
                Ok(TracedValue::Tensor {
                    value: output,
                    node: Some(node),
                })
            }
            "matmul" => {
                let (left, left_node) = tensor_arg(0)?;
                let (right, right_node) = tensor_arg(1)?;
                let output = left.matmul(&right).map_err(tensor_error)?;
                let node = self.tape.push(
                    output.clone(),
                    Rule::Matmul {
                        left: left_node,
                        right: right_node,
                        left_value: left,
                        right_value: right,
                    },
                )?;
                Ok(TracedValue::Tensor {
                    value: output,
                    node: Some(node),
                })
            }
            "transpose" => {
                let (input, input_node) = tensor_arg(0)?;
                let output = input.transpose().map_err(tensor_error)?;
                let node = self
                    .tape
                    .push(output.clone(), Rule::Transpose { input: input_node })?;
                Ok(TracedValue::Tensor {
                    value: output,
                    node: Some(node),
                })
            }
            "tensor_relu" => {
                let (input, input_node) = tensor_arg(0)?;
                let output = input.relu().map_err(tensor_error)?;
                let node = self.tape.push(
                    output.clone(),
                    Rule::Relu {
                        input: input_node,
                        input_value: input,
                    },
                )?;
                Ok(TracedValue::Tensor {
                    value: output,
                    node: Some(node),
                })
            }
            "tensor_exp" => {
                let (input, input_node) = tensor_arg(0)?;
                let output = input.exp().map_err(tensor_error)?;
                let node = self.tape.push(
                    output.clone(),
                    Rule::Exp {
                        input: input_node,
                        output: output.clone(),
                    },
                )?;
                Ok(TracedValue::Tensor {
                    value: output,
                    node: Some(node),
                })
            }
            "tensor_log" => {
                let (input, input_node) = tensor_arg(0)?;
                let output = input.log().map_err(tensor_error)?;
                let node = self.tape.push(
                    output.clone(),
                    Rule::Log {
                        input: input_node,
                        input_value: input,
                    },
                )?;
                Ok(TracedValue::Tensor {
                    value: output,
                    node: Some(node),
                })
            }
            "tensor_softmax" => {
                let (input, input_node) = tensor_arg(0)?;
                let reduction_axis = axis(1)?;
                let output = input.softmax(reduction_axis).map_err(tensor_error)?;
                let node = self.tape.push(
                    output.clone(),
                    Rule::Softmax {
                        input: input_node,
                        output: output.clone(),
                        axis: reduction_axis,
                    },
                )?;
                Ok(TracedValue::Tensor {
                    value: output,
                    node: Some(node),
                })
            }
            "tensor_logsumexp" => {
                let (input, input_node) = tensor_arg(0)?;
                let reduction_axis = axis(1)?;
                let output = input.logsumexp(reduction_axis).map_err(tensor_error)?;
                let node = self.tape.push(
                    output.clone(),
                    Rule::LogSumExp {
                        input: input_node,
                        input_value: input,
                        axis: reduction_axis,
                    },
                )?;
                Ok(TracedValue::Tensor {
                    value: output,
                    node: Some(node),
                })
            }
            "tensor_cross_entropy" => {
                let (logits, logits_node) = tensor_arg(0)?;
                let (targets, targets_node) = tensor_arg(1)?;
                let reduction_axis = axis(2)?;
                let (output, probabilities, log_probabilities) = logits
                    .cross_entropy_with_cache(&targets, reduction_axis)
                    .map_err(tensor_error)?;
                let rows = logits
                    .shape()
                    .first()
                    .copied()
                    .ok_or_else(|| Error::new("cross_entropy requires rank-2 logits"))?;
                let node = self.tape.push(
                    output.clone(),
                    Rule::CrossEntropy {
                        logits: logits_node,
                        targets: targets_node,
                        targets_value: targets.clone(),
                        probabilities,
                        log_probabilities,
                        rows,
                    },
                )?;
                Ok(TracedValue::Tensor {
                    value: output,
                    node: Some(node),
                })
            }
            "tensor_sum_to_shape" => {
                let (input, input_node) = tensor_arg(0)?;
                let target = shape_argument(values.get(1))?;
                let output = input.sum_to_shape(&target).map_err(tensor_error)?;
                let node = self.tape.push(
                    output.clone(),
                    Rule::SumToShape {
                        input: input_node,
                        input_shape: shape(&input)?,
                    },
                )?;
                Ok(TracedValue::Tensor {
                    value: output,
                    node: Some(node),
                })
            }
            "reshape" => {
                let (input, input_node) = tensor_arg(0)?;
                let target = shape_argument(values.get(1))?;
                let output = input.reshape(&target).map_err(tensor_error)?;
                let node = self.tape.push(
                    output.clone(),
                    Rule::Reshape {
                        input: input_node,
                        input_shape: shape(&input)?,
                    },
                )?;
                Ok(TracedValue::Tensor {
                    value: output,
                    node: Some(node),
                })
            }
            "tensor_f32" => {
                let data = f32_list(values.first())?;
                let target = shape_argument(values.get(1))?;
                let output = Tensor::from_f32(data, &target).map_err(tensor_error)?;
                Ok(TracedValue::Tensor {
                    value: output,
                    node: None,
                })
            }
            "tensor_zeros_f32" => {
                let target = shape_argument(values.first())?;
                let output = Tensor::zeros_f32(&target).map_err(tensor_error)?;
                Ok(TracedValue::Tensor {
                    value: output,
                    node: None,
                })
            }
            _ => Err(Error::new(format!(
                "tensor autograd builtin `{name}` is unsupported"
            ))),
        }
    }
}

pub fn differentiate_tensor(
    program: &Program,
    function_name: &str,
    input: Tensor,
) -> Result<Tensor, Error> {
    differentiate_tensors(program, function_name, vec![input])?
        .into_iter()
        .next()
        .ok_or_else(|| Error::new("tensor backward input has no gradient"))
}

pub fn differentiate_tensors(
    program: &Program,
    function_name: &str,
    inputs: Vec<Tensor>,
) -> Result<Vec<Tensor>, Error> {
    if inputs.is_empty() {
        return Err(Error::new("tensor gradients requires at least one input"));
    }
    if inputs
        .iter()
        .any(|input| input.dtype() != crate::tensor::TensorDtype::F32)
    {
        return Err(Error::new("tensor gradients requires Tensor[F32] inputs"));
    }
    let (mut evaluator, input_values) = Evaluator::new_many(program, inputs);
    let function = evaluator
        .functions
        .get(function_name)
        .copied()
        .ok_or_else(|| {
            Error::new(format!(
                "autograd function `{function_name}` is unavailable"
            ))
        })?;
    if function.parameters.len() != input_values.len() {
        return Err(Error::new(
            "tensor gradients input count must match function parameters",
        ));
    }
    let output = evaluator.call(function_name, input_values, 0)?;
    let (output, output_node) = output.tensor()?;
    if !output.shape().is_empty() {
        return Err(Error::new(format!(
            "tensor backward requires a rank-zero output, got shape {:?}",
            output.shape()
        )));
    }
    let output_node = output_node
        .ok_or_else(|| Error::new("tensor backward output is not connected to input"))?;
    evaluator.tape.gradients(output_node)
}

fn is_tensor_builtin(name: &str) -> bool {
    TENSOR_BUILTINS.contains(&name)
}

const TENSOR_BUILTINS: &[&str] = &[
    "tensor_sum",
    "tensor_mean",
    "matmul",
    "transpose",
    "tensor_relu",
    "tensor_exp",
    "tensor_log",
    "tensor_softmax",
    "tensor_logsumexp",
    "tensor_cross_entropy",
    "tensor_sum_to_shape",
    "reshape",
    "tensor_f32",
    "tensor_zeros_f32",
];

fn canonical_tensor_name(name: &str) -> Result<&str, Error> {
    let mut target = None;
    for (alias, canonical) in TENSOR_ALIASES {
        if *alias != name {
            continue;
        }
        if let Some(previous) = target {
            if previous != *canonical {
                return Err(Error::new(format!(
                    "tensor alias `{name}` is ambiguous: `{previous}` or `{canonical}`"
                )));
            }
        } else {
            target = Some(*canonical);
        }
    }
    Ok(target.unwrap_or(name))
}

fn tensor_error(error: TensorError) -> Error {
    Error::new(error.to_string())
}

fn shape(tensor: &Tensor) -> Result<Vec<i64>, Error> {
    tensor
        .shape()
        .iter()
        .map(|value| i64::try_from(*value).map_err(|_| Error::new("tensor shape exceeds I64")))
        .collect()
}

fn add(left: &Tensor, right: &Tensor) -> Result<Tensor, Error> {
    left.elementwise(right, "+").map_err(tensor_error)
}

fn subtract(left: &Tensor, right: &Tensor) -> Result<Tensor, Error> {
    left.elementwise(right, "-").map_err(tensor_error)
}

fn multiply(left: &Tensor, right: &Tensor) -> Result<Tensor, Error> {
    left.elementwise(right, "*").map_err(tensor_error)
}

fn divide(left: &Tensor, right: &Tensor) -> Result<Tensor, Error> {
    left.elementwise(right, "/").map_err(tensor_error)
}

fn matmul(left: &Tensor, right: &Tensor) -> Result<Tensor, Error> {
    left.matmul(right).map_err(tensor_error)
}

fn transpose(tensor: &Tensor) -> Result<Tensor, Error> {
    tensor.transpose().map_err(tensor_error)
}

fn reshape(tensor: &Tensor, target: &[i64]) -> Result<Tensor, Error> {
    tensor.reshape(target).map_err(tensor_error)
}

fn sum(tensor: &Tensor, axis: Option<usize>) -> Result<Tensor, Error> {
    tensor.sum_axis(axis).map_err(tensor_error)
}

fn broadcast(tensor: &Tensor, target: &[i64]) -> Result<Tensor, Error> {
    tensor.broadcast_to(target).map_err(tensor_error)
}

fn reduce_to(tensor: &Tensor, target: &[i64]) -> Result<Tensor, Error> {
    if shape(tensor)? == target {
        Ok(tensor.clone())
    } else {
        tensor.sum_to_shape(target).map_err(tensor_error)
    }
}

fn scalar_one() -> Result<Tensor, Error> {
    Tensor::from_f32(vec![1.0], &[]).map_err(tensor_error)
}

fn scalar_minus_one() -> Result<Tensor, Error> {
    Tensor::from_f32(vec![-1.0], &[]).map_err(tensor_error)
}

fn ones_like(tensor: &Tensor) -> Result<Tensor, Error> {
    let values = vec![1.0; tensor.element_count()];
    Tensor::from_f32(values, &shape(tensor)?).map_err(tensor_error)
}

fn expand_reduction(
    gradient: &Tensor,
    input_shape: &[i64],
    axis: Option<usize>,
) -> Result<Tensor, Error> {
    match axis {
        None => broadcast(gradient, input_shape),
        Some(axis) => {
            let mut expanded_shape = input_shape.to_vec();
            expanded_shape[axis] = 1;
            let reshaped = reshape(gradient, &expanded_shape)?;
            broadcast(&reshaped, input_shape)
        }
    }
}

fn reduction_count(tensor: &Tensor, axis: Option<usize>) -> Result<usize, Error> {
    let count = axis
        .map(|axis| tensor.shape().get(axis).copied().unwrap_or(0))
        .unwrap_or_else(|| tensor.element_count());
    (count > 0)
        .then_some(count)
        .ok_or_else(|| Error::new("tensor mean of an empty reduction is undefined"))
}

fn shape_argument(value: Option<&TracedValue>) -> Result<Vec<i64>, Error> {
    let Some(TracedValue::List(values)) = value else {
        return Err(Error::new("tensor shape must be a List[I64]"));
    };
    values
        .iter()
        .map(|value| match value {
            TracedValue::I64(value) => Ok(*value),
            _ => Err(Error::new("tensor shape must contain only I64 values")),
        })
        .collect()
}

fn f32_list(value: Option<&TracedValue>) -> Result<Vec<f32>, Error> {
    let Some(TracedValue::List(values)) = value else {
        return Err(Error::new("tensor values must be a List of numbers"));
    };
    values
        .iter()
        .map(|value| match value {
            TracedValue::F64(value) if value.is_finite() => Ok(*value as f32),
            TracedValue::I64(value) => Ok(*value as f32),
            _ => Err(Error::new("tensor values must contain finite numbers")),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        TENSOR_ALIASES, TENSOR_BUILTINS, canonical_tensor_name, differentiate_tensor,
        differentiate_tensors,
    };
    use crate::{ir, parser, tensor::Tensor};

    fn gradient(source: &str, values: Vec<f32>, shape: &[i64]) -> Tensor {
        let parsed = parser::parse(source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let program = ir::lower(&parsed.program.expect("program"));
        let input = Tensor::from_f32(values, shape).expect("input tensor");
        differentiate_tensor(&program, "f", input).expect("tensor gradient")
    }

    #[test]
    fn short_alias_registry_is_unique_and_targets_supported_builtins() {
        for (index, (alias, canonical)) in TENSOR_ALIASES.iter().enumerate() {
            assert!(
                TENSOR_BUILTINS.contains(canonical),
                "alias `{alias}` targets unknown builtin `{canonical}`"
            );
            assert!(
                !TENSOR_BUILTINS.contains(alias),
                "alias `{alias}` shadows canonical builtin"
            );
            assert_eq!(
                canonical_tensor_name(alias).expect("alias resolution"),
                *canonical
            );
            assert!(
                TENSOR_ALIASES[index + 1..]
                    .iter()
                    .all(|(other, _)| other != alias),
                "alias `{alias}` is registered more than once"
            );
        }
    }

    #[test]
    fn differentiates_broadcasted_sum_of_squares() {
        let result = gradient(
            "fn f(x: Tensor[F32]) -> Tensor[F32]:\n    return tensor_sum(x * x)\n",
            vec![1.0, 2.0, 3.0],
            &[1, 3],
        );
        assert_eq!(result.values_f32().expect("values"), vec![2.0, 4.0, 6.0]);
    }

    #[test]
    fn differentiates_matmul_and_mean() {
        let result = gradient(
            "fn f(x: Tensor[F32]) -> Tensor[F32]:\n    let weights: Tensor[F32] = tensor_f32([2.0, 3.0, 4.0, 5.0], [2, 2])\n    return tensor_mean(matmul(x, weights))\n",
            vec![1.0, 2.0, 3.0, 4.0],
            &[2, 2],
        );
        let values = result.values_f32().expect("values");
        assert_eq!(values, vec![1.25, 2.25, 1.25, 2.25]);
    }

    #[test]
    fn differentiates_multiple_tensor_inputs() {
        let source = "fn f(left: Tensor[F32], right: Tensor[F32]) -> Tensor[F32]:\n    return tensor_sum(left * right)\n";
        let parsed = parser::parse(source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let program = ir::lower(&parsed.program.expect("program"));
        let gradients = differentiate_tensors(
            &program,
            "f",
            vec![
                Tensor::from_f32(vec![2.0, 3.0], &[2]).expect("left tensor"),
                Tensor::from_f32(vec![5.0, 7.0], &[2]).expect("right tensor"),
            ],
        )
        .expect("tensor gradients");
        assert_eq!(gradients.len(), 2);
        assert_eq!(
            gradients[0].values_f32().expect("left gradient"),
            vec![5.0, 7.0]
        );
        assert_eq!(
            gradients[1].values_f32().expect("right gradient"),
            vec![2.0, 3.0]
        );
    }

    #[test]
    fn differentiates_fused_cross_entropy_for_logits_and_targets() {
        let source = "fn f(logits: Tensor[F32], targets: Tensor[F32]) -> Tensor[F32]:\n    return tensor_cross_entropy(logits, targets, 1)\n";
        let parsed = parser::parse(source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let program = ir::lower(&parsed.program.expect("program"));
        let gradients = differentiate_tensors(
            &program,
            "f",
            vec![
                Tensor::from_f32(vec![0.0, 1.0, 2.0, 0.0], &[2, 2]).expect("logits"),
                Tensor::from_f32(vec![0.0, 1.0, 1.0, 0.0], &[2, 2]).expect("targets"),
            ],
        )
        .expect("cross-entropy gradients");
        let logits = gradients[0].values_f32().expect("logits gradient");
        let expected = [0.1344707, -0.1344707, -0.0596015, 0.0596015];
        for (actual, expected) in logits.iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-5, "{actual} != {expected}");
        }
        let targets = gradients[1].values_f32().expect("target gradient");
        assert!(targets.iter().all(|value| value.is_finite()));

        let epsilon = 1.0e-3f32;
        let base = vec![0.0f32, 1.0, 2.0, 0.0];
        for index in 0..base.len() {
            let mut plus = base.clone();
            plus[index] += epsilon;
            let mut minus = base.clone();
            minus[index] -= epsilon;
            let plus_loss = Tensor::from_f32(plus, &[2, 2])
                .expect("plus logits")
                .cross_entropy(
                    &Tensor::from_f32(vec![0.0, 1.0, 1.0, 0.0], &[2, 2]).expect("plus targets"),
                    1,
                )
                .expect("plus loss")
                .scalar_f32()
                .expect("plus scalar");
            let minus_loss = Tensor::from_f32(minus, &[2, 2])
                .expect("minus logits")
                .cross_entropy(
                    &Tensor::from_f32(vec![0.0, 1.0, 1.0, 0.0], &[2, 2]).expect("minus targets"),
                    1,
                )
                .expect("minus loss")
                .scalar_f32()
                .expect("minus scalar");
            let numerical = (plus_loss - minus_loss) / (2.0 * epsilon);
            assert!(
                (numerical - logits[index]).abs() < 2.0e-4,
                "finite difference {index}: {numerical} != {}",
                logits[index]
            );
        }
    }
}
