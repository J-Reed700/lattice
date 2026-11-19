//! # Get Corpus Shape Use Case
//!
//! Vault-wide type mix and recent growth, for surfaces that do not already hold
//! the document list (Home, the post-ingest sentence). The Library counts its
//! own array instead, so its counts and its filter can never disagree.

use std::sync::Arc;

use crate::features::stats::corpus_shape_repository::{CorpusShape, CorpusShapeRepositoryPort};
use lattice_core::domain::ports::article_archive::ArticleArchive;
use lattice_core::shared::error::Result;

pub struct GetCorpusShapeUseCase {
    repo: Arc<dyn CorpusShapeRepositoryPort>,
}

impl GetCorpusShapeUseCase {
    pub fn new(repo: Arc<dyn CorpusShapeRepositoryPort>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self, archive: &dyn ArticleArchive) -> Result<CorpusShape> {
        self.repo.corpus_shape(archive).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use lattice_core::domain::ports::article_archive::{ArticleArchive, SavedArticle};
    use std::path::Path;

    struct NoArchive;

    #[async_trait]
    impl ArticleArchive for NoArchive {
        async fn delete_article(&self, _path: &Path) -> Result<()> {
            Ok(())
        }

        fn owns(&self, _path: &Path) -> bool {
            false
        }

        fn is_saved_article(&self, _path: &Path) -> bool {
            false
        }

        async fn saved_article(&self, _path: &Path) -> Option<SavedArticle> {
            None
        }
    }

    struct StubRepo(CorpusShape);

    #[async_trait]
    impl CorpusShapeRepositoryPort for StubRepo {
        async fn corpus_shape(&self, _archive: &dyn ArticleArchive) -> Result<CorpusShape> {
            Ok(self.0.clone())
        }
    }

    #[tokio::test]
    async fn delegates_to_the_repository() {
        let expected = CorpusShape {
            total: 12,
            by_type: vec![("PDF".to_string(), 12)],
            grown_last_7_days: 3,
            grown_today: 1,
            last_indexed_at: Some("2026-05-15T09:00:00+00:00".to_string()),
        };
        let use_case = GetCorpusShapeUseCase::new(Arc::new(StubRepo(expected.clone())));
        assert_eq!(use_case.execute(&NoArchive).await.unwrap(), expected);
    }
}
