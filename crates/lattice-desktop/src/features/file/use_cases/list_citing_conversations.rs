//! # List Citing Conversations Use Case
//!
//! "Where has this document come up?" — the third list in the Library's
//! neighborhood panel.

use std::sync::Arc;

use crate::features::file::repository::{CitingConversation, CitingConversationsRepositoryPort};
use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::Page;

pub struct ListCitingConversationsUseCase {
    repo: Arc<dyn CitingConversationsRepositoryPort>,
}

impl ListCitingConversationsUseCase {
    pub fn new(repo: Arc<dyn CitingConversationsRepositoryPort>) -> Self {
        Self { repo }
    }

    /// Newest first. No `limit` lists every citing conversation.
    pub async fn execute(
        &self,
        document_id: String,
        limit: Option<i64>,
    ) -> Result<Vec<CitingConversation>> {
        self.repo
            .find_citing_conversations(&document_id, Page::new(limit, None)?)
            .await
    }
}

#[cfg(test)]
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
            page: Page,
        ) -> Result<Vec<CitingConversation>> {
            self.seen
                .lock()
                .map(|mut s| s.push((document_id.to_string(), page.sql_limit())))
                .ok();
            Ok(vec![])
        }
    }

    #[tokio::test]
    async fn passes_the_callers_limit_on_unchanged() {
        let repo = Arc::new(SpyRepo::default());
        let use_case = ListCitingConversationsUseCase::new(repo.clone());

        use_case.execute("doc".into(), None).await.unwrap();
        use_case.execute("doc".into(), Some(9_000)).await.unwrap();
        use_case.execute("doc".into(), Some(7)).await.unwrap();
        assert!(use_case.execute("doc".into(), Some(0)).await.is_err());

        let seen = repo.seen.lock().unwrap().clone();
        assert_eq!(
            seen.iter().map(|(_, l)| *l).collect::<Vec<_>>(),
            vec![-1, 9_000, 7]
        );
    }
}
