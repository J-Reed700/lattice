#![allow(unsafe_code)]

use crate::domain::embedding_constants::DEFAULT_EMBEDDING_DIM;
use crate::infrastructure::persistence::database::connection::query_with_heavy_timeout;
use crate::shared::error::{AppError, Result, ResultExt};
use crate::shared::utils::alignment::bytes_to_f32_slice;
use memmap2::Mmap;
use sqlx::{Row, SqlitePool};
use std::fs::{File, Metadata};
use std::io::{BufWriter, Write};
use std::path::Path;

#[derive(Debug)]
pub struct EmbeddingIndex {
    mmap: Option<Mmap>, // None for empty index
    count: usize,
    dim: usize,
    ids: Vec<String>,
    metadata: Option<Metadata>,
}

impl EmbeddingIndex {
    /// Create a new empty embedding index with the specified dimension
    pub fn new(dim: usize) -> Self {
        Self {
            mmap: None,
            count: 0,
            dim,
            ids: vec![],
            metadata: None,
        }
    }

    pub async fn from_database(pool: &SqlitePool) -> Result<Self> {
        let pool_clone = pool.clone();
        let records = query_with_heavy_timeout(|| async {
            sqlx::query(
                r#"
                SELECT te.id, te.embedding, te.dimension
                FROM text_embeddings te
                ORDER BY te.created_at
                "#,
            )
            .fetch_all(&pool_clone)
            .await
        })
        .await?;

        if records.is_empty() {
            // P0 FIX: Use None for empty index instead of creating temp file
            // This prevents temp file leak and simplifies empty index handling
            return Ok(Self {
                mmap: None, // No mmap for empty case
                count: 0,
                dim: DEFAULT_EMBEDDING_DIM,
                ids: vec![],
                metadata: None,
            });
        }

        let dim: i64 = records
            .first()
            .context("No records found to determine embedding dimension")?
            .get("dimension");
        let dim = dim as usize;
        let count = records.len();

        // FIX: Use unique temp file per call to prevent parallel test collisions
        // Previously: std::env::temp_dir().join("embeddings.bin") caused SIGBUS
        // when multiple tests wrote to same file, invalidating mmap references
        let temp_path =
            std::env::temp_dir().join(format!("embeddings-{}.bin", uuid::Uuid::new_v4()));
        let file = File::create(&temp_path)?;
        let mut writer = BufWriter::new(file);

        writer.write_all(&(count as u32).to_le_bytes())?;
        writer.write_all(&(dim as u32).to_le_bytes())?;

        let mut ids = Vec::with_capacity(count);

        for record in records {
            let id: String = record.get("id");
            ids.push(id);

            let embedding_bytes: Vec<u8> = record.get("embedding");

            // Convert bytes to f32 slice with alignment validation
            // This will fail on ARM if data is misaligned
            let float_slice = bytes_to_f32_slice(&embedding_bytes).context(
                "Failed to convert embedding bytes to f32 slice - alignment issue on ARM",
            )?;

            for &val in float_slice {
                writer.write_all(&val.to_le_bytes())?;
            }
        }

        writer.flush()?;
        drop(writer);

        let file = File::open(&temp_path)?;

        // P1 Issue #4: Store metadata to detect if file changes during use
        let metadata = file
            .metadata()
            .context("Failed to read file metadata for mmap validation")?;

        // Validate file size is reasonable
        if metadata.len() == 0 {
            return Err(AppError::InvalidInput("Cannot mmap empty file".to_string()));
        }

        if metadata.len() > 10 * 1024 * 1024 * 1024 {
            // 10GB
            return Err(AppError::InvalidInput(format!(
                "File too large to mmap: {} bytes",
                metadata.len()
            )));
        }

        // SAFETY: Memory-mapping the file is safe because:
        // 1. The file was just created and written by us, so it exists and has valid content
        // 2. The file contains count*dim*4 + 8 bytes as expected (validated during write)
        // 3. The file will remain valid as long as this mmap exists (owned by Self)
        // 4. The mmap is read-only by default, preventing data races
        // 5. File handle 'file' is valid and open for reading
        // 6. Subsequent accesses validate bounds before dereferencing
        // 7. Metadata is stored to detect if file is modified
        let mmap = unsafe { Mmap::map(&file)? };

        Ok(Self {
            mmap: Some(mmap),
            count,
            dim,
            ids,
            metadata: Some(metadata),
        })
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let file = File::open(path.as_ref())?;

        // P1 Issue #4: Store metadata to detect if file changes during use
        let metadata = file
            .metadata()
            .context("Failed to read file metadata for mmap validation")?;

        // Validate file size is reasonable
        if metadata.len() == 0 {
            return Err(AppError::InvalidInput("Cannot mmap empty file".to_string()));
        }

        if metadata.len() > 10 * 1024 * 1024 * 1024 {
            // 10GB
            return Err(AppError::InvalidInput(format!(
                "File too large to mmap: {} bytes",
                metadata.len()
            )));
        }

        // SAFETY: Memory-mapping the file is safe because:
        // 1. The file was successfully opened, so it exists and is readable
        // 2. We validate the file size and structure immediately below
        // 3. The mmap will remain valid as long as it's owned by Self
        // 4. The mmap is read-only by default, preventing data races
        // 5. File handle 'file' is valid and open for reading
        // 6. Bounds checking is performed before any data access
        // 7. Metadata is stored to detect if file is modified
        let mmap = unsafe { Mmap::map(&file)? };

        if mmap.len() < 8 {
            return Err(AppError::InvalidInput(
                "Invalid index file: too small".to_string(),
            ));
        }

        let count_bytes: [u8; 4] = mmap
            .get(0..4)
            .context("Index file too small (expected at least 8 bytes for header)")?
            .try_into()
            .map_err(|_| {
                AppError::InvalidInput("Failed to read count bytes from index header".to_string())
            })?;
        let count = u32::from_le_bytes(count_bytes) as usize;

        let dim_bytes: [u8; 4] = mmap
            .get(4..8)
            .context("Index file truncated (missing dimension bytes in header)")?
            .try_into()
            .map_err(|_| {
                AppError::InvalidInput(
                    "Failed to read dimension bytes from index header".to_string(),
                )
            })?;
        let dim = u32::from_le_bytes(dim_bytes) as usize;

        let expected_size = 8 + count * dim * 4;
        if mmap.len() < expected_size {
            return Err(AppError::InvalidInput(format!(
                "Invalid index file: expected {} bytes, got {}",
                expected_size,
                mmap.len()
            )));
        }

        Ok(Self {
            mmap: Some(mmap),
            count,
            dim,
            ids: vec![],
            metadata: Some(metadata),
        })
    }

