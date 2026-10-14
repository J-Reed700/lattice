//! Every course, one pack each, into one folder: a backup, or a move of a
//! whole library. One course failing does not stop the others.
use super::{
    files::{chosen_folder, DestinationPicker},
    hash, invalid,
    records::pack_operation,
    validate_uuid, Destination, LearningPackRepository,
};
use crate::features::learning::{
    portability_dto::{
        ExportAllLearningPacksRequestDto, ExportAllLearningPacksResultDto,
        ExportLearningPackRequestDto, ExportedLearningPackDto, FailedLearningPackExportDto,
    },
    repository::LearningRepository,
};
use crate::shared::error::{AppError, Result};
use sha2::{Digest, Sha256};
use std::path::Path;

const KIND: &str = "export_all";

/// A course's operation ID within a batch: the same batch, folder and course
/// always give the same ID, so a retry into the same folder replays the
/// exports already written there instead of writing `(2)` copies, and a
/// retry into another folder writes fresh packs there. Name-based like
/// UUIDv5, over SHA-256.
fn course_operation_id(batch: &uuid::Uuid, dir: &Path, program_id: &str) -> String {
    let folder = dir.as_os_str().as_encoded_bytes();
    let mut hasher = Sha256::new();
    hasher.update(batch.as_bytes());
    hasher.update((folder.len() as u64).to_le_bytes());
    hasher.update(folder);
    hasher.update(program_id.as_bytes());
    let digest = hasher.finalize();
    let bytes: [u8; 16] = digest.first_chunk().copied().unwrap_or_default();
    uuid::Builder::from_custom_bytes(bytes)
        .into_uuid()
        .to_string()
}

/// The user-facing reason, without the generic error-category wrapper.
fn reason(error: &AppError) -> String {
    match error {
        AppError::InvalidInput(message) | AppError::NotFound(message) => message.clone(),
        other => other.to_string(),
    }
}

impl LearningPackRepository {
    /// Asks `picker` for a folder, then exports every course into it. A
    /// retried batch answers with what it already did, without asking again.
    pub async fn export_all(
        &self,
        req: &ExportAllLearningPacksRequestDto,
        picker: &dyn DestinationPicker,
    ) -> Result<ExportAllLearningPacksResultDto> {
        validate_uuid(&req.operation_id, "operation")?;
        let payload = hash(req)?;
        if let Some(done) = pack_operation(&req.operation_id, KIND, &payload)
            .replay(&self.pool)
            .await?
        {
            return Ok(done);
        }
        let Some(chosen) = picker.pick_folder().await? else {
            return Ok(ExportAllLearningPacksResultDto::Cancelled);
        };
        let batch = uuid::Uuid::parse_str(&req.operation_id)
            .map_err(|_| AppError::InvalidInput("Invalid operation ID.".into()))?;
        let dir = chosen_folder(&self.app_data_dir, &chosen).await?;
        let (mut exported, mut failed) = (Vec::new(), Vec::new());
        for course in LearningRepository::new(self.pool.clone()).list().await? {
            let request = ExportLearningPackRequestDto {
                operation_id: course_operation_id(&batch, &dir, &course.id),
                program_id: course.id.clone(),
                options: req.options.clone(),
            };
            // Only a picker can be dismissed; a folder always has a free name.
            let written = self
                .export_to_path(&request, Destination::Folder(&dir))
                .await
                .and_then(|done| done.ok_or_else(|| invalid("The export was cancelled.")));
            match written {
                Ok(export) => exported.push(ExportedLearningPackDto {
                    program_id: course.id,
                    title: course.title,
                    path: export.destination_path,
                }),
                Err(error) => failed.push(FailedLearningPackExportDto {
                    program_id: course.id,
                    title: course.title,
                    error: reason(&error),
                }),
            }
        }
        let result = ExportAllLearningPacksResultDto::Exported {
            destination_dir: dir.to_string_lossy().into_owned(),
            exported,
            failed,
        };
        pack_operation(&req.operation_id, KIND, &payload)
            .record(&self.pool, &result)
            .await?;
        Ok(result)
    }
}
