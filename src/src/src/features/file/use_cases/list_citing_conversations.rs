//! # List Citing Conversations Use Case
//!
//! "Where has this document come up?" — the third list in the Library's
//! neighborhood panel.

use std::sync::Arc;

use crate::features::file::repository::{CitingConversation, CitingConversationsRepositoryPort};
use crate::shared::error::Result;

const DEFAULT_LIMIT: i64 = 10;
const MAX_LIMIT: i64 = 50;

pub struct ListCitingConversationsUseCase {
    repo: Arc<dyn CitingConversationsRepositoryPort>,
}

impl ListCitingConversationsUseCase {
    pub fn new(repo: Arc<dyn CitingConversationsRepositoryPort>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        document_id: String,
        limit: Option<i64>,
    ) -> Result<Vec<CitingConversation>> {
        let limit = limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        self.repo
            .find_citing_conversations(&document_id, limit)
            .await
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;

    #[derive(Default)]
    struct SpyRepo {
        seen: Mutex<Vec<(String, i64)>>,
    }

    #[async_trait]
    impl CitingConversationsRepositoryPort for SpyRepo {
        async fn find_citing_conversations(
            &self,
            document_id: &str,
            limit: i64,
        ) -> Result<Vec<CitingConversation>> {
            self.seen
                .lock()
                .map(|mut s| s.push((document_id.to_string(), limit)))
                .ok();
            Ok(vec![])
        }
    }

    #[tokio::test]
    async fn defaults_and_clamps_the_limit() {
        let repo = Arc::new(SpyRepo::default());
        let use_case = ListCitingConversationsUseCase::new(repo.clone());

        use_case.execute("doc".into(), None).await.unwrap();
        use_case.execute("doc".into(), Some(0)).await.unwrap();
        use_case.execute("doc".into(), Some(9_000)).await.unwrap();
        use_case.execute("doc".into(), Some(7)).await.unwrap();

        let seen = repo.seen.lock().unwrap().clone();
        assert_eq!(
            seen.iter().map(|(_, l)| *l).collect::<Vec<_>>(),
            vec![10, 1, 50, 7]
        );
    }
}