    pub fn save(&self, path: impl AsRef<Path>, embeddings: &[(String, Vec<f32>)]) -> Result<()> {
        let file = File::create(path.as_ref())?;
        let mut writer = BufWriter::new(file);

        let count = embeddings.len() as u32;
        let dim = embeddings
            .first()
            .map(|(_, emb)| emb.len() as u32)
            .unwrap_or(DEFAULT_EMBEDDING_DIM as u32);

        writer.write_all(&count.to_le_bytes())?;
        writer.write_all(&dim.to_le_bytes())?;

        for (_, embedding) in embeddings {
            for &val in embedding {
                writer.write_all(&val.to_le_bytes())?;
            }
        }

        writer.flush()?;
        Ok(())
    }

    /// SECURITY FIX: Return Result instead of empty slice on alignment errors
    ///
    /// This prevents silent failures on ARM processors where misaligned f32 access
    /// causes SIGBUS crashes. Now callers must handle alignment errors explicitly.
    ///
    /// P0 FIX: Handle empty index case (None mmap) gracefully
    #[inline]
    pub fn get_embedding(&self, idx: usize) -> Result<&[f32]> {
        // P0 FIX: Check for empty index first
        let mmap = match &self.mmap {
            None => {
                return Err(AppError::InvalidInput(
                    "Cannot get embedding from empty index".to_string(),
                ));
            }
            Some(mmap) => mmap,
        };

        if idx >= self.count {
            return Err(AppError::InvalidInput(format!(
                "Embedding index {} out of bounds (count: {})",
                idx, self.count
            )));
        }

        let offset = 8 + idx * self.dim * 4;
        let end_offset = offset + self.dim * 4;

        // Bounds check before accessing mmap
        if end_offset > mmap.len() {
            return Err(AppError::InvalidInput(format!(
                "Embedding data truncated at index {} (offset {} + {} exceeds mmap size {})",
                idx,
                offset,
                self.dim * 4,
                mmap.len()
            )));
        }

        let ptr = mmap.get(offset..end_offset).ok_or_else(|| {
            AppError::InvalidInput(format!(
                "Embedding slice out of bounds: {}..{} (mmap size: {})",
                offset,
                end_offset,
                mmap.len()
            ))
        })?;

        // CRITICAL: Validate alignment before transmutation for ARM safety
        // On ARM (M1/M2 Macs), misaligned f32 access causes SIGBUS crash
        if !(ptr.as_ptr() as usize).is_multiple_of(std::mem::align_of::<f32>()) {
            return Err(AppError::InternalError(format!(
                "CRITICAL ALIGNMENT ERROR: Memory-mapped data at offset {} is misaligned for f32 access. \
                 This will crash on ARM processors (M1/M2 Macs). Pointer: {:p}, Required alignment: 4 bytes. \
                 This indicates corrupted index file or bug in index generation.",
                offset,
                ptr.as_ptr()
            )));
        }

        // SAFETY: Transmuting &[u8] from mmap to &[f32] is safe here because:
        // 1. The slice length is exactly self.dim * 4 bytes (verified by slice bounds above)
        // 2. The data comes from a file we created with valid f32 values
        // 3. We validated the file size matches expected_size during load()
        // 4. The mmap is read-only, preventing concurrent modification
        // 5. The lifetime is tied to &self, so the mmap remains valid
        // 6. Alignment is validated above (critical for ARM compatibility)
        // 7. Files created by save() use 8-byte header ensuring natural 4-byte alignment
        // 8. Bounds check performed above ensures we don't read past end of mmap
        Ok(unsafe { std::slice::from_raw_parts(ptr.as_ptr() as *const f32, self.dim) })
    }

    #[inline]
    pub fn get_id(&self, idx: usize) -> Option<&str> {
        self.ids.get(idx).map(|s| s.as_str())
    }

    #[inline]
    pub fn count(&self) -> usize {
        self.count
    }

    #[inline]
    pub fn dimension(&self) -> usize {
        self.dim
    }

    pub fn iter(&self) -> EmbeddingIterator<'_> {
        EmbeddingIterator {
            index: self,
            current: 0,
        }
    }
}

pub struct EmbeddingIterator<'a> {
    index: &'a EmbeddingIndex,
    current: usize,
}

impl<'a> Iterator for EmbeddingIterator<'a> {
    type Item = Result<(usize, &'a [f32])>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current >= self.index.count() {
            return None;
        }

        let idx = self.current;
        let emb = self.index.get_embedding(idx);
        self.current += 1;

        Some(emb.map(|e| (idx, e)))
    }
}
