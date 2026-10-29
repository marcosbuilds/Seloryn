use super::TensorError;
use super::shape::{
    DEFAULT_GLOBAL_MEMORY_BUDGET_BYTES, broadcast_shape, checked_shape, contiguous_strides,
    element_count,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(super) static GLOBAL_ALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Eq, PartialEq)]
pub(super) struct TensorAllocation {
    pub(super) bytes: usize,
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
    pub(super) contiguous: bool,
    pub(crate) data: TensorData,
    pub(super) allocation: Arc<TensorAllocation>,
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

    /// Gather rows from a vocabulary table using a rank-2 I64 index tensor.
    /// The output is `[batch, context, embedding_dim]` and never aliases the
    /// table. Invalid IDs are rejected before any output allocation is kept.
    pub fn embedding(indices: &Self, table: &Self) -> Result<Self, TensorError> {
        if indices.dtype != TensorDtype::I64 {
            return Err(TensorError::DtypeMismatch {
                expected: "I64",
                actual: indices.dtype.name(),
            });
        }
        if table.dtype != TensorDtype::F32 {
            return Err(TensorError::DtypeMismatch {
                expected: "F32",
                actual: table.dtype.name(),
            });
        }
        if indices.rank() != 2 || table.rank() != 2 {
            return Err(TensorError::EmbeddingRank {
                indices: indices.rank(),
                table: table.rank(),
            });
        }
        let vocabulary_size = table.shape[0];
        let embedding_dim = table.shape[1];
        if vocabulary_size == 0 || embedding_dim == 0 {
            return Err(TensorError::EmbeddingShape {
                indices: indices.shape.clone(),
                table: table.shape.clone(),
            });
        }
        for flat in 0..indices.element_count() {
            let index = indices.i64_at_flat(flat);
            let coordinate = usize::try_from(index).map_err(|_| TensorError::EmbeddingIndex {
                index,
                vocabulary_size,
            })?;
            if coordinate >= vocabulary_size {
                return Err(TensorError::EmbeddingIndex {
                    index,
                    vocabulary_size,
                });
            }
        }
        let output_elements = indices
            .element_count()
            .checked_mul(embedding_dim)
            .ok_or(TensorError::ShapeOverflow)?;
        let output_allocation = super::ops::reserve_output(TensorDtype::F32, output_elements)?;
        let mut output = Vec::with_capacity(output_elements);
        for flat in 0..indices.element_count() {
            let index = usize::try_from(indices.i64_at_flat(flat)).map_err(|_| {
                TensorError::EmbeddingIndex {
                    index: indices.i64_at_flat(flat),
                    vocabulary_size,
                }
            })?;
            let table_offset = index
                .checked_mul(embedding_dim)
                .ok_or(TensorError::ShapeOverflow)?;
            for dimension in 0..embedding_dim {
                output.push(table.f32_at_flat(table_offset + dimension));
            }
        }
        let mut output_shape = super::ops::shape_from_usize(&indices.shape)?;
        output_shape.push(i64::try_from(embedding_dim).map_err(|_| TensorError::ShapeOverflow)?);
        let shape = checked_shape(&output_shape)?;
        Self::with_allocation(
            TensorDtype::F32,
            TensorData::F32(Arc::new(output)),
            shape,
            super::DEFAULT_MEMORY_BUDGET_BYTES,
            output_allocation,
        )
    }

