#![allow(unsafe_code)]

//! Byte-slice → f32 conversion with alignment validation.
//!
//! Critical for ARM (Apple Silicon, Raspberry Pi, ARM Linux) where
//! misaligned reads SIGBUS instead of slow-pathing like x86 does.
//!
//! Public surface (only what's actually used):
//! - `bytes_to_f32_slice` — zero-copy view, errors on misalignment
//! - `bytes_to_f32_vec` — always-safe copy into a properly-aligned `Vec<f32>`

use crate::shared::error::{AppError, Result};

/// Validates that a pointer is properly aligned for type T.
pub(crate) fn validate_alignment<T>(ptr: *const u8) -> Result<()> {
    let alignment = ptr as usize % std::mem::align_of::<T>();
    if alignment != 0 {
        return Err(AppError::Other(format!(
            "Pointer {:p} is not aligned for type {}. \
             Alignment: {}, Required: {}. \
             This would crash on ARM processors (M1/M2 Macs, ARM Linux).",
            ptr,
            std::any::type_name::<T>(),
            alignment,
            std::mem::align_of::<T>()
        )));
    }
    Ok(())
}

/// Converts bytes to a typed slice with alignment + length validation.
fn bytes_to_slice<T: bytemuck::Pod>(bytes: &[u8]) -> Result<&[T]> {
    if !bytes.len().is_multiple_of(std::mem::size_of::<T>()) {
        return Err(AppError::InvalidInput(format!(
            "Byte array length {} is not a multiple of type size {}",
            bytes.len(),
            std::mem::size_of::<T>()
        )));
    }
    validate_alignment::<T>(bytes.as_ptr())?;

    // SAFETY: length validated above; alignment validated above;
    // T: Pod means all bit patterns are valid; lifetime tied to input.
    let slice = unsafe {
        std::slice::from_raw_parts(
            bytes.as_ptr() as *const T,
            bytes.len() / std::mem::size_of::<T>(),
        )
    };
    Ok(slice)
}

/// Zero-copy view of bytes as `&[f32]`. Errors if the input is
/// misaligned for f32; callers wanting a fallback should use
/// `bytes_to_f32_vec` (always safe, copies).
pub fn bytes_to_f32_slice(bytes: &[u8]) -> Result<&[f32]> {
    bytes_to_slice::<f32>(bytes)
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

    // SAFETY: Vec is always properly aligned for its element type;
    // we're copying raw bytes into a properly aligned Vec<f32>.
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), floats.as_mut_ptr() as *mut u8, bytes.len());
    }

    Ok(floats)
}
