use super::TensorError;
use super::shape::{
    DEFAULT_GLOBAL_MEMORY_BUDGET_BYTES, broadcast_shape, checked_shape, contiguous_strides,
    element_count,
};
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

static GLOBAL_ALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Eq, PartialEq)]
struct TensorAllocation {
    bytes: usize,
}

impl Drop for TensorAllocation {
    fn drop(&mut self) {
        GLOBAL_ALLOCATED_BYTES.fetch_sub(self.bytes, Ordering::SeqCst);
    }
}

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
    F32(Arc<Vec<f32>>),
    I64(Arc<Vec<i64>>),
}

impl TensorData {
    fn len(&self) -> usize {
        match self {
            Self::F32(values) => values.len(),
            Self::I64(values) => values.len(),
        }
    }

    fn capacity(&self) -> usize {
        match self {
            Self::F32(values) => values.capacity(),
            Self::I64(values) => values.capacity(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tensor {
    pub(crate) dtype: TensorDtype,
    pub(crate) shape: Vec<usize>,
    pub(crate) strides: Vec<usize>,
    pub(crate) data: TensorData,
    allocation: Arc<TensorAllocation>,
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
        Self::new(
            TensorDtype::F32,
            TensorData::F32(Arc::new(values)),
            shape,
            budget,
        )
    }

    pub fn from_i64(values: Vec<i64>, shape: &[i64]) -> Result<Self, TensorError> {
        Self::from_i64_with_budget(values, shape, super::DEFAULT_MEMORY_BUDGET_BYTES)
    }

    pub fn from_i64_with_budget(
        values: Vec<i64>,
        shape: &[i64],
        budget: usize,
    ) -> Result<Self, TensorError> {
        Self::new(
            TensorDtype::I64,
            TensorData::I64(Arc::new(values)),
            shape,
            budget,
        )
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
        let bytes = data
            .capacity()
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
        let allocation = reserve_global(bytes)?;
        Self::with_allocation(dtype, data, shape, budget, allocation)
    }

    fn with_allocation(
        dtype: TensorDtype,
        data: TensorData,
        shape: Vec<usize>,
        budget: usize,
        allocation: Arc<TensorAllocation>,
    ) -> Result<Self, TensorError> {
        let elements = element_count(&shape)?;
        if data.len() != elements {
            return Err(TensorError::DataLength {
                expected: elements,
                actual: data.len(),
            });
        }
        let bytes = data
            .capacity()
            .checked_mul(dtype.element_size())
            .ok_or(TensorError::ShapeOverflow)?;
        if bytes > budget {
            return Err(TensorError::AllocationBudget { bytes, budget });
        }
        if allocation.bytes != bytes {
            return Err(TensorError::GlobalAllocationBudget {
                bytes,
                used: allocation.bytes,
                budget: DEFAULT_GLOBAL_MEMORY_BUDGET_BYTES,
            });
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
            allocation,
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
        let allocation = reserve_global(bytes)?;
        let data = match dtype {
            TensorDtype::F32 => TensorData::F32(Arc::new(vec![0.0; elements])),
            TensorDtype::I64 => TensorData::I64(Arc::new(vec![0; elements])),
        };
        Ok(Self {
            dtype,
            strides: contiguous_strides(&shape),
            shape,
            data,
            allocation,
        })
    }

    pub fn dtype(&self) -> TensorDtype {
        self.dtype
    }

    /// Convert an I64 tensor to F32 only when every value is exactly representable.
    pub fn to_f32(&self) -> Result<Self, TensorError> {
        if self.dtype != TensorDtype::I64 {
            return Err(TensorError::DtypeMismatch {
                expected: "I64",
                actual: self.dtype.name(),
            });
        }
        let mut values = Vec::with_capacity(self.element_count());
        for index in 0..self.element_count() {
            let value = self.i64_at_flat(index);
            if !is_exact_f32_integer(value) {
                return Err(TensorError::DtypeConversion {
                    from: "I64",
                    to: "F32",
                });
            }
            values.push(value as f32);
        }
        let shape = shape_from_usize(&self.shape)?;
        Self::from_f32(values, &shape)
    }

    /// Convert an F32 tensor to I64 only when every value is finite and integral.
    pub fn to_i64(&self) -> Result<Self, TensorError> {
        if self.dtype != TensorDtype::F32 {
            return Err(TensorError::DtypeMismatch {
                expected: "F32",
                actual: self.dtype.name(),
            });
        }
        let mut values = Vec::with_capacity(self.element_count());
        for index in 0..self.element_count() {
            let value = self.f32_at_flat(index);
            if !value.is_finite()
                || value.fract() != 0.0
                || value < i64::MIN as f32
                || value >= 9_223_372_036_854_775_808.0_f32
            {
                return Err(TensorError::DtypeConversion {
                    from: "F32",
                    to: "I64",
                });
            }
            values.push(value as i64);
        }
        let shape = shape_from_usize(&self.shape)?;
        Self::from_i64(values, &shape)
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

    pub fn scalar_f32(&self) -> Result<f32, TensorError> {
        if self.dtype != TensorDtype::F32 {
            return Err(TensorError::DtypeMismatch {
                expected: "F32",
                actual: self.dtype.name(),
            });
        }
        if !self.shape.is_empty() {
            return Err(TensorError::NotScalar {
                shape: self.shape.clone(),
            });
        }
        Ok(self.f32_at_flat(0))
    }

    pub fn get_f32(&self, indices: &[i64]) -> Result<f32, TensorError> {
        if self.dtype != TensorDtype::F32 {
            return Err(TensorError::DtypeMismatch {
                expected: "F32",
                actual: self.dtype.name(),
            });
        }
        let flat = self.checked_flat_index(indices)?;
        Ok(self.f32_at_flat(flat))
    }

    pub fn get_i64(&self, indices: &[i64]) -> Result<i64, TensorError> {
        if self.dtype != TensorDtype::I64 {
            return Err(TensorError::DtypeMismatch {
                expected: "I64",
                actual: self.dtype.name(),
            });
        }
        let flat = self.checked_flat_index(indices)?;
        Ok(self.i64_at_flat(flat))
    }

    /// Return the logical F32 values in row-major order.
    ///
    /// This is intentionally a copying API. It keeps tensor storage owned by
    /// `Tensor` while allowing bounded numerical oracles and graph engines to
    /// inspect a value without exposing the backing `Arc<Vec<_>>`.
    pub fn values_f32(&self) -> Result<Vec<f32>, TensorError> {
        if self.dtype != TensorDtype::F32 {
            return Err(TensorError::DtypeMismatch {
                expected: "F32",
                actual: self.dtype.name(),
            });
        }
        Ok((0..self.element_count())
            .map(|index| self.f32_at_flat(index))
            .collect())
    }

    /// Materialize a F32 tensor in a broadcast-compatible target shape.
    pub fn broadcast_to(&self, target_shape: &[i64]) -> Result<Self, TensorError> {
        if self.dtype != TensorDtype::F32 {
            return Err(TensorError::DtypeMismatch {
                expected: "F32",
                actual: self.dtype.name(),
            });
        }
        let target_shape = checked_shape(target_shape)?;
        let output_shape = broadcast_shape(&self.shape, &target_shape)?;
        if output_shape != target_shape {
            return Err(TensorError::BroadcastMismatch {
                left: self.shape.clone(),
                right: target_shape,
            });
        }
        let elements = element_count(&target_shape)?;
        let shape = shape_from_usize(&target_shape)?;
        let values = (0..elements)
            .map(|index| self.f32_at_flat(broadcast_index(index, &target_shape, &self.shape)))
            .collect();
        Self::from_f32(values, &shape)
    }

    fn checked_flat_index(&self, indices: &[i64]) -> Result<usize, TensorError> {
        if indices.len() != self.rank() {
            return Err(TensorError::InvalidIndex {
                indices: indices.to_vec(),
                shape: self.shape.clone(),
            });
        }
        let strides = contiguous_strides(&self.shape);
        indices
            .iter()
            .enumerate()
            .try_fold(0usize, |flat, (axis, index)| {
                let coordinate =
                    usize::try_from(*index).map_err(|_| TensorError::InvalidIndex {
                        indices: indices.to_vec(),
                        shape: self.shape.clone(),
                    })?;
                if coordinate >= self.shape[axis] {
                    return Err(TensorError::InvalidIndex {
                        indices: indices.to_vec(),
                        shape: self.shape.clone(),
                    });
                }
                flat.checked_add(coordinate.checked_mul(strides[axis]).ok_or_else(|| {
                    TensorError::InvalidIndex {
                        indices: indices.to_vec(),
                        shape: self.shape.clone(),
                    }
                })?)
                .ok_or_else(|| TensorError::InvalidIndex {
                    indices: indices.to_vec(),
                    shape: self.shape.clone(),
                })
            })
    }

    #[cfg(test)]
    fn allocated_bytes(&self) -> usize {
        self.allocation.bytes
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
        let mut reshaped = if self.is_contiguous() {
            self.clone()
        } else {
            self.to_contiguous()?
        };
        reshaped.shape = next_shape;
        reshaped.strides = contiguous_strides(&reshaped.shape);
        Ok(reshaped)
    }

    fn is_contiguous(&self) -> bool {
        self.strides() == contiguous_strides(&self.shape)
    }

    fn to_contiguous(&self) -> Result<Self, TensorError> {
        let shape = shape_from_usize(&self.shape)?;
        match &self.data {
            TensorData::F32(_) => {
                let values = (0..self.element_count())
                    .map(|index| self.f32_at_flat(index))
                    .collect();
                Self::from_f32(values, &shape)
            }
            TensorData::I64(_) => {
                let values = (0..self.element_count())
                    .map(|index| self.i64_at_flat(index))
                    .collect();
                Self::from_i64(values, &shape)
            }
        }
    }

    fn storage_index(&self, index: usize) -> usize {
        let logical_strides = contiguous_strides(&self.shape);
        self.shape
            .iter()
            .enumerate()
            .fold(0usize, |storage_index, (axis, dimension)| {
                let coordinate = (index / logical_strides[axis]) % dimension;
                storage_index + coordinate * self.strides[axis]
            })
    }

    fn f32_at_flat(&self, index: usize) -> f32 {
        let TensorData::F32(values) = &self.data else {
            unreachable!("F32 tensor must hold F32 data")
        };
        values[self.storage_index(index)]
    }

    fn i64_at_flat(&self, index: usize) -> i64 {
        let TensorData::I64(values) = &self.data else {
            unreachable!("I64 tensor must hold I64 data")
        };
        values[self.storage_index(index)]
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
        let output_allocation = reserve_output(self.dtype, output_elements)?;
        let output_shape_i64 = shape_from_usize(&output_shape)?;
        match (&self.data, &other.data) {
            (TensorData::F32(_), TensorData::F32(_)) => {
                let mut values = (0..output_elements)
                    .map(|index| {
                        let left_index = broadcast_index(index, &output_shape, &self.shape);
                        let right_index = broadcast_index(index, &output_shape, &other.shape);
                        float_operation(
                            operator,
                            self.f32_at_flat(left_index),
                            other.f32_at_flat(right_index),
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                values.shrink_to_fit();
                Self::with_allocation(
                    TensorDtype::F32,
                    TensorData::F32(Arc::new(values)),
                    checked_shape(&output_shape_i64)?,
                    super::DEFAULT_MEMORY_BUDGET_BYTES,
                    output_allocation,
                )
            }
            (TensorData::I64(_), TensorData::I64(_)) => {
                let mut values = (0..output_elements)
                    .map(|index| {
                        let left_index = broadcast_index(index, &output_shape, &self.shape);
                        let right_index = broadcast_index(index, &output_shape, &other.shape);
                        integer_operation(
                            operator,
                            self.i64_at_flat(left_index),
                            other.i64_at_flat(right_index),
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                values.shrink_to_fit();
                Self::with_allocation(
                    TensorDtype::I64,
                    TensorData::I64(Arc::new(values)),
                    checked_shape(&output_shape_i64)?,
                    super::DEFAULT_MEMORY_BUDGET_BYTES,
                    output_allocation,
                )
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
            (TensorData::F32(_), None) => {
                let sum = (0..self.element_count()).try_fold(0.0f32, |sum, index| {
                    let next = sum + self.f32_at_flat(index);
                    next.is_finite()
                        .then_some(next)
                        .ok_or(TensorError::NonFinite)
                })?;
                Self::from_f32(vec![sum], &[])
            }
            (TensorData::I64(_), None) => {
                let sum = (0..self.element_count()).try_fold(0i64, |sum, index| {
                    sum.checked_add(self.i64_at_flat(index))
                        .ok_or(TensorError::IntegerOverflow)
                })?;
                Self::from_i64(vec![sum], &[])
            }
            (TensorData::F32(_), Some(axis)) => self.reduce_f32(axis, false),
            (TensorData::I64(_), Some(axis)) => self.reduce_i64(axis),
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
            (TensorData::F32(_), None) => {
                if self.element_count() == 0 {
                    return Err(TensorError::EmptyMean);
                }
                let sum = (0..self.element_count())
                    .map(|index| self.f32_at_flat(index))
                    .sum::<f32>();
                let mean = sum / self.element_count() as f32;
                if !mean.is_finite() {
                    return Err(TensorError::NonFinite);
                }
                Self::from_f32(vec![mean], &[])
            }
            (TensorData::F32(_), Some(axis)) => self.reduce_f32(axis, true),
            _ => Err(TensorError::DtypeMismatch {
                expected: "F32",
                actual: self.dtype.name(),
            }),
        }
    }

    fn reduce_f32(&self, axis: usize, mean: bool) -> Result<Self, TensorError> {
        let axis_size = self.shape[axis];
        if mean && axis_size == 0 {
            return Err(TensorError::EmptyMean);
        }
        let outer = checked_product(&self.shape[..axis])?;
        let inner = checked_product(&self.shape[axis + 1..])?;
        let output_shape = self
            .shape
            .iter()
            .enumerate()
            .filter_map(|(index, dimension)| (index != axis).then_some(*dimension))
            .collect::<Vec<_>>();
        let output_elements = outer.checked_mul(inner).ok_or(TensorError::ShapeOverflow)?;
        let output_shape = shape_from_usize(&output_shape)?;
        let output_allocation = reserve_output(TensorDtype::F32, output_elements)?;
        let mut output = vec![0.0; output_elements];
        for outer_index in 0..outer {
            for inner_index in 0..inner {
                let mut sum = 0.0f32;
                for axis_index in 0..axis_size {
                    let index = (outer_index * axis_size + axis_index) * inner + inner_index;
                    sum += self.f32_at_flat(index);
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
        Self::with_allocation(
            TensorDtype::F32,
            TensorData::F32(Arc::new(output)),
            checked_shape(&output_shape)?,
            super::DEFAULT_MEMORY_BUDGET_BYTES,
            output_allocation,
        )
    }

    fn reduce_i64(&self, axis: usize) -> Result<Self, TensorError> {
        let axis_size = self.shape[axis];
        let outer = checked_product(&self.shape[..axis])?;
        let inner = checked_product(&self.shape[axis + 1..])?;
        let output_shape = self
            .shape
            .iter()
            .enumerate()
            .filter_map(|(index, dimension)| (index != axis).then_some(*dimension))
            .collect::<Vec<_>>();
        let output_elements = outer.checked_mul(inner).ok_or(TensorError::ShapeOverflow)?;
        let output_shape = shape_from_usize(&output_shape)?;
        let output_allocation = reserve_output(TensorDtype::I64, output_elements)?;
        let mut output = vec![0i64; output_elements];
        for outer_index in 0..outer {
            for inner_index in 0..inner {
                let mut sum = 0i64;
                for axis_index in 0..axis_size {
                    let index = (outer_index * axis_size + axis_index) * inner + inner_index;
                    sum = sum
                        .checked_add(self.i64_at_flat(index))
                        .ok_or(TensorError::IntegerOverflow)?;
                }
                output[outer_index * inner + inner_index] = sum;
            }
        }
        Self::with_allocation(
            TensorDtype::I64,
            TensorData::I64(Arc::new(output)),
            checked_shape(&output_shape)?,
            super::DEFAULT_MEMORY_BUDGET_BYTES,
            output_allocation,
        )
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
        let (TensorData::F32(_), TensorData::F32(_)) = (&self.data, &other.data) else {
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
        let output_allocation = reserve_output(TensorDtype::F32, output_elements)?;
        let mut output = vec![0.0f32; output_elements];
        for row in 0..rows {
            for column in 0..columns {
                let mut sum = 0.0f32;
                for index in 0..inner {
                    sum += self.f32_at_flat(row * inner + index)
                        * other.f32_at_flat(index * columns + column);
                }
                if !sum.is_finite() {
                    return Err(TensorError::NonFinite);
                }
                output[row * columns + column] = sum;
            }
        }
        let shape = [rows as i64, columns as i64];
        Self::with_allocation(
            TensorDtype::F32,
            TensorData::F32(Arc::new(output)),
            checked_shape(&shape)?,
            super::DEFAULT_MEMORY_BUDGET_BYTES,
            output_allocation,
        )
    }

    pub fn transpose(&self) -> Result<Self, TensorError> {
        if self.rank() != 2 {
            return Err(TensorError::TransposeRank { rank: self.rank() });
        }
        Ok(Self {
            dtype: self.dtype,
            shape: vec![self.shape[1], self.shape[0]],
            strides: vec![self.strides[1], self.strides[0]],
            data: self.data.clone(),
            allocation: self.allocation.clone(),
        })
    }

    pub fn relu(&self) -> Result<Self, TensorError> {
        self.unary_f32(|value| value.max(0.0))
    }

    pub fn exp(&self) -> Result<Self, TensorError> {
        self.unary_f32(f32::exp)
    }

    pub fn log(&self) -> Result<Self, TensorError> {
        if self.dtype != TensorDtype::F32 {
            return Err(TensorError::DtypeMismatch {
                expected: "F32",
                actual: self.dtype.name(),
            });
        }
        let TensorData::F32(_) = &self.data else {
            unreachable!("F32 tensor must hold F32 data")
        };
        let output_allocation = reserve_output(TensorDtype::F32, self.element_count())?;
        let mut output = Vec::with_capacity(self.element_count());
        for index in 0..self.element_count() {
            let value = self.f32_at_flat(index);
            if value <= 0.0 {
                return Err(TensorError::DomainError { operation: "log" });
            }
            let result = value.ln();
            if !result.is_finite() {
                return Err(TensorError::NonFinite);
            }
            output.push(result);
        }
        Self::with_allocation(
            TensorDtype::F32,
            TensorData::F32(Arc::new(output)),
            self.shape.clone(),
            super::DEFAULT_MEMORY_BUDGET_BYTES,
            output_allocation,
        )
    }

    pub fn softmax(&self, axis: usize) -> Result<Self, TensorError> {
        self.check_f32_axis(axis, "softmax")?;
        let TensorData::F32(_) = &self.data else {
            unreachable!("F32 tensor must hold F32 data")
        };
        let (outer, axis_size, inner) = reduction_layout(&self.shape, axis)?;
        let output_elements = self.element_count();
        let output_allocation = reserve_output(TensorDtype::F32, output_elements)?;
        let mut output = vec![0.0f32; output_elements];
        for outer_index in 0..outer {
            for inner_index in 0..inner {
                let maximum = (0..axis_size)
                    .map(|axis_index| {
                        self.f32_at_flat(
                            (outer_index * axis_size + axis_index) * inner + inner_index,
                        )
                    })
                    .fold(f32::NEG_INFINITY, f32::max);
                let normalizer = (0..axis_size).try_fold(0.0f64, |sum, axis_index| {
                    let index = (outer_index * axis_size + axis_index) * inner + inner_index;
                    let next =
                        sum + (f64::from(self.f32_at_flat(index)) - f64::from(maximum)).exp();
                    next.is_finite()
                        .then_some(next)
                        .ok_or(TensorError::NonFinite)
                })?;
                if normalizer <= 0.0 || !normalizer.is_finite() {
                    return Err(TensorError::NonFinite);
                }
                for axis_index in 0..axis_size {
                    let index = (outer_index * axis_size + axis_index) * inner + inner_index;
                    let result = ((f64::from(self.f32_at_flat(index)) - f64::from(maximum)).exp()
                        / normalizer) as f32;
                    if !result.is_finite() {
                        return Err(TensorError::NonFinite);
                    }
                    output[index] = result;
                }
            }
        }
        Self::with_allocation(
            TensorDtype::F32,
            TensorData::F32(Arc::new(output)),
            self.shape.clone(),
            super::DEFAULT_MEMORY_BUDGET_BYTES,
            output_allocation,
        )
    }

    pub fn logsumexp(&self, axis: usize) -> Result<Self, TensorError> {
        self.check_f32_axis(axis, "logsumexp")?;
        let TensorData::F32(_) = &self.data else {
            unreachable!("F32 tensor must hold F32 data")
        };
        let (outer, axis_size, inner) = reduction_layout(&self.shape, axis)?;
        let output_shape = self
            .shape
            .iter()
            .enumerate()
            .filter_map(|(index, dimension)| (index != axis).then_some(*dimension))
            .collect::<Vec<_>>();
        let output_elements = outer.checked_mul(inner).ok_or(TensorError::ShapeOverflow)?;
        let output_allocation = reserve_output(TensorDtype::F32, output_elements)?;
        let mut output = vec![0.0f32; output_elements];
        for outer_index in 0..outer {
            for inner_index in 0..inner {
                let maximum = (0..axis_size)
                    .map(|axis_index| {
                        self.f32_at_flat(
                            (outer_index * axis_size + axis_index) * inner + inner_index,
                        )
                    })
                    .fold(f32::NEG_INFINITY, f32::max);
                let sum = (0..axis_size).try_fold(0.0f64, |sum, axis_index| {
                    let index = (outer_index * axis_size + axis_index) * inner + inner_index;
                    let next =
                        sum + (f64::from(self.f32_at_flat(index)) - f64::from(maximum)).exp();
                    next.is_finite()
                        .then_some(next)
                        .ok_or(TensorError::NonFinite)
                })?;
                let result = f64::from(maximum) + sum.ln();
                if !result.is_finite() {
                    return Err(TensorError::NonFinite);
                }
                let result = result as f32;
                if !result.is_finite() {
                    return Err(TensorError::NonFinite);
                }
                output[outer_index * inner + inner_index] = result;
            }
        }
        Self::with_allocation(
            TensorDtype::F32,
            TensorData::F32(Arc::new(output)),
            output_shape,
            super::DEFAULT_MEMORY_BUDGET_BYTES,
            output_allocation,
        )
    }

    pub fn sum_to_shape(&self, target_shape: &[i64]) -> Result<Self, TensorError> {
        if self.dtype != TensorDtype::F32 {
            return Err(TensorError::DtypeMismatch {
                expected: "F32",
                actual: self.dtype.name(),
            });
        }
        let target_shape = checked_shape(target_shape)?;
        let broadcasted = broadcast_shape(&target_shape, &self.shape)?;
        if broadcasted != self.shape {
            return Err(TensorError::BroadcastMismatch {
                left: self.shape.clone(),
                right: target_shape,
            });
        }
        let TensorData::F32(_) = &self.data else {
            unreachable!("F32 tensor must hold F32 data")
        };
        let output_elements = element_count(&target_shape)?;
        let output_allocation = reserve_output(TensorDtype::F32, output_elements)?;
        let mut output = vec![0.0f32; output_elements];
        let input_strides = contiguous_strides(&self.shape);
        let target_strides = contiguous_strides(&target_shape);
        let rank_offset = self.rank() - target_shape.len();
        for index in 0..self.element_count() {
            let value = self.f32_at_flat(index);
            let mut target_index = 0usize;
            for (input_axis, input_stride) in input_strides.iter().enumerate().skip(rank_offset) {
                let coordinate = (index / input_stride) % self.shape[input_axis];
                let target_axis = input_axis - rank_offset;
                let target_coordinate = if target_shape[target_axis] == 1 {
                    0
                } else {
                    coordinate
                };
                target_index = target_index
                    .checked_add(target_coordinate * target_strides[target_axis])
                    .ok_or(TensorError::ShapeOverflow)?;
            }
            let next = output[target_index] + value;
            if !next.is_finite() {
                return Err(TensorError::NonFinite);
            }
            output[target_index] = next;
        }
        Self::with_allocation(
            TensorDtype::F32,
            TensorData::F32(Arc::new(output)),
            target_shape,
            super::DEFAULT_MEMORY_BUDGET_BYTES,
            output_allocation,
        )
    }

    fn unary_f32(&self, function: impl Fn(f32) -> f32) -> Result<Self, TensorError> {
        if self.dtype != TensorDtype::F32 {
            return Err(TensorError::DtypeMismatch {
                expected: "F32",
                actual: self.dtype.name(),
            });
        }
        let TensorData::F32(_) = &self.data else {
            unreachable!("F32 tensor must hold F32 data")
        };
        let output_allocation = reserve_output(TensorDtype::F32, self.element_count())?;
        let mut output = Vec::with_capacity(self.element_count());
        for index in 0..self.element_count() {
            let value = self.f32_at_flat(index);
            let result = function(value);
            if !result.is_finite() {
                return Err(TensorError::NonFinite);
            }
            output.push(result);
        }
        Self::with_allocation(
            TensorDtype::F32,
            TensorData::F32(Arc::new(output)),
            self.shape.clone(),
            super::DEFAULT_MEMORY_BUDGET_BYTES,
            output_allocation,
        )
    }

    fn check_f32_axis(&self, axis: usize, operation: &'static str) -> Result<(), TensorError> {
        if self.dtype != TensorDtype::F32 {
            return Err(TensorError::DtypeMismatch {
                expected: "F32",
                actual: self.dtype.name(),
            });
        }
        validate_axis(Some(axis), self.rank())?;
        if self.shape[axis] == 0 {
            return Err(TensorError::EmptyReduction { operation });
        }
        Ok(())
    }

    pub fn render(&self) -> String {
        self.render_with_limit(4096).0
    }

    pub(crate) fn render_with_limit(&self, max_values: usize) -> (String, usize) {
        let (values, truncated) = match self.dtype {
            TensorDtype::F32 => (
                (0..self.element_count())
                    .take(max_values)
                    .map(|index| format_f32(self.f32_at_flat(index)))
                    .collect::<Vec<_>>(),
                self.element_count().saturating_sub(max_values),
            ),
            TensorDtype::I64 => (
                (0..self.element_count())
                    .take(max_values)
                    .map(|index| self.i64_at_flat(index).to_string())
                    .collect::<Vec<_>>(),
                self.element_count().saturating_sub(max_values),
            ),
        };
        let values = values.join(", ");
        let suffix = match (truncated > 0, values.is_empty()) {
            (true, true) => format!("... ({truncated} more)"),
            (true, false) => format!(", ... ({truncated} more)"),
            (false, _) => String::new(),
        };
        let rendered = format!(
            "Tensor[{}, shape={:?}, values=[{}{}]]",
            self.dtype.name(),
            self.shape,
            values,
            suffix
        );
        (rendered, max_values.min(self.element_count()))
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

fn checked_product(shape: &[usize]) -> Result<usize, TensorError> {
    shape.iter().try_fold(1usize, |product, dimension| {
        product
            .checked_mul(*dimension)
            .ok_or(TensorError::ShapeOverflow)
    })
}

fn reduction_layout(shape: &[usize], axis: usize) -> Result<(usize, usize, usize), TensorError> {
    validate_axis(Some(axis), shape.len())?;
    let outer = checked_product(&shape[..axis])?;
    let inner = checked_product(&shape[axis + 1..])?;
    Ok((outer, shape[axis], inner))
}

fn shape_from_usize(shape: &[usize]) -> Result<Vec<i64>, TensorError> {
    shape
        .iter()
        .map(|dimension| i64::try_from(*dimension).map_err(|_| TensorError::ShapeOverflow))
        .collect()
}

fn reserve_output(
    dtype: TensorDtype,
    elements: usize,
) -> Result<Arc<TensorAllocation>, TensorError> {
    let bytes = elements
        .checked_mul(dtype.element_size())
        .ok_or(TensorError::ShapeOverflow)?;
    if bytes > super::DEFAULT_MEMORY_BUDGET_BYTES {
        return Err(TensorError::AllocationBudget {
            bytes,
            budget: super::DEFAULT_MEMORY_BUDGET_BYTES,
        });
    }
    reserve_global(bytes)
}

fn reserve_global(bytes: usize) -> Result<Arc<TensorAllocation>, TensorError> {
    loop {
        let used = GLOBAL_ALLOCATED_BYTES.load(Ordering::SeqCst);
        let Some(next) = used.checked_add(bytes) else {
            return Err(TensorError::GlobalAllocationBudget {
                bytes,
                used,
                budget: DEFAULT_GLOBAL_MEMORY_BUDGET_BYTES,
            });
        };
        if next > DEFAULT_GLOBAL_MEMORY_BUDGET_BYTES {
            return Err(TensorError::GlobalAllocationBudget {
                bytes,
                used,
                budget: DEFAULT_GLOBAL_MEMORY_BUDGET_BYTES,
            });
        }
        if GLOBAL_ALLOCATED_BYTES
            .compare_exchange(used, next, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            return Ok(Arc::new(TensorAllocation { bytes }));
        }
    }
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

fn is_exact_f32_integer(value: i64) -> bool {
    let magnitude = value.unsigned_abs();
    let significant_bits = 64 - magnitude.leading_zeros();
    let required_zero_bits = significant_bits.saturating_sub(24);
    required_zero_bits == 0 || (magnitude & ((1u64 << required_zero_bits) - 1)) == 0
}

impl fmt::Display for TensorDtype {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::{Tensor, TensorData, TensorDtype};
    use crate::tensor::TensorError;
    use std::sync::Arc;

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
    fn accounts_for_vec_capacity_in_the_tensor_budget() {
        let mut values = Vec::with_capacity(2);
        values.push(1.0f32);
        assert!(matches!(
            Tensor::from_f32_with_budget(values, &[1], 4),
            Err(TensorError::AllocationBudget { bytes, budget: 4 }) if bytes >= 8
        ));
    }

    #[test]
    fn enforces_global_resident_buffer_budget() {
        let first = Tensor::zeros_f32(&[6_291_456]).expect("first global allocation");
        let second = Tensor::zeros_f32(&[6_291_456]).expect("second global allocation");
        assert_eq!(first.allocated_bytes(), 24 * 1024 * 1024);
        assert!(matches!(
            Tensor::zeros_f32(&[6_291_456]),
            Err(TensorError::GlobalAllocationBudget { .. })
        ));
        drop(second);
        drop(first);
        Tensor::zeros_f32(&[1]).expect("released global allocation");
    }

    #[test]
    fn reshape_preserves_order_without_aliasing() {
        let tensor = Tensor::from_i64(vec![1, 2, 3, 4], &[2, 2]).expect("source tensor");
        let reshaped = tensor.reshape(&[4]).expect("same element count");
        assert_eq!(reshaped.shape(), &[4]);
        assert_eq!(reshaped.strides(), &[1]);
        assert_eq!(reshaped.dtype(), TensorDtype::I64);
        assert_eq!(reshaped, tensor.reshape(&[4]).expect("repeat reshape"));
        assert_eq!(tensor.clone(), tensor);
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
    fn overflowing_shapes_report_errors_without_panicking() {
        let zero_f32 = Tensor::from_f32(Vec::new(), &[0, i64::MAX, i64::MAX]);
        assert!(matches!(zero_f32, Err(TensorError::ShapeOverflow)));
        let zero_i64 = Tensor::from_i64(Vec::new(), &[0, i64::MAX, i64::MAX]);
        assert!(matches!(zero_i64, Err(TensorError::ShapeOverflow)));
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
    fn computes_transpose_unary_stable_reductions_and_sum_to_shape() {
        let matrix = Tensor::from_f32(vec![1.0, 2.0, 3.0, 4.0], &[2, 2]).expect("matrix");
        let transposed = matrix.transpose().expect("transpose");
        assert_eq!(transposed.strides(), &[1, 2]);
        assert_eq!(transposed.allocated_bytes(), matrix.allocated_bytes());
        match (&matrix.data, &transposed.data) {
            (TensorData::F32(left), TensorData::F32(right)) => {
                assert!(Arc::ptr_eq(left, right));
            }
            _ => panic!("transpose changed tensor dtype"),
        }
        assert!(Arc::ptr_eq(&matrix.allocation, &transposed.allocation));
        assert_eq!(
            transposed.render(),
            "Tensor[F32, shape=[2, 2], values=[1.0, 3.0, 2.0, 4.0]]"
        );
        assert_eq!(
            Tensor::from_i64(vec![1, 2, 3, 4, 5, 6], &[2, 3])
                .expect("integer matrix")
                .transpose()
                .expect("integer transpose")
                .render(),
            "Tensor[I64, shape=[3, 2], values=[1, 4, 2, 5, 3, 6]]"
        );

        let values = Tensor::from_f32(vec![-1.0, 0.0, 1.0, 2.0], &[2, 2]).expect("values");
        assert_eq!(
            values.relu().expect("relu").render(),
            "Tensor[F32, shape=[2, 2], values=[0.0, 0.0, 1.0, 2.0]]"
        );
        let logarithms = Tensor::from_f32(vec![1.0, std::f32::consts::E], &[2]).expect("logs");
        let log = logarithms.log().expect("log");
        let TensorData::F32(log_values) = &log.data else {
            panic!("expected F32 log output")
        };
        assert!((log_values[0] - 0.0).abs() < 1e-6);
        assert!((log_values[1] - 1.0).abs() < 1e-6);

        let logits =
            Tensor::from_f32(vec![1000.0, 1001.0, 1002.0, 1003.0], &[2, 2]).expect("logits");
        let softmax = logits.softmax(1).expect("stable softmax");
        let TensorData::F32(softmax_values) = &softmax.data else {
            panic!("expected F32 softmax output")
        };
        assert!((softmax_values[0] + softmax_values[1] - 1.0).abs() < 1e-6);
        assert!((softmax_values[2] + softmax_values[3] - 1.0).abs() < 1e-6);
        let logsumexp = logits.logsumexp(1).expect("stable logsumexp");
        let TensorData::F32(logsumexp_values) = &logsumexp.data else {
            panic!("expected F32 logsumexp output")
        };
        assert!((logsumexp_values[0] - 1001.31323).abs() < 1e-3);
        assert!((logsumexp_values[1] - 1003.31323).abs() < 1e-3);

        let broadcasted = Tensor::from_f32(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], &[2, 3])
            .expect("broadcasted gradient");
        assert_eq!(
            broadcasted
                .sum_to_shape(&[1, 3])
                .expect("sum rows")
                .render(),
            "Tensor[F32, shape=[1, 3], values=[5.0, 7.0, 9.0]]"
        );
        assert_eq!(
            broadcasted.sum_to_shape(&[]).expect("sum all").render(),
            "Tensor[F32, shape=[], values=[21.0]]"
        );
    }

    #[test]
    fn indexes_contiguous_and_transposed_tensors_with_checked_bounds() {
        let matrix = Tensor::from_f32(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], &[2, 3]).expect("matrix");
        assert_eq!(matrix.get_f32(&[1, 0]).expect("contiguous index"), 4.0);
        let transposed = matrix.transpose().expect("transpose");
        assert_eq!(transposed.shape(), &[3, 2]);
        assert_eq!(transposed.get_f32(&[0, 1]).expect("view index"), 4.0);
        assert_eq!(transposed.get_f32(&[2, 1]).expect("last view index"), 6.0);

        let integers = Tensor::from_i64(vec![7, 8, 9], &[3]).expect("integers");
        assert_eq!(integers.get_i64(&[2]).expect("integer index"), 9);
        assert!(matches!(
            integers.get_i64(&[-1]),
            Err(TensorError::InvalidIndex { .. })
        ));
        assert!(matches!(
            integers.get_i64(&[3]),
            Err(TensorError::InvalidIndex { .. })
        ));
        assert!(matches!(
            integers.get_i64(&[]),
            Err(TensorError::InvalidIndex { .. })
        ));
        assert!(matches!(
            integers.get_f32(&[0]),
            Err(TensorError::DtypeMismatch {
                expected: "F32",
                actual: "I64"
            })
        ));
    }

    #[test]
    fn converts_dtypes_losslessly_and_preserves_transpose_order() {
        let integers = Tensor::from_i64(vec![1, 2, 3, 4, 5, 6], &[2, 3])
            .expect("integer matrix")
            .transpose()
            .expect("transpose");
        assert_eq!(
            integers
                .to_f32()
                .expect("exact integer conversion")
                .render(),
            "Tensor[F32, shape=[3, 2], values=[1.0, 4.0, 2.0, 5.0, 3.0, 6.0]]"
        );

        let floats = Tensor::from_f32(vec![1.0, -2.0, 16_777_216.0], &[3]).expect("floats");
        assert_eq!(
            floats.to_i64().expect("exact float conversion").render(),
            "Tensor[I64, shape=[3], values=[1, -2, 16777216]]"
        );
        let exact_large = Tensor::from_i64(vec![1_i64 << 40, i64::MIN], &[2])
            .expect("exact powers of two")
            .to_f32()
            .expect("exact large integer conversion");
        assert_eq!(
            exact_large.get_f32(&[0]).expect("first exact value"),
            (1_i64 << 40) as f32
        );
        assert_eq!(
            exact_large.get_f32(&[1]).expect("minimum exact value"),
            i64::MIN as f32
        );
        assert!(matches!(
            Tensor::from_i64(vec![16_777_217], &[1])
                .expect("integer outside exact F32 range")
                .to_f32(),
            Err(TensorError::DtypeConversion {
                from: "I64",
                to: "F32"
            })
        ));
        assert!(matches!(
            Tensor::from_f32(vec![1.5], &[1])
                .expect("fractional float")
                .to_i64(),
            Err(TensorError::DtypeConversion {
                from: "F32",
                to: "I64"
            })
        ));
        assert!(matches!(
            Tensor::from_f32(vec![1.0], &[1])
                .expect("float tensor")
                .to_f32(),
            Err(TensorError::DtypeMismatch {
                expected: "I64",
                actual: "F32"
            })
        ));
    }

    #[test]
    fn rejects_n2_invalid_rank_domain_empty_axis_and_target_shape() {
        let vector = Tensor::from_f32(vec![1.0, 2.0], &[2]).expect("vector");
        assert!(matches!(
            vector.transpose(),
            Err(TensorError::TransposeRank { rank: 1 })
        ));
        let nonpositive = Tensor::from_f32(vec![0.0, -1.0], &[2]).expect("nonpositive");
        assert!(matches!(
            nonpositive.log(),
            Err(TensorError::DomainError { operation: "log" })
        ));
        let empty = Tensor::from_f32(Vec::new(), &[0, 2]).expect("empty");
        assert!(matches!(
            empty.softmax(0),
            Err(TensorError::EmptyReduction {
                operation: "softmax"
            })
        ));
        assert_eq!(
            empty.softmax(1).expect("empty non-reduced axis").shape(),
            &[0, 2]
        );
        assert_eq!(
            empty.logsumexp(1).expect("empty outer dimension").shape(),
            &[0]
        );
        let rank_zero = Tensor::from_f32(vec![2.0], &[]).expect("rank-zero");
        assert!(matches!(
            rank_zero.softmax(0),
            Err(TensorError::InvalidAxis { axis: 0, rank: 0 })
        ));
        assert!(matches!(
            rank_zero.logsumexp(0),
            Err(TensorError::InvalidAxis { axis: 0, rank: 0 })
        ));
        assert_eq!(
            empty
                .sum_to_shape(&[1, 2])
                .expect("zero broadcast source")
                .render(),
            "Tensor[F32, shape=[1, 2], values=[0.0, 0.0]]"
        );
        let matrix = Tensor::from_f32(vec![1.0, 2.0, 3.0, 4.0], &[2, 2]).expect("matrix");
        assert!(matches!(
            matrix.sum_to_shape(&[3]),
            Err(TensorError::BroadcastMismatch { .. })
        ));
        assert!(matches!(
            Tensor::from_i64(vec![1], &[1]).expect("integer").relu(),
            Err(TensorError::DtypeMismatch {
                expected: "F32",
                actual: "I64"
            })
        ));
    }

    #[test]
    fn truncates_large_rendered_tensors_deterministically() {
        let tensor = Tensor::zeros_f32(&[4097]).expect("render test tensor");
        let rendered = tensor.render();
        assert!(rendered.starts_with("Tensor[F32, shape=[4097], values=[0.0, 0.0"));
        assert!(rendered.ends_with(", ... (1 more)]]"));
        assert_eq!(rendered.matches("0.0").count(), 4096);
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
