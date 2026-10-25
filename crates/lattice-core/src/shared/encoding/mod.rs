//! Byte-slice → f32 conversion with alignment validation.
//!
//! Critical for ARM (Apple Silicon, Raspberry Pi, ARM Linux) where
//! misaligned reads SIGBUS instead of slow-pathing like x86 does.
//!
//! Public surface (only what's actually used):
//! - `bytes_to_f32_slice` — zero-copy view, errors on misalignment
//! - `bytes_to_f32_vec` — always-safe copy into a properly-aligned `Vec<f32>`

use crate::shared::error::{AppError, Result};

/// Zero-copy view of bytes as `&[f32]`. Errors if the input is
/// misaligned for f32; callers wanting a fallback should use
/// `bytes_to_f32_vec` (always safe, copies).
pub fn bytes_to_f32_slice(bytes: &[u8]) -> Result<&[f32]> {
    if bytes.is_empty() {
        return Ok(&[]);
    }
    bytemuck::try_cast_slice(bytes)
        .map_err(|error| AppError::InvalidInput(format!("Invalid f32 byte slice: {error}")))
}

/// Copy bytes into a freshly-allocated `Vec<f32>`. Always safe — `Vec`
/// allocations are properly aligned regardless of the source pointer.
pub fn bytes_to_f32_vec(bytes: &[u8]) -> Result<Vec<f32>> {
    if !bytes.len().is_multiple_of(std::mem::size_of::<f32>()) {
        return Err(AppError::InvalidInput(format!(
            "Byte array length {} is not a multiple of f32 size (4 bytes)",
            bytes.len()
        )));
    }

    let float_count = bytes.len() / std::mem::size_of::<f32>();
    let mut floats = vec![0.0f32; float_count];

    bytemuck::cast_slice_mut(&mut floats).copy_from_slice(bytes);

    Ok(floats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aligned_bytes_are_borrowed_without_copying() {
        let values = [1.25_f32, -3.5, 0.0];
        let decoded = bytes_to_f32_slice(bytemuck::cast_slice(&values)).unwrap();
        assert_eq!(decoded, values);
        assert_eq!(decoded.as_ptr(), values.as_ptr());
    }

    #[test]
    fn unaligned_bytes_can_be_copied_but_not_borrowed() {
        let mut buffer = [0_u8; 8];
        let offset = (0..4)
            .find(|offset| !(buffer.as_ptr() as usize + offset).is_multiple_of(4))
            .unwrap();
        let bytes = &mut buffer[offset..offset + 4];
        bytes.copy_from_slice(&1.25_f32.to_ne_bytes());
        assert!(bytes_to_f32_slice(bytes).is_err());
        assert_eq!(bytes_to_f32_vec(bytes).unwrap(), [1.25]);
    }

    #[test]
    fn empty_and_incomplete_inputs_are_handled() {
        assert!(bytes_to_f32_slice(&[]).unwrap().is_empty());
        assert!(bytes_to_f32_vec(&[]).unwrap().is_empty());
        assert!(bytes_to_f32_slice(&[0; 3]).is_err());
        assert!(bytes_to_f32_vec(&[0; 3]).is_err());
    }
}
