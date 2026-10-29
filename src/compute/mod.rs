//! Compute domains used by the runtime: tensors, autodiff and GPU probes.

pub(crate) mod autograd;
pub(crate) mod gpu;
pub(crate) mod tensor;
pub(crate) mod tensor_autograd;
