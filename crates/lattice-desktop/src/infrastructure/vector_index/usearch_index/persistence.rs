//! Opening, saving and reloading the index file and its key map.
use super::{
    temp_path_for, write_file_atomically, DirtyState, KeyState, USearchVectorIndex, VectorMeta,
};
use crate::infrastructure::vector_index::compression::VectorIndexCompression;
use lattice_core::shared::error::{AppError, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

/// Serializable key map for persistence beside the USearch index file. Kept
/// as JSON: an entry is a handful of short strings, and a readable format is
/// worth more than a binary one's milliseconds.
#[derive(serde::Serialize, serde::Deserialize)]
struct KeyMapData {
    id_to_key: HashMap<String, u64>,
    next_key: u64,
    metadata: HashMap<String, SerializableMeta>,
}

/// The persisted half of [`VectorMeta`]. Chunk text is deliberately absent:
/// SQLite already holds it.
#[derive(serde::Serialize, serde::Deserialize)]
struct SerializableMeta {
    chunk_id: String,
    document_id: String,
}

impl USearchVectorIndex {
    /// Load an existing uncompressed index from disk, or create a new one.
    pub fn open_or_create(dimension: usize, index_path: PathBuf) -> Result<Self> {
        Self::open_or_create_with_compression(dimension, index_path, VectorIndexCompression::None)
    }

    /// Load an existing index from disk, or create a new one if it doesn't exist.
    ///
    /// The caller is responsible for having run
    /// [`ensure_index_layout_match`](super::super::dimension_metadata::ensure_index_layout_match)
    /// first: an index file written under a different configuration will fail
    /// to load here rather than being silently reinterpreted.
    pub fn open_or_create_with_compression(
        dimension: usize,
        index_path: PathBuf,
        compression: VectorIndexCompression,
    ) -> Result<Self> {
        let instance = Self::with_compression(dimension, Some(index_path.clone()), compression)?;
        instance.discard_stale_temporaries();

        if index_path.exists() {
            let path_str = index_path.to_string_lossy().to_string();
            // `load`, not `view`: `view` maps the file read-only for an
            // instant, allocation-free open, but this index is mutated in
            // place every time a document is indexed, and USearch cannot grow
            // or rewrite a viewed index. The startup cost `view` would save is
            // reading one already-built graph, which is exactly the cost the
            // manifest check exists to make rare.
            instance.index.load(&path_str).map_err(|e| {
                AppError::InternalError(format!("Failed to load USearch index from disk: {}", e))
            })?;

            if let Some(ref keymap_path) = instance.keymap_path {
                if keymap_path.exists() {
                    instance.load_keymap(keymap_path)?;
                } else {
                    tracing::warn!(
                        "USearch index exists but key map is missing — index will be empty until rebuild"
                    );
                }
            }

            tracing::info!(
                path = %index_path.display(),
                size = instance.index.size(),
                "USearch index loaded from disk"
            );
        } else {
            // Capacity is not reserved up front: `ensure_capacity` grows the
            // index on demand from the first insert.
            tracing::info!(
                path = %index_path.display(),
                "Created new USearch index"
            );
        }

        Ok(instance)
    }

    /// Save the index and key map to disk.
    ///
    /// Both go out through a temporary file and a rename, so a crash mid-save
    /// leaves the previous snapshot intact rather than a half-written graph
    /// USearch would refuse to load. The manifest that vouches for this
    /// snapshot is written by the caller afterwards — see
    /// the search feature's vector index manifest — because only it knows what SQLite looked like.
    pub fn save_to_disk(&self) -> Result<()> {
        self.save_snapshot().map(|_| ())
    }

    /// Return the count under the snapshot lock, before any later mutation.
    pub(crate) fn save_snapshot(&self) -> Result<usize> {
        let _snapshot = self.persistence.lock();
        // Hold the same read lock used by searches while saving the vectors
        // and metadata, so additions cannot split the two snapshots.
        let state = self.state.read();
        if let Some(ref path) = self.index_path {
            // The graph and keymap are separate files. Invalidate the old
            // manifest before replacing either, so an interrupted save cannot
            // leave it certifying a mixture of snapshots with equal counts.
            let manifest = super::super::index_config::manifest_path_for(path);
            match std::fs::remove_file(&manifest) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    return Err(AppError::FileStorage(format!(
                        "Failed to invalidate vector index manifest: {e}"
                    )));
                }
            }
            // Ensure parent directory exists
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    AppError::FileStorage(format!("Failed to create index directory: {}", e))
                })?;
            }

            let temp = temp_path_for(path);
            let temp_str = temp.to_string_lossy().to_string();
            self.index.save(&temp_str).map_err(|e| {
                AppError::FileStorage(format!("Failed to save USearch index: {}", e))
            })?;
            std::fs::rename(&temp, path).map_err(|e| {
                let _ = std::fs::remove_file(&temp);
                AppError::FileStorage(format!("Failed to replace USearch index: {}", e))
            })?;

            tracing::debug!(path = %path.display(), "USearch index saved to disk");
        }

        if let Some(ref path) = self.keymap_path {
            self.save_keymap(path, &state)?;
            tracing::debug!(path = %path.display(), "Key map saved to disk");
        }

        *self.dirty.lock() = DirtyState::default();

        Ok(self.index.size())
    }

    /// Remove temporaries a crashed save left behind.
    ///
    /// They are never read — the real files are only ever replaced by an
    /// atomic rename — but leaving a corpus-sized file lying around is not
    /// something a user should have to clean up by hand.
    fn discard_stale_temporaries(&self) {
        for path in [self.index_path.as_deref(), self.keymap_path.as_deref()]
            .into_iter()
            .flatten()
            .map(temp_path_for)
        {
            if path.exists() {
                match std::fs::remove_file(&path) {
                    Ok(()) => {
                        tracing::info!(path = %path.display(), "Removed a temporary file from an interrupted index save")
                    }
                    Err(e) => {
                        tracing::warn!(path = %path.display(), error = %e, "Could not remove a stale index temporary")
                    }
                }
            }
        }
    }

    /// Save the key map to a JSON file (atomic via temp file + rename).
    fn save_keymap(&self, path: &Path, state: &KeyState) -> Result<()> {
        let data = KeyMapData {
            id_to_key: state.id_to_key.clone(),
            next_key: self.next_key.load(Ordering::SeqCst),
            metadata: state
                .metadata
                .iter()
                .map(|(id, m)| {
                    (
                        id.clone(),
                        SerializableMeta {
                            chunk_id: m.chunk_id.clone(),
                            document_id: m.document_id.clone(),
                        },
                    )
                })
                .collect(),
        };

        let bytes = serde_json::to_vec(&data)
            .map_err(|e| AppError::Serialization(format!("Failed to serialize key map: {}", e)))?;
        write_file_atomically(path, &bytes)
    }

    /// Check membership for a bounded page of authoritative chunk IDs. The
    /// contents are deliberately not copied into the index.
    pub fn hydrate_content<I>(&self, rows: I) -> usize
    where
        I: IntoIterator<Item = String>,
    {
        let state = self.state.read();
        rows.into_iter()
            .filter(|id| state.id_to_key.contains_key(id))
            .count()
    }

    /// Load the key map from a JSON file.
    fn load_keymap(&self, path: &Path) -> Result<()> {
        let file = std::fs::File::open(path)
            .map_err(|e| AppError::FileStorage(format!("Failed to open key map file: {}", e)))?;
        let reader = std::io::BufReader::new(file);
        let data: KeyMapData = serde_json::from_reader(reader).map_err(|e| {
            AppError::Deserialization(format!("Failed to deserialize key map: {}", e))
        })?;

        // Rebuild maps under a single lock
        let mut state = self.state.write();

        state.id_to_key = data.id_to_key.clone();
        state.key_to_id = data
            .id_to_key
            .into_iter()
            .map(|(id, key)| (key, id))
            .collect();

        for (id, m) in data.metadata {
            if let Some(&key) = state.id_to_key.get(&id) {
                state
                    .document_to_keys
                    .entry(m.document_id.clone())
                    .or_default()
                    .insert(key);
            }
            state.metadata.insert(
                id,
                VectorMeta {
                    chunk_id: m.chunk_id,
                    document_id: m.document_id,
                },
            );
        }
        let KeyState {
            id_to_key,
            metadata,
            document_to_keys,
            ..
        } = &mut *state;
        for (id, key) in id_to_key.iter() {
            if !metadata.contains_key(id) {
                document_to_keys.entry(id.clone()).or_default().insert(*key);
            }
        }

        self.next_key.store(data.next_key, Ordering::SeqCst);

        Ok(())
    }
}
