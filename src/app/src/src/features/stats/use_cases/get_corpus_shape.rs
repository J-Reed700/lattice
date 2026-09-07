//! # Get Corpus Shape Use Case
//!
//! Vault-wide type mix and recent growth, for surfaces that do not already hold
//! the document list (Home, the post-ingest sentence). The Library counts its
//! own array instead, so its counts and its filter can never disagree.

use std::sync::Arc;

use crate::features::stats::corpus_shape_repository::{CorpusShape, CorpusShapeRepositoryPort};
use crate::shared::error::Result;

pub struct GetCorpusShapeUseCase {
    repo: Arc<dyn CorpusShapeRepositoryPort>,
}

impl GetCorpusShapeUseCase {
    pub fn new(repo: Arc<dyn CorpusShapeRepositoryPort>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self) -> Result<CorpusShape> {
        self.repo.corpus_shape().await
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct StubRepo(CorpusShape);

    #[async_trait]
    impl CorpusShapeRepositoryPort for StubRepo {
        async fn corpus_shape(&self) -> Result<CorpusShape> {
            Ok(self.0.clone())
        }
    }

    #[tokio::test]
    async fn delegates_to_the_repository() {
        let expected = CorpusShape {
            total: 12,
            by_type: vec![("PDF".to_string(), 12)],
            grown_last_7_days: 3,
        };
        let use_case = GetCorpusShapeUseCase::new(Arc::new(StubRepo(expected.clone())));
        assert_eq!(use_case.execute().await.unwrap(), expected);
    }
}