    /// Scatter-add an embedding output gradient back into a vocabulary table.
    /// Repeated IDs intentionally accumulate rather than overwrite.
    pub fn embedding_backward(
        indices: &Self,
        gradient: &Self,
        table_shape: &[i64],
    ) -> Result<Self, TensorError> {
        if indices.dtype != TensorDtype::I64 {
            return Err(TensorError::DtypeMismatch {
                expected: "I64",
                actual: indices.dtype.name(),
            });
        }
        if gradient.dtype != TensorDtype::F32 {
            return Err(TensorError::DtypeMismatch {
                expected: "F32",
                actual: gradient.dtype.name(),
            });
        }
        if indices.rank() != 2 || gradient.rank() != 3 {
            return Err(TensorError::EmbeddingRank {
                indices: indices.rank(),
                table: gradient.rank().saturating_sub(1),
            });
        }
        let checked_table_shape = checked_shape(table_shape)?;
        if checked_table_shape.len() != 2
            || gradient.shape[..2] != indices.shape[..2]
            || gradient.shape[2] != checked_table_shape[1]
        {
            return Err(TensorError::EmbeddingShape {
                indices: indices.shape.clone(),
                table: checked_table_shape,
            });
        }
        let vocabulary_size = checked_table_shape[0];
        let embedding_dim = checked_table_shape[1];
        if vocabulary_size == 0 || embedding_dim == 0 {
            return Err(TensorError::EmbeddingShape {
                indices: indices.shape.clone(),
                table: checked_table_shape,
            });
        }
        let output_elements = vocabulary_size
            .checked_mul(embedding_dim)
            .ok_or(TensorError::ShapeOverflow)?;
        let mut output = vec![0.0f32; output_elements];
        for flat in 0..indices.element_count() {
            let index = indices.i64_at_flat(flat);
            let row = usize::try_from(index).map_err(|_| TensorError::EmbeddingIndex {
                index,
                vocabulary_size,
            })?;
            if row >= vocabulary_size {
                return Err(TensorError::EmbeddingIndex {
                    index,
                    vocabulary_size,
                });
            }
            let gradient_offset = flat
                .checked_mul(embedding_dim)
                .ok_or(TensorError::ShapeOverflow)?;
            let output_offset = row
                .checked_mul(embedding_dim)
                .ok_or(TensorError::ShapeOverflow)?;
            for dimension in 0..embedding_dim {
                let next = output[output_offset + dimension]
                    + gradient.f32_at_flat(gradient_offset + dimension);
                if !next.is_finite() {
                    return Err(TensorError::NonFinite);
                }
                output[output_offset + dimension] = next;
            }
        }
        Self::from_f32(output, table_shape)
    }

    /// Mean softmax cross-entropy for logits `[batch, classes]` and integer
    /// class IDs `[batch]`. The returned cache contains row probabilities.
    pub fn cross_entropy_ids(&self, targets: &Self) -> Result<Self, TensorError> {
        let (loss, _) = self.cross_entropy_ids_with_cache(targets)?;
        Ok(loss)
    }

    pub(crate) fn cross_entropy_ids_with_cache(
        &self,
        targets: &Self,
    ) -> Result<(Self, Self), TensorError> {
        if self.dtype != TensorDtype::F32 {
            return Err(TensorError::DtypeMismatch {
                expected: "F32",
                actual: self.dtype.name(),
            });
        }
        if targets.dtype != TensorDtype::I64 {
            return Err(TensorError::DtypeMismatch {
                expected: "I64",
                actual: targets.dtype.name(),
            });
        }
        if self.rank() != 2 || targets.rank() != 1 || targets.shape[0] != self.shape[0] {
            return Err(TensorError::EmbeddingShape {
                indices: targets.shape.clone(),
                table: self.shape.clone(),
            });
        }
        let rows = self.shape[0];
        let classes = self.shape[1];
        if rows == 0 || classes == 0 {
            return Err(TensorError::EmptyReduction {
                operation: "cross_entropy_ids",
            });
        }
        let mut probabilities = vec![0.0f32; self.element_count()];
        let mut total_loss = 0.0f64;
        for row in 0..rows {
            let offset = row * classes;
            let maximum = (0..classes)
                .map(|class| self.f32_at_flat(offset + class))
                .fold(f32::NEG_INFINITY, f32::max);
            let normalizer = (0..classes).try_fold(0.0f64, |sum, class| {
                let next =
                    sum + (f64::from(self.f32_at_flat(offset + class)) - f64::from(maximum)).exp();
                next.is_finite()
                    .then_some(next)
                    .ok_or(TensorError::NonFinite)
            })?;
            if !normalizer.is_finite() || normalizer <= 0.0 {
                return Err(TensorError::NonFinite);
            }
            let target = targets.i64_at_flat(row);
            let target_class =
                usize::try_from(target).map_err(|_| TensorError::EmbeddingIndex {
                    index: target,
                    vocabulary_size: classes,
                })?;
            if target_class >= classes {
                return Err(TensorError::EmbeddingIndex {
                    index: target,
                    vocabulary_size: classes,
                });
            }
            let log_normalizer = f64::from(maximum) + normalizer.ln();
            let target_log_probability =
                f64::from(self.f32_at_flat(offset + target_class)) - log_normalizer;
            total_loss -= target_log_probability;
            for class in 0..classes {
                let probability =
                    (f64::from(self.f32_at_flat(offset + class)) - log_normalizer).exp();
                if !probability.is_finite() {
                    return Err(TensorError::NonFinite);
                }
                probabilities[offset + class] = probability as f32;
            }
        }
        let loss = total_loss / rows as f64;
        if !loss.is_finite() {
            return Err(TensorError::NonFinite);
        }
        Ok((
            Self::from_f32(vec![loss as f32], &[])?,
            Self::from_f32(probabilities, &[rows as i64, classes as i64])?,
        ))
    }

