use super::TensorError;
use super::shape::{broadcast_shape, checked_shape, contiguous_strides, element_count};
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TensorDtype {
    F32,
    I64,
}

impl TensorDtype {
    pub fn name(self) -> &'static str {
        match self {
            Self::F32 => "F32",
            Self::I64 => "I64",
        }
    }

    pub fn element_size(self) -> usize {
        match self {
            Self::F32 => 4,
            Self::I64 => 8,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TensorData {
    F32(Vec<f32>),
    I64(Vec<i64>),
}

impl TensorData {
    fn len(&self) -> usize {
        match self {
            Self::F32(values) => values.len(),
            Self::I64(values) => values.len(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tensor {
    pub(crate) dtype: TensorDtype,
    pub(crate) shape: Vec<usize>,
    pub(crate) strides: Vec<usize>,
    pub(crate) data: TensorData,
}

impl Tensor {
    pub fn zeros_f32(shape: &[i64]) -> Result<Self, TensorError> {
        Self::zeros_f32_with_budget(shape, super::DEFAULT_MEMORY_BUDGET_BYTES)
    }

    pub fn zeros_f32_with_budget(shape: &[i64], budget: usize) -> Result<Self, TensorError> {
        Self::zeros(TensorDtype::F32, shape, budget)
    }

    pub fn zeros_i64(shape: &[i64]) -> Result<Self, TensorError> {
        Self::zeros_i64_with_budget(shape, super::DEFAULT_MEMORY_BUDGET_BYTES)
    }

    pub fn zeros_i64_with_budget(shape: &[i64], budget: usize) -> Result<Self, TensorError> {
        Self::zeros(TensorDtype::I64, shape, budget)
    }

    pub fn from_f32(values: Vec<f32>, shape: &[i64]) -> Result<Self, TensorError> {
        Self::from_f32_with_budget(values, shape, super::DEFAULT_MEMORY_BUDGET_BYTES)
    }

    pub fn from_f32_with_budget(
        values: Vec<f32>,
        shape: &[i64],
        budget: usize,
    ) -> Result<Self, TensorError> {
        Self::new(TensorDtype::F32, TensorData::F32(values), shape, budget)
    }

    pub fn from_i64(values: Vec<i64>, shape: &[i64]) -> Result<Self, TensorError> {
        Self::from_i64_with_budget(values, shape, super::DEFAULT_MEMORY_BUDGET_BYTES)
    }

    pub fn from_i64_with_budget(
        values: Vec<i64>,
        shape: &[i64],
        budget: usize,
    ) -> Result<Self, TensorError> {
        Self::new(TensorDtype::I64, TensorData::I64(values), shape, budget)
    }

    fn new(
        dtype: TensorDtype,
        data: TensorData,
        shape: &[i64],
        budget: usize,
    ) -> Result<Self, TensorError> {
        let shape = checked_shape(shape)?;
        let elements = element_count(&shape)?;
        if data.len() != elements {
            return Err(TensorError::DataLength {
                expected: elements,
                actual: data.len(),
            });
        }
        let bytes = elements
            .checked_mul(dtype.element_size())
            .ok_or(TensorError::ShapeOverflow)?;
        if bytes > budget {
            return Err(TensorError::AllocationBudget { bytes, budget });
        }
        if let TensorData::F32(values) = &data
            && values.iter().any(|value| !value.is_finite())
        {
            return Err(TensorError::NonFinite);
        }
        Ok(Self {
            dtype,
            strides: contiguous_strides(&shape),
            shape,
            data,
        })
    }

    fn zeros(dtype: TensorDtype, shape: &[i64], budget: usize) -> Result<Self, TensorError> {
        let shape = checked_shape(shape)?;
        let elements = element_count(&shape)?;
        let bytes = elements
            .checked_mul(dtype.element_size())
            .ok_or(TensorError::ShapeOverflow)?;
        if bytes > budget {
            return Err(TensorError::AllocationBudget { bytes, budget });
        }
        let data = match dtype {
            TensorDtype::F32 => TensorData::F32(vec![0.0; elements]),
            TensorDtype::I64 => TensorData::I64(vec![0; elements]),
        };
        Ok(Self {
            dtype,
            strides: contiguous_strides(&shape),
            shape,
            data,
        })
    }

    pub fn dtype(&self) -> TensorDtype {
        self.dtype
    }

    pub fn shape(&self) -> &[usize] {
        &self.shape
    }

    pub fn strides(&self) -> &[usize] {
        &self.strides
    }

    pub fn rank(&self) -> usize {
        self.shape.len()
    }

    pub fn element_count(&self) -> usize {
        self.data.len()
    }

    pub fn reshape(&self, shape: &[i64]) -> Result<Self, TensorError> {
        let next_shape = checked_shape(shape)?;
        let elements = element_count(&next_shape)?;
        if elements != self.element_count() {
            return Err(TensorError::InvalidReshape {
                elements: self.element_count(),
                shape: next_shape,
            });
        }
        let mut reshaped = self.copy();
        reshaped.shape = next_shape;
        reshaped.strides = contiguous_strides(&reshaped.shape);
        Ok(reshaped)
    }

    pub fn copy(&self) -> Self {
        self.clone()
    }

    pub fn elementwise(&self, other: &Self, operator: &str) -> Result<Self, TensorError> {
        if self.dtype != other.dtype {
            return Err(TensorError::DtypeMismatch {
                expected: self.dtype.name(),
                actual: other.dtype.name(),
            });
        }
        let output_shape = broadcast_shape(&self.shape, &other.shape)?;
        let output_elements = element_count(&output_shape)?;
        ensure_allocation(self.dtype, output_elements)?;
        let output_shape_i64 = shape_from_usize(&output_shape)?;
        match (&self.data, &other.data) {
            (TensorData::F32(left), TensorData::F32(right)) => {
                let values = (0..output_elements)
                    .map(|index| {
                        let left_index = broadcast_index(index, &output_shape, &self.shape);
                        let right_index = broadcast_index(index, &output_shape, &other.shape);
                        float_operation(operator, left[left_index], right[right_index])
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Self::from_f32(values, &output_shape_i64)
            }
            (TensorData::I64(left), TensorData::I64(right)) => {
                let values = (0..output_elements)
                    .map(|index| {
                        let left_index = broadcast_index(index, &output_shape, &self.shape);
                        let right_index = broadcast_index(index, &output_shape, &other.shape);
                        integer_operation(operator, left[left_index], right[right_index])
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Self::from_i64(values, &output_shape_i64)
            }
            _ => Err(TensorError::DtypeMismatch {
                expected: self.dtype.name(),
                actual: other.dtype.name(),
            }),
        }
    }

    pub fn sum_all(&self) -> Result<Self, TensorError> {
        self.sum_axis(None)
    }

    pub fn mean_all(&self) -> Result<Self, TensorError> {
        self.mean_axis(None)
    }

    pub fn sum_axis(&self, axis: Option<usize>) -> Result<Self, TensorError> {
        let axis = validate_axis(axis, self.rank())?;
        match (&self.data, axis) {
            (TensorData::F32(values), None) => {
                let sum = values.iter().try_fold(0.0f32, |sum, value| {
                    let next = sum + value;
                    next.is_finite()
                        .then_some(next)
                        .ok_or(TensorError::NonFinite)
                })?;
                Self::from_f32(vec![sum], &[])
            }
            (TensorData::I64(values), None) => {
                let sum = values.iter().try_fold(0i64, |sum, value| {
                    sum.checked_add(*value).ok_or(TensorError::IntegerOverflow)
                })?;
                Self::from_i64(vec![sum], &[])
            }
            (TensorData::F32(values), Some(axis)) => self.reduce_f32(values, axis, false),
            (TensorData::I64(values), Some(axis)) => self.reduce_i64(values, axis),
        }
    }

    pub fn mean_axis(&self, axis: Option<usize>) -> Result<Self, TensorError> {
        let axis = validate_axis(axis, self.rank())?;
        if self.dtype != TensorDtype::F32 {
            return Err(TensorError::DtypeMismatch {
                expected: "F32",
                actual: self.dtype.name(),
            });
        }
        match (&self.data, axis) {
            (TensorData::F32(values), None) => {
                if values.is_empty() {
                    return Err(TensorError::EmptyMean);
                }
                let sum = values.iter().fold(0.0f32, |sum, value| sum + value);
                let mean = sum / values.len() as f32;
                if !mean.is_finite() {
                    return Err(TensorError::NonFinite);
                }
                Self::from_f32(vec![mean], &[])
            }
            (TensorData::F32(values), Some(axis)) => self.reduce_f32(values, axis, true),
            _ => Err(TensorError::DtypeMismatch {
                expected: "F32",
                actual: self.dtype.name(),
            }),
        }
    }

    fn reduce_f32(&self, values: &[f32], axis: usize, mean: bool) -> Result<Self, TensorError> {
        let axis_size = self.shape[axis];
        if mean && axis_size == 0 {
            return Err(TensorError::EmptyMean);
        }
        let outer = self.shape[..axis].iter().product::<usize>();
        let inner = self.shape[axis + 1..].iter().product::<usize>();
        let output_shape = self
            .shape
            .iter()
            .enumerate()
            .filter_map(|(index, dimension)| (index != axis).then_some(*dimension))
            .collect::<Vec<_>>();
        let output_elements = outer.checked_mul(inner).ok_or(TensorError::ShapeOverflow)?;
        ensure_allocation(TensorDtype::F32, output_elements)?;
        let mut output = vec![0.0; output_elements];
        for outer_index in 0..outer {
            for inner_index in 0..inner {
                let mut sum = 0.0f32;
                for axis_index in 0..axis_size {
                    let index = (outer_index * axis_size + axis_index) * inner + inner_index;
                    sum += values[index];
                }
                if mean {
                    sum /= axis_size as f32;
                }
                if !sum.is_finite() {
                    return Err(TensorError::NonFinite);
                }
                output[outer_index * inner + inner_index] = sum;
            }
        }
        shape_from_usize(&output_shape).and_then(|shape| Self::from_f32(output, &shape))
    }

    fn reduce_i64(&self, values: &[i64], axis: usize) -> Result<Self, TensorError> {
        let axis_size = self.shape[axis];
        let outer = self.shape[..axis].iter().product::<usize>();
        let inner = self.shape[axis + 1..].iter().product::<usize>();
        let output_shape = self
            .shape
            .iter()
            .enumerate()
            .filter_map(|(index, dimension)| (index != axis).then_some(*dimension))
            .collect::<Vec<_>>();
        let output_elements = outer.checked_mul(inner).ok_or(TensorError::ShapeOverflow)?;
        ensure_allocation(TensorDtype::I64, output_elements)?;
        let mut output = vec![0i64; output_elements];
        for outer_index in 0..outer {
            for inner_index in 0..inner {
                let mut sum = 0i64;
                for axis_index in 0..axis_size {
                    let index = (outer_index * axis_size + axis_index) * inner + inner_index;
                    sum = sum
                        .checked_add(values[index])
                        .ok_or(TensorError::IntegerOverflow)?;
                }
                output[outer_index * inner + inner_index] = sum;
            }
        }
        shape_from_usize(&output_shape).and_then(|shape| Self::from_i64(output, &shape))
    }

    pub fn matmul(&self, other: &Self) -> Result<Self, TensorError> {
        if self.dtype != TensorDtype::F32 || other.dtype != TensorDtype::F32 {
            return Err(TensorError::DtypeMismatch {
                expected: "F32",
                actual: if self.dtype != TensorDtype::F32 {
                    self.dtype.name()
                } else {
                    other.dtype.name()
                },
            });
        }
        if self.rank() != 2 || other.rank() != 2 {
            return Err(TensorError::MatmulRank {
                left: self.rank(),
                right: other.rank(),
            });
        }
        let (rows, inner) = (self.shape[0], self.shape[1]);
        let (other_inner, columns) = (other.shape[0], other.shape[1]);
        if inner != other_inner {
            return Err(TensorError::MatmulShape {
                left: self.shape.clone(),
                right: other.shape.clone(),
            });
        }
        let (TensorData::F32(left), TensorData::F32(right)) = (&self.data, &other.data) else {
            return Err(TensorError::DtypeMismatch {
                expected: "F32",
                actual: if self.dtype != TensorDtype::F32 {
                    self.dtype.name()
                } else {
                    other.dtype.name()
                },
            });
        };
        let output_elements = rows
            .checked_mul(columns)
            .ok_or(TensorError::ShapeOverflow)?;
        ensure_allocation(TensorDtype::F32, output_elements)?;
        let mut output = vec![0.0f32; output_elements];
        for row in 0..rows {
            for column in 0..columns {
                let mut sum = 0.0f32;
                for index in 0..inner {
                    sum += left[row * inner + index] * right[index * columns + column];
                }
                if !sum.is_finite() {
                    return Err(TensorError::NonFinite);
                }
                output[row * columns + column] = sum;
            }
        }
        let shape = [rows as i64, columns as i64];
        Self::from_f32(output, &shape)
    }

    pub fn render(&self) -> String {
        debug_assert_eq!(self.strides(), contiguous_strides(&self.shape));
        let values = match &self.data {
            TensorData::F32(values) => values
                .iter()
                .map(|value| format_f32(*value))
                .collect::<Vec<_>>(),
            TensorData::I64(values) => values.iter().map(ToString::to_string).collect(),
        };
        format!(
            "Tensor[{}, shape={:?}, values=[{}]]",
            self.dtype.name(),
            self.shape,
            values.join(", ")
        )
    }
}

fn validate_axis(axis: Option<usize>, rank: usize) -> Result<Option<usize>, TensorError> {
    if let Some(axis) = axis
        && axis >= rank
    {
        return Err(TensorError::InvalidAxis { axis, rank });
    }
    Ok(axis)
}

fn shape_from_usize(shape: &[usize]) -> Result<Vec<i64>, TensorError> {
    shape
        .iter()
        .map(|dimension| i64::try_from(*dimension).map_err(|_| TensorError::ShapeOverflow))
        .collect()
}

fn ensure_allocation(dtype: TensorDtype, elements: usize) -> Result<(), TensorError> {
    let bytes = elements
        .checked_mul(dtype.element_size())
        .ok_or(TensorError::ShapeOverflow)?;
    if bytes > super::DEFAULT_MEMORY_BUDGET_BYTES {
        return Err(TensorError::AllocationBudget {
            bytes,
            budget: super::DEFAULT_MEMORY_BUDGET_BYTES,
        });
    }
    Ok(())
}

fn broadcast_index(index: usize, output_shape: &[usize], input_shape: &[usize]) -> usize {
    if input_shape.is_empty() {
        return 0;
    }
    let output_strides = contiguous_strides(output_shape);
    let input_strides = contiguous_strides(input_shape);
    let rank_offset = output_shape.len() - input_shape.len();
    let mut input_index = 0;
    for (input_axis, dimension) in input_shape.iter().enumerate() {
        let output_axis = rank_offset + input_axis;
        let coordinate = (index / output_strides[output_axis]) % output_shape[output_axis];
        let input_coordinate = if *dimension == 1 { 0 } else { coordinate };
        input_index += input_coordinate * input_strides[input_axis];
    }
    input_index
}

fn float_operation(operator: &str, left: f32, right: f32) -> Result<f32, TensorError> {
    let value = match operator {
        "+" => left + right,
        "-" => left - right,
        "*" => left * right,
        "/" => {
            if right == 0.0 {
                return Err(TensorError::DivisionByZero);
            }
            left / right
        }
        _ => {
            return Err(TensorError::UnsupportedOperator {
                operator: operator.to_owned(),
            });
        }
    };
    value
        .is_finite()
        .then_some(value)
        .ok_or(TensorError::NonFinite)
}

fn integer_operation(operator: &str, left: i64, right: i64) -> Result<i64, TensorError> {
    let result = match operator {
        "+" => left.checked_add(right),
        "-" => left.checked_sub(right),
        "*" => left.checked_mul(right),
        "/" => {
            if right == 0 {
                return Err(TensorError::DivisionByZero);
            }
            left.checked_div(right)
        }
        _ => {
            return Err(TensorError::UnsupportedOperator {
                operator: operator.to_owned(),
            });
        }
    };
    result.ok_or(TensorError::IntegerOverflow)
}

fn format_f32(value: f32) -> String {
    let rendered = value.to_string();
    if rendered.contains('.') || rendered.contains('e') || rendered.contains('E') {
        rendered
    } else {
        format!("{rendered}.0")
    }
}

impl fmt::Display for TensorDtype {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::{Tensor, TensorDtype};
    use crate::tensor::TensorError;

    #[test]
    fn stores_rank_zero_empty_and_row_major_shapes() {
        let scalar = Tensor::from_i64(vec![7], &[]).expect("rank-zero scalar");
        assert_eq!(scalar.shape(), &[]);
        assert_eq!(scalar.strides(), &[]);
        assert_eq!(scalar.element_count(), 1);

        let empty = Tensor::from_f32(Vec::new(), &[0, 3]).expect("empty tensor");
        assert_eq!(empty.shape(), &[0, 3]);
        assert_eq!(empty.strides(), &[3, 1]);
        assert_eq!(empty.element_count(), 0);
    }

    #[test]
    fn rejects_negative_overflowing_and_over_budget_shapes() {
        assert!(matches!(
            Tensor::from_i64(Vec::new(), &[-1]),
            Err(TensorError::NegativeDimension { .. })
        ));
        assert!(matches!(
            Tensor::from_i64(Vec::new(), &[i64::MAX, i64::MAX]),
            Err(TensorError::ShapeOverflow)
        ));
        assert!(matches!(
            Tensor::from_i64_with_budget(vec![1], &[1], 0),
            Err(TensorError::AllocationBudget { .. })
        ));
    }

    #[test]
    fn reshape_preserves_order_without_aliasing() {
        let tensor = Tensor::from_i64(vec![1, 2, 3, 4], &[2, 2]).expect("source tensor");
        let reshaped = tensor.reshape(&[4]).expect("same element count");
        assert_eq!(reshaped.shape(), &[4]);
        assert_eq!(reshaped.strides(), &[1]);
        assert_eq!(reshaped.dtype(), TensorDtype::I64);
        assert_eq!(reshaped, tensor.reshape(&[4]).expect("repeat reshape"));
        assert_eq!(tensor.copy(), tensor);
        assert!(matches!(
            tensor.reshape(&[3]),
            Err(TensorError::InvalidReshape { .. })
        ));
    }

    #[test]
    fn broadcasts_elementwise_with_an_independent_reference() {
        let left = Tensor::from_f32(vec![1.0, 2.0], &[2, 1]).expect("left");
        let right = Tensor::from_f32(vec![10.0, 20.0, 30.0], &[1, 3]).expect("right");
        let actual = left.elementwise(&right, "+").expect("broadcast add");
        let expected = vec![11.0, 21.0, 31.0, 12.0, 22.0, 32.0];
        assert_eq!(actual.shape(), &[2, 3]);
        let reference = Tensor::from_f32(expected, &[2, 3]).expect("reference");
        assert_eq!(actual, reference);
        assert!(
            left.elementwise(
                &Tensor::from_f32(vec![1.0, 2.0], &[2, 1]).expect("other"),
                "+"
            )
            .is_ok()
        );
        let incompatible = Tensor::from_f32(vec![1.0; 6], &[3, 2]).expect("incompatible");
        assert!(matches!(
            left.elementwise(&incompatible, "+"),
            Err(TensorError::BroadcastMismatch { .. })
        ));
    }

    #[test]
    fn handles_zero_broadcast_and_empty_reductions() {
        let empty = Tensor::from_f32(Vec::new(), &[0, 3]).expect("empty");
        let ones = Tensor::from_f32(vec![1.0, 2.0, 3.0], &[1, 3]).expect("broadcast source");
        let result = empty.elementwise(&ones, "+").expect("zero broadcast");
        assert_eq!(result.shape(), &[0, 3]);
        assert_eq!(result.element_count(), 0);
        assert_eq!(
            empty.sum_all().expect("empty sum").render(),
            "Tensor[F32, shape=[], values=[0.0]]"
        );
        assert!(matches!(empty.mean_all(), Err(TensorError::EmptyMean)));

        let sparse_metadata = Tensor::from_f32(Vec::new(), &[0, 20_000_000]).expect("zero storage");
        assert!(matches!(
            sparse_metadata.sum_axis(Some(0)),
            Err(TensorError::AllocationBudget { .. })
        ));
    }

    #[test]
    fn computes_reductions_and_f32_matmul() {
        let matrix = Tensor::from_f32(vec![1.0, 2.0, 3.0, 4.0], &[2, 2]).expect("matrix");
        assert_eq!(
            matrix.sum_all().expect("sum").render(),
            "Tensor[F32, shape=[], values=[10.0]]"
        );
        assert_eq!(
            matrix.mean_all().expect("mean").render(),
            "Tensor[F32, shape=[], values=[2.5]]"
        );
        assert_eq!(
            matrix.sum_axis(Some(0)).expect("axis sum").render(),
            "Tensor[F32, shape=[2], values=[4.0, 6.0]]"
        );
        let right = Tensor::from_f32(vec![5.0, 6.0, 7.0, 8.0], &[2, 2]).expect("right");
        assert_eq!(
            matrix.matmul(&right).expect("matmul").render(),
            "Tensor[F32, shape=[2, 2], values=[19.0, 22.0, 43.0, 50.0]]"
        );
    }

    #[test]
    fn reports_numeric_failures_without_non_finite_storage() {
        assert!(matches!(
            Tensor::from_f32(vec![f32::NAN], &[1]),
            Err(TensorError::NonFinite)
        ));
        let left = Tensor::from_i64(vec![i64::MIN], &[1]).expect("minimum");
        let right = Tensor::from_i64(vec![-1], &[1]).expect("minus one");
        assert!(matches!(
            left.elementwise(&right, "/"),
            Err(TensorError::IntegerOverflow)
        ));
        let zero = Tensor::from_f32(vec![0.0], &[1]).expect("zero");
        assert!(matches!(
            zero.elementwise(&zero, "/"),
            Err(TensorError::DivisionByZero)
        ));

        let left = Tensor::from_i64(vec![2, 3], &[2]).expect("integer left");
        let right = Tensor::from_i64(vec![4, 5], &[2]).expect("integer right");
        assert_eq!(
            left.elementwise(&right, "*")
                .expect("integer multiply")
                .render(),
            "Tensor[I64, shape=[2], values=[8, 15]]"
        );
    }

    #[test]
    fn checked_shape_campaign_never_panics() {
        for first in 0..32_i64 {
            for second in 0..32_i64 {
                let shape = [first, second];
                let count = usize::try_from(first * second).expect("small product");
                let values = vec![0i64; count];
                let tensor = Tensor::from_i64(values, &shape).expect("small shape");
                assert_eq!(tensor.element_count(), count);
            }
        }
    }
}
