use super::TensorError;

pub const MAX_RANK: usize = 32;
pub const DEFAULT_MEMORY_BUDGET_BYTES: usize = 64 * 1024 * 1024;
pub const DEFAULT_GLOBAL_MEMORY_BUDGET_BYTES: usize = 64 * 1024 * 1024;

pub fn checked_shape(shape: &[i64]) -> Result<Vec<usize>, TensorError> {
    if shape.len() > MAX_RANK {
        return Err(TensorError::InvalidRank {
            rank: shape.len(),
            maximum: MAX_RANK,
        });
    }
    let checked = shape
        .iter()
        .enumerate()
        .map(|(axis, dimension)| {
            usize::try_from(*dimension).map_err(|_| TensorError::NegativeDimension {
                axis,
                value: *dimension,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    checked.iter().rev().try_fold(1usize, |stride, dimension| {
        stride
            .checked_mul(*dimension)
            .ok_or(TensorError::ShapeOverflow)
    })?;
    Ok(checked)
}

pub fn element_count(shape: &[usize]) -> Result<usize, TensorError> {
    shape.iter().try_fold(1usize, |count, dimension| {
        count
            .checked_mul(*dimension)
            .ok_or(TensorError::ShapeOverflow)
    })
}

pub fn contiguous_strides(shape: &[usize]) -> Vec<usize> {
    let mut strides = vec![1; shape.len()];
    let mut stride = 1usize;
    for (index, dimension) in shape.iter().enumerate().rev() {
        strides[index] = stride;
        stride = stride
            .checked_mul(*dimension)
            .expect("validated tensor shape has overflowing strides");
    }
    strides
}

pub fn broadcast_shape(left: &[usize], right: &[usize]) -> Result<Vec<usize>, TensorError> {
    let rank = left.len().max(right.len());
    let mut result = vec![0; rank];
    for offset in 0..rank {
        let left_dimension = left
            .len()
            .checked_sub(offset + 1)
            .and_then(|index| left.get(index))
            .copied()
            .unwrap_or(1);
        let right_dimension = right
            .len()
            .checked_sub(offset + 1)
            .and_then(|index| right.get(index))
            .copied()
            .unwrap_or(1);
        let dimension = if left_dimension == right_dimension {
            left_dimension
        } else if left_dimension == 1 {
            right_dimension
        } else if right_dimension == 1 {
            left_dimension
        } else {
            return Err(TensorError::BroadcastMismatch {
                left: left.to_vec(),
                right: right.to_vec(),
            });
        };
        result[rank - offset - 1] = dimension;
    }
    Ok(result)
}
