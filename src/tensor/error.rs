use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TensorError {
    InvalidRank {
        rank: usize,
        maximum: usize,
    },
    NegativeDimension {
        axis: usize,
        value: i64,
    },
    ShapeOverflow,
    AllocationBudget {
        bytes: usize,
        budget: usize,
    },
    GlobalAllocationBudget {
        bytes: usize,
        used: usize,
        budget: usize,
    },
    DataLength {
        expected: usize,
        actual: usize,
    },
    DtypeMismatch {
        expected: &'static str,
        actual: &'static str,
    },
    BroadcastMismatch {
        left: Vec<usize>,
        right: Vec<usize>,
    },
    InvalidAxis {
        axis: usize,
        rank: usize,
    },
    EmptyMean,
    InvalidReshape {
        elements: usize,
        shape: Vec<usize>,
    },
    MatmulRank {
        left: usize,
        right: usize,
    },
    MatmulShape {
        left: Vec<usize>,
        right: Vec<usize>,
    },
    IntegerOverflow,
    DivisionByZero,
    NonFinite,
    UnsupportedOperator {
        operator: String,
    },
}

impl TensorError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidRank { .. }
            | Self::NegativeDimension { .. }
            | Self::ShapeOverflow
            | Self::InvalidReshape { .. } => "SEL-R201",
            Self::AllocationBudget { .. } => "SEL-R202",
            Self::GlobalAllocationBudget { .. } => "SEL-R209",
            Self::DataLength { .. } | Self::DtypeMismatch { .. } => "SEL-R203",
            Self::BroadcastMismatch { .. } => "SEL-R204",
            Self::InvalidAxis { .. } | Self::MatmulRank { .. } | Self::MatmulShape { .. } => {
                "SEL-R205"
            }
            Self::IntegerOverflow | Self::DivisionByZero => "SEL-R206",
            Self::NonFinite | Self::EmptyMean => "SEL-R207",
            Self::UnsupportedOperator { .. } => "SEL-R208",
        }
    }
}

impl fmt::Display for TensorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRank { rank, maximum } => {
                write!(formatter, "tensor rank {rank} exceeds maximum {maximum}")
            }
            Self::NegativeDimension { axis, value } => {
                write!(formatter, "tensor dimension {axis} is negative: {value}")
            }
            Self::ShapeOverflow => write!(formatter, "tensor shape product overflowed"),
            Self::AllocationBudget { bytes, budget } => {
                write!(
                    formatter,
                    "tensor allocation of {bytes} bytes exceeds budget {budget}"
                )
            }
            Self::GlobalAllocationBudget {
                bytes,
                used,
                budget,
            } => write!(
                formatter,
                "global tensor allocation of {bytes} bytes exceeds budget {budget} (already used {used} bytes)"
            ),
            Self::DataLength { expected, actual } => write!(
                formatter,
                "tensor data has {actual} elements but shape requires {expected}"
            ),
            Self::DtypeMismatch { expected, actual } => {
                write!(
                    formatter,
                    "tensor dtype mismatch: expected {expected}, got {actual}"
                )
            }
            Self::BroadcastMismatch { left, right } => write!(
                formatter,
                "tensor shapes cannot broadcast: {:?} and {:?}",
                left, right
            ),
            Self::InvalidAxis { axis, rank } => {
                write!(formatter, "tensor axis {axis} is invalid for rank {rank}")
            }
            Self::EmptyMean => write!(formatter, "mean of an empty tensor is undefined"),
            Self::InvalidReshape { elements, shape } => write!(
                formatter,
                "cannot reshape {elements} elements to shape {:?}",
                shape
            ),
            Self::MatmulRank { left, right } => write!(
                formatter,
                "matmul requires rank-2 tensors, got ranks {left} and {right}"
            ),
            Self::MatmulShape { left, right } => {
                write!(
                    formatter,
                    "matmul shapes are incompatible: {:?} and {:?}",
                    left, right
                )
            }
            Self::IntegerOverflow => write!(formatter, "tensor integer arithmetic overflowed"),
            Self::DivisionByZero => write!(formatter, "tensor division by zero"),
            Self::NonFinite => write!(formatter, "tensor operation produced a non-finite F32"),
            Self::UnsupportedOperator { operator } => {
                write!(formatter, "tensor operator `{operator}` is unavailable")
            }
        }
    }
}

impl std::error::Error for TensorError {}
