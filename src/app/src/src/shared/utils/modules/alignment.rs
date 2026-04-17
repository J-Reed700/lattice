#![allow(unsafe_code)]

use crate::shared::error::{AppError, Result};

/// Validates that a pointer is properly aligned for type T
///
/// This is critical for ARM processors (M1/M2 Macs, ARM Linux, Raspberry Pi)
/// which will crash with SIGBUS on misaligned access.
pub fn validate_alignment<T>(ptr: *const u8) -> Result<()> {
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

/// Converts bytes to typed slice with alignment validation
///
/// Uses bytemuck::Pod trait to ensure safe transmutation.
/// Returns error if alignment or length is invalid.
pub fn bytes_to_slice<T: bytemuck::Pod>(bytes: &[u8]) -> Result<&[T]> {
    // Check length
    if !bytes.len().is_multiple_of(std::mem::size_of::<T>()) {
        return Err(AppError::InvalidInput(format!(
            "Byte array length {} is not a multiple of type size {}",
            bytes.len(),
            std::mem::size_of::<T>()
        )));
    }

    // Check alignment
    validate_alignment::<T>(bytes.as_ptr())?;

    // SAFETY: Safe because:
    // 1. Length validated above
    // 2. Alignment validated above
    // 3. T: Pod means all bit patterns are valid
    // 4. Lifetime of result tied to input bytes
    let slice = unsafe {
        std::slice::from_raw_parts(
            bytes.as_ptr() as *const T,
            bytes.len() / std::mem::size_of::<T>(),
        )
    };

    Ok(slice)
}

/// Converts bytes to f32 slice with alignment validation
///
/// This is a common operation for embedding vectors.
/// Returns error if alignment is invalid, with fallback to copy.
pub fn bytes_to_f32_slice(bytes: &[u8]) -> Result<&[f32]> {
    bytes_to_slice::<f32>(bytes)
}

/// Converts bytes to f32 Vec with proper alignment (always safe)
///
/// This is a fallback for misaligned data. `Vec<f32>` is always
/// properly aligned, so we can safely copy misaligned data into it.
pub fn bytes_to_f32_vec(bytes: &[u8]) -> Result<Vec<f32>> {
    // Check length
    if !bytes.len().is_multiple_of(std::mem::size_of::<f32>()) {
        return Err(AppError::InvalidInput(format!(
            "Byte array length {} is not a multiple of f32 size (4 bytes)",
            bytes.len()
        )));
    }

    let float_count = bytes.len() / std::mem::size_of::<f32>();
    let mut floats = vec![0.0f32; float_count];

    // SAFETY: Vec is always properly aligned for its element type
    // We're copying raw bytes into a properly aligned Vec<f32>
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), floats.as_mut_ptr() as *mut u8, bytes.len());
    }

    Ok(floats)
}

/// Converts bytes to f32 slice with automatic fallback to copy if misaligned
///
/// Returns either a zero-copy slice (fast) or an owned Vec (safe but slower).
pub enum AlignedF32Data<'a> {
    /// Zero-copy reference (data was properly aligned)
    Borrowed(&'a [f32]),
    /// Owned copy (data was misaligned, had to copy)
    Owned(Vec<f32>),
}

impl<'a> AlignedF32Data<'a> {
    pub fn as_slice(&self) -> &[f32] {
        match self {
            AlignedF32Data::Borrowed(slice) => slice,
            AlignedF32Data::Owned(vec) => vec,
        }
    }
}

/// Converts bytes to f32 data with automatic fallback
///
/// Tries zero-copy first, falls back to copy if misaligned.
/// Always succeeds (never panics on alignment).
pub fn bytes_to_f32_safe(bytes: &[u8]) -> Result<AlignedF32Data<'_>> {
    match bytes_to_f32_slice(bytes) {
        Ok(slice) => Ok(AlignedF32Data::Borrowed(slice)),
        Err(_) => {
            // Alignment failed, use copy fallback
            tracing::warn!(
                "Data at {:p} is misaligned for f32. \
                 Using copy-based loading (slower). \
                 This data would crash on ARM processors.",
                bytes.as_ptr()
            );
            Ok(AlignedF32Data::Owned(bytes_to_f32_vec(bytes)?))
        }
    }
}
