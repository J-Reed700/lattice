use super::*;
use chrono::Utc;
use lattice_core::domain::entities::document::{
    Category, DocumentStatus, Language, StoredDocument,
};
use lattice_core::domain::entities::Document;
use lattice_core::domain::ports::article_archive::ArticleArchive;
use lattice_core::domain::value_objects::checksum::Checksum;
use lattice_core::shared::types::{DocumentId, ValidatedFilePath};
use std::path::{Path, PathBuf};

struct ArchiveAt(PathBuf);

#[async_trait::async_trait]
impl ArticleArchive for ArchiveAt {
    async fn delete_article(&self, _path: &Path) -> Result<()> {
        Ok(())
    }

    fn owns(&self, path: &Path) -> bool {
        path.starts_with(&self.0)
    }
}

fn document(path: PathBuf, file_type: &str, category: Category) -> Document {
    let now = Utc::now();
    Document::with_id(StoredDocument {
        id: DocumentId::new(),
        file_path: ValidatedFilePath::new(path).expect("valid path"),
        file_name: format!("file.{file_type}"),
        file_type: Some(file_type.to_string()),
        mime_type: "text/plain".to_string(),
        size_bytes: 1,
        modified_at: now,
        indexed_at: now,
        checksum: Checksum::new("0".repeat(64)).expect("valid checksum"),
        status: DocumentStatus::Indexed,
        error_message: None,
        language: Language::Unknown,
        category,
        quality_score: 0.5,
        access_count: 0,
        last_accessed_at: None,
        word_count: 0,
        content: String::new(),
    })
}

fn archive_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("temp dir")
}

#[test]
fn a_file_inside_the_archive_is_web() {
    let dir = archive_dir();
    let archive = ArchiveAt(dir.path().to_path_buf());
    let doc = document(
        dir.path().join("some-article").join("article.md"),
        "md",
        Category::Uncategorized,
    );

    let dto = document_metadata(&doc, &archive);

    assert!(dto.web);
    assert_eq!(dto.type_bucket, "Web");
}

#[test]
fn a_pdf_outside_the_archive_is_not_web() {
    let dir = archive_dir();
    let other = archive_dir();
    let archive = ArchiveAt(dir.path().to_path_buf());
    let doc = document(other.path().join("paper.pdf"), "pdf", Category::Research);

    let dto = document_metadata(&doc, &archive);

    assert!(!dto.web);
    assert_eq!(dto.type_bucket, "PDF");
}
