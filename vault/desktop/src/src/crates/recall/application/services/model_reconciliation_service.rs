use crate::domain::entities::model::Model;
use crate::domain::entities::model_file::ModelFile;
use crate::domain::repositories::model_repository::ModelRepository;
use crate::domain::value_objects::model_status::{FileStatus, ModelStatus};
use crate::error::AppError;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, info, warn};

pub struct ModelReconciliationService {
    repository: Arc<dyn ModelRepository>,
}

impl ModelReconciliationService {
    pub fn new(repository: Arc<dyn ModelRepository>) -> Self {
        Self { repository }
    }

    pub async fn reconcile_filesystem_with_database(
        &self,
        base_models_path: &Path,
    ) -> Result<ReconciliationReport, AppError> {
        info!("Starting filesystem reconciliation");

        let db_models = self.repository.find_all().await?;
        let mut report = ReconciliationReport::default();

        for model in &db_models {
            let model_path = PathBuf::from(&model.base_path);

            if !model_path.exists() {
                warn!(model_id = %model.model_id, path = %model.base_path,
                      "Model path does not exist in filesystem");
                report.missing_from_filesystem.push(model.model_id.clone());
                continue;
            }

            for file in &model.files {
                let file_path = PathBuf::from(&file.file_path);

                if !file_path.exists() {
                    warn!(model_id = %model.model_id, file = %file.file_name,
                          "Model file does not exist in filesystem");
                    report
                        .missing_files
                        .push(format!("{}:{}", model.model_id, file.file_name));

                    if file.status == FileStatus::Completed {
                        self.repository
                            .update_file_progress(
                                &model.model_id,
                                &file.file_name,
                                0,
                                FileStatus::Failed,
                            )
                            .await?;
                        report.status_corrections += 1;
                    }
                } else if file.status == FileStatus::Completed {
                    if let Ok(metadata) = std::fs::metadata(&file_path) {
                        let actual_size = metadata.len() as i64;
                        if actual_size != file.size_bytes {
                            warn!(
                                model_id = %model.model_id,
                                file = %file.file_name,
                                expected = file.size_bytes,
                                actual = actual_size,
                                "File size mismatch"
                            );
                            report
                                .size_mismatches
                                .push(format!("{}:{}", model.model_id, file.file_name));
                        }
                    }
                }
            }

            let all_files_complete = model
                .files
                .iter()
                .all(|f| f.status == FileStatus::Completed);
            if all_files_complete && model.status != ModelStatus::Completed {
                info!(model_id = %model.model_id, "Correcting model status to completed");
                self.repository
                    .update_status(&model.model_id, ModelStatus::Completed)
                    .await?;
                report.status_corrections += 1;
            } else if !all_files_complete && model.status == ModelStatus::Completed {
                info!(model_id = %model.model_id, "Correcting model status to downloading");
                self.repository
                    .update_status(&model.model_id, ModelStatus::Downloading)
                    .await?;
                report.status_corrections += 1;
            }

            report.reconciled_models += 1;
        }

        info!(
            reconciled = report.reconciled_models,
            corrections = report.status_corrections,
            missing_files = report.missing_files.len(),
            "Reconciliation complete"
        );

        Ok(report)
    }

    pub async fn verify_model_integrity(
        &self,
        model_id: &str,
    ) -> Result<IntegrityReport, AppError> {
        let model = self
            .repository
            .find_by_model_id(model_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Model not found: {}", model_id)))?;

        let mut report = IntegrityReport {
            model_id: model_id.to_string(),
            is_valid: true,
            issues: Vec::new(),
        };

        let model_path = PathBuf::from(&model.base_path);
        if !model_path.exists() {
            report.is_valid = false;
            report.issues.push(format!(
                "Model directory does not exist: {}",
                model.base_path
            ));
            return Ok(report);
        }

        for file in &model.files {
            let file_path = PathBuf::from(&file.file_path);

            if !file_path.exists() {
                report.is_valid = false;
                report
                    .issues
                    .push(format!("File missing: {}", file.file_name));
            } else if file.status == FileStatus::Completed {
                if let Ok(metadata) = std::fs::metadata(&file_path) {
                    let actual_size = metadata.len() as i64;
                    if actual_size != file.size_bytes {
                        report.is_valid = false;
                        report.issues.push(format!(
                            "File size mismatch for {}: expected {}, got {}",
                            file.file_name, file.size_bytes, actual_size
                        ));
                    }
                }
            }
        }

        Ok(report)
    }
}

#[derive(Default, Debug)]
pub struct ReconciliationReport {
    pub reconciled_models: usize,
    pub status_corrections: usize,
    pub missing_from_filesystem: Vec<String>,
    pub missing_files: Vec<String>,
    pub size_mismatches: Vec<String>,
}

#[derive(Debug)]
pub struct IntegrityReport {
    pub model_id: String,
    pub is_valid: bool,
    pub issues: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_reconciliation_service() {
        // Test would require mock repository
    }
}
