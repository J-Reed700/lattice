//! Limits for reading compressed office document parts.

use crate::features::indexing::engine::error::{IndexingError, Result};
use std::fs::File;
use std::io::{Read, Take};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::Semaphore;
use zip::ZipArchive;

/// Maximum number of entries in one office archive, including unused media.
pub(super) const MAX_ARCHIVE_MEMBERS: usize = 20_000;
/// Maximum uncompressed size of any one XML part.
pub(super) const MAX_ARCHIVE_MEMBER_BYTES: u64 = 16 * 1024 * 1024;
/// Maximum uncompressed bytes read from relevant parts in one document.
pub(super) const MAX_ARCHIVE_DOCUMENT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ARCHIVE_WORKERS: usize = 2;
const ARCHIVE_WORK_TIMEOUT: Duration = Duration::from_secs(30);

static ARCHIVE_WORKERS: OnceLock<Arc<Semaphore>> = OnceLock::new();

fn archive_workers() -> Arc<Semaphore> {
    Arc::clone(ARCHIVE_WORKERS.get_or_init(|| Arc::new(Semaphore::new(MAX_ARCHIVE_WORKERS))))
}

/// Run one office document extraction under a process-wide worker limit.
/// Admission happens before `spawn_blocking`; the blocking closure owns the
/// permit until it exits, even if this future times out or is cancelled.
pub(super) async fn run_archive_work<T, F>(path: PathBuf, operation: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    run_archive_work_with_gate(archive_workers(), path, ARCHIVE_WORK_TIMEOUT, operation).await
}

async fn run_archive_work_with_gate<T, F>(
    workers: Arc<Semaphore>,
    path: PathBuf,
    timeout: Duration,
    operation: F,
) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    let timeout_path = path.clone();
    tokio::time::timeout(timeout, async move {
        let permit = workers.acquire_owned().await.map_err(|_| {
            limit_error(&path, "office extraction admission was closed".to_string())
        })?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            operation()
        })
        .await
        .map_err(|error| limit_error(&path, format!("office extraction worker failed: {error}")))?
    })
    .await
    .map_err(|_| {
        limit_error(
            &timeout_path,
            "office extraction exceeded its 30 second budget".to_string(),
        )
    })?
}

#[derive(Default)]
pub(super) struct ArchiveBudget {
    consumed: u64,
}

impl ArchiveBudget {
    pub(super) fn check_members(
        &self,
        archive: &ZipArchive<File>,
        path: &std::path::Path,
    ) -> Result<()> {
        if archive.len() > MAX_ARCHIVE_MEMBERS {
            return Err(limit_error(
                path,
                format!(
                    "archive has {} members; limit is {MAX_ARCHIVE_MEMBERS}",
                    archive.len()
                ),
            ));
        }
        Ok(())
    }

    pub(super) fn read_part(
        &mut self,
        archive: &mut ZipArchive<File>,
        name: &str,
        path: &std::path::Path,
    ) -> Result<String> {
        // `file_names()` iterates the ZIP crate's name map, whose order is
        // not the central-directory index order. Converting its position to
        // `by_index` can silently read a different part. Resolve and retain
        // the member by name so metadata checks and capped reads target the
        // same entry.
        let mut member = archive
            .by_name(name)
            .map_err(|e| limit_error(path, format!("could not open {name}: {e}")))?;
        let declared = member.size();
        let remaining = MAX_ARCHIVE_DOCUMENT_BYTES.saturating_sub(self.consumed);
        if declared > MAX_ARCHIVE_MEMBER_BYTES {
            return Err(limit_error(path, format!("{name} declares {declared} uncompressed bytes; member limit is {MAX_ARCHIVE_MEMBER_BYTES}")));
        }
        if declared > remaining {
            return Err(limit_error(path, format!("{name} declares {declared} uncompressed bytes; document has only {remaining} bytes remaining")));
        }

        let mut bytes = Vec::with_capacity(declared.min(64 * 1024) as usize);
        let mut limited: Take<_> = member
            .by_ref()
            .take(remaining.min(MAX_ARCHIVE_MEMBER_BYTES) + 1);
        limited
            .read_to_end(&mut bytes)
            .map_err(|e| limit_error(path, format!("could not read {name}: {e}")))?;
        if bytes.len() as u64 > remaining.min(MAX_ARCHIVE_MEMBER_BYTES) {
            return Err(limit_error(
                path,
                format!("{name} exceeded its uncompressed byte budget while reading"),
            ));
        }
        self.consumed = self.consumed.saturating_add(bytes.len() as u64);
        String::from_utf8(bytes)
            .map_err(|e| limit_error(path, format!("{name} is not UTF-8 XML: {e}")))
    }
}

fn limit_error(path: &std::path::Path, reason: String) -> IndexingError {
    IndexingError::ContentExtraction {
        path: path.display().to_string(),
        reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[tokio::test]
    async fn timed_out_worker_keeps_its_shared_slot_until_the_blocking_work_exits() {
        let workers = Arc::new(Semaphore::new(1));
        let task_workers = Arc::clone(&workers);
        let work = tokio::spawn(async move {
            run_archive_work_with_gate(
                task_workers,
                PathBuf::from("slow.docx"),
                Duration::from_millis(5),
                || {
                    std::thread::sleep(Duration::from_millis(60));
                    Ok(())
                },
            )
            .await
        });

        assert!(work.await.unwrap().is_err());
        assert_eq!(workers.available_permits(), 0);
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert_eq!(workers.available_permits(), 1);
    }

    #[test]
    fn rejects_a_highly_compressible_member_before_materializing_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bomb.docx");
        let mut writer = zip::ZipWriter::new(File::create(&path).unwrap());
        writer
            .start_file("word/document.xml", zip::write::FileOptions::default())
            .unwrap();
        let chunk = vec![b'a'; 1024 * 1024];
        for _ in 0..(MAX_ARCHIVE_MEMBER_BYTES / chunk.len() as u64 + 1) {
            writer.write_all(&chunk).unwrap();
        }
        writer.finish().unwrap();
        assert!(std::fs::metadata(&path).unwrap().len() < 100_000);

        let mut archive = ZipArchive::new(File::open(&path).unwrap()).unwrap();
        let mut budget = ArchiveBudget::default();
        budget.check_members(&archive, &path).unwrap();
        assert!(budget
            .read_part(&mut archive, "word/document.xml", &path)
            .is_err());
    }
}