    /// Deterministic bounded F32 initializer used by the `.sely` model.
    pub fn uniform_f32(shape: &[i64], seed: i64) -> Result<Self, TensorError> {
        let shape = checked_shape(shape)?;
        let elements = element_count(&shape)?;
        let allocation = super::ops::reserve_output(TensorDtype::F32, elements)?;
        let mut state = seed as u64;
        if state == 0 {
            state = 0x9E37_79B9_7F4A_7C15;
        }
        let mut values = Vec::with_capacity(elements);
        for _ in 0..elements {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            let random = state.wrapping_mul(0x2545_F491_4F6C_DD1D);
            let unit = (random as f64) / (u64::MAX as f64);
            values.push(((unit * 2.0 - 1.0) * 0.1) as f32);
        }
        Self::with_allocation(
            TensorDtype::F32,
            TensorData::F32(Arc::new(values)),
            shape,
            super::DEFAULT_MEMORY_BUDGET_BYTES,
            allocation,
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
        let allocation = super::ops::reserve_global(bytes)?;
        Self::with_allocation(dtype, data, shape, budget, allocation)
    }

    pub(super) fn with_allocation(
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
            contiguous: true,
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
        let allocation = super::ops::reserve_global(bytes)?;
        let data = match dtype {
            TensorDtype::F32 => TensorData::F32(Arc::new(vec![0.0; elements])),
            TensorDtype::I64 => TensorData::I64(Arc::new(vec![0; elements])),
        };
        Ok(Self {
            dtype,
            strides: contiguous_strides(&shape),
            shape,
            contiguous: true,
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
            if !super::ops::is_exact_f32_integer(value) {
                return Err(TensorError::DtypeConversion {
                    from: "I64",
                    to: "F32",
                });
            }
            values.push(value as f32);
        }
        let shape = super::ops::shape_from_usize(&self.shape)?;
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
        let shape = super::ops::shape_from_usize(&self.shape)?;
        Self::from_i64(values, &shape)
    }

    pub fn shape(&self) -> &[usize] {
        &self.shape
    }

    #[allow(dead_code)]
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
        let shape = super::ops::shape_from_usize(&target_shape)?;
        let values = (0..elements)
            .map(|index| {
                self.f32_at_flat(super::ops::broadcast_index(
                    index,
                    &target_shape,
                    &self.shape,
                ))
            })
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
    pub(super) fn allocated_bytes(&self) -> usize {
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
        self.contiguous
    }

    fn to_contiguous(&self) -> Result<Self, TensorError> {
        let shape = super::ops::shape_from_usize(&self.shape)?;
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
        if self.contiguous {
            return index;
        }
        let logical_strides = contiguous_strides(&self.shape);
        self.shape
            .iter()
            .enumerate()
            .fold(0usize, |storage_index, (axis, dimension)| {
                let coordinate = (index / logical_strides[axis]) % dimension;
                storage_index + coordinate * self.strides[axis]
            })
    }

    pub(super) fn f32_at_flat(&self, index: usize) -> f32 {
        let TensorData::F32(values) = &self.data else {
            unreachable!("F32 tensor must hold F32 data")
        };
        values[self.storage_index(index)]
    }

    pub(super) fn i64_at_flat(&self, index: usize) -> i64 {
        let TensorData::I64(values) = &self.data else {
            unreachable!("I64 tensor must hold I64 data")
        };
        values[self.storage_index(index)]
    }
}
