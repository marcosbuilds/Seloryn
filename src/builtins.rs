#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Builtin {
    TensorF32,
    TensorI64,
    ZerosF32,
    ZerosI64,
    TensorShape,
    TensorRank,
    TensorElements,
    TensorDtype,
    Reshape,
    Sum,
    Mean,
    Matmul,
}

pub fn lookup(name: &str) -> Option<Builtin> {
    match name {
        "tensor_f32" => Some(Builtin::TensorF32),
        "tensor_i64" => Some(Builtin::TensorI64),
        "tensor_zeros_f32" => Some(Builtin::ZerosF32),
        "tensor_zeros_i64" => Some(Builtin::ZerosI64),
        "tensor_shape" => Some(Builtin::TensorShape),
        "tensor_rank" => Some(Builtin::TensorRank),
        "tensor_elements" => Some(Builtin::TensorElements),
        "tensor_dtype" => Some(Builtin::TensorDtype),
        "reshape" => Some(Builtin::Reshape),
        "tensor_sum" => Some(Builtin::Sum),
        "tensor_mean" => Some(Builtin::Mean),
        "matmul" => Some(Builtin::Matmul),
        _ => None,
    }
}
