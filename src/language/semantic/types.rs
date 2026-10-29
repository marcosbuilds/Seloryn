use crate::builtins::Builtin;
use crate::compute::tensor::TensorDtype;
use crate::diagnostic::Span;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Type {
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
pub(crate) struct RecordDef {
    pub(crate) fields: Vec<(String, Type, Span)>,
}

#[derive(Clone, Debug)]
pub(crate) struct VariantDef {
    pub(crate) name: String,
    pub(crate) fields: Vec<(Type, Span)>,
}

#[derive(Clone, Debug)]
pub(crate) struct EnumDef {
    pub(crate) variants: Vec<VariantDef>,
}

#[derive(Clone, Debug)]
pub(crate) enum Symbol {
    Value { ty: Type, mutable: bool },
    Function(FunctionSignature),
}

#[derive(Clone, Debug)]
pub(crate) struct FunctionSignature {
    pub(crate) parameters: Vec<Type>,
    pub(crate) return_type: Type,
}

#[derive(Default)]
pub(crate) struct Scope {
    pub(crate) values: HashMap<String, Symbol>,
}

pub(crate) struct FunctionContext {
    pub(crate) return_type: Type,
}

#[derive(Clone)]
pub(crate) enum Coverage {
    Wildcard,
    Keys(Vec<String>),
    Owned,
}

pub(crate) fn assignable(actual: &Type, expected: &Type) -> bool {
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

pub(crate) fn named_error_type(name: &str) -> Option<Type> {
    match name {
        "JsonError" => Some(Type::JsonError),
        "JsonIoError" => Some(Type::JsonIoError),
        "ModelError" => Some(Type::ModelError),
        "CheckpointError" => Some(Type::CheckpointError),
        _ => None,
    }
}

pub(crate) fn display_type(ty: &Type) -> String {
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

pub(crate) fn builtin_name(builtin: Builtin) -> &'static str {
    match builtin {
        Builtin::TensorF32 => "tensor_f32",
        Builtin::TensorI64 => "tensor_i64",
        Builtin::TensorEmbedding => "tensor_embedding",
        Builtin::UniformF32 => "tensor_uniform_f32",
        Builtin::ZerosF32 => "tensor_zeros_f32",
        Builtin::ZerosI64 => "tensor_zeros_i64",
        Builtin::TensorShape => "tensor_shape",
        Builtin::TensorRank => "tensor_rank",
        Builtin::TensorElements => "tensor_elements",
        Builtin::TensorDtype => "tensor_dtype",
        Builtin::TensorToF32 => "tensor_to_f32",
        Builtin::TensorToI64 => "tensor_to_i64",
        Builtin::TensorItemF32 => "tensor_item_f32",
        Builtin::TensorGetF32 => "tensor_get_f32",
        Builtin::TensorGetI64 => "tensor_get_i64",
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
        Builtin::CrossEntropy => "tensor_cross_entropy",
        Builtin::CrossEntropyIds => "tensor_cross_entropy_ids",
        Builtin::SumToShape => "tensor_sum_to_shape",
    }
}
