//! Which chats share memory: one pool per space, project folder or course,
//! worked out here and nowhere else (design: `docs/design/2026-10-28-tutor.md`
//! §3.4).
use super::ConversationRepository;
use lattice_core::shared::error::{AppError, Result};
use sqlx::SqliteConnection;
use std::path::Path;

/// What the user says about themselves. These reach the pool unless the user
/// keeps them to one chat; anything else is shared only by hand.
pub(super) const SHARED_KINDS: &str = "'user_fact','preference','goal'";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PoolKind {
    Space,
    Folder,
    Course,
}

impl PoolKind {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Space => "space",
            Self::Folder => "folder",
            Self::Course => "course",
        }
    }

    /// The chats in the pool, as a filter on `c` taking the pool's keys. A chat
    /// is in one pool, the one [`ConversationRepository::chat_pool`] gives it.
    fn members(self) -> &'static str {
        match self {
            Self::Space => {
                "c.space_id IN (SELECT value FROM json_each(?)) \
                 AND c.explorer_root IS NULL AND c.program_id IS NULL"
            }
            Self::Folder => "c.explorer_root IN (SELECT value FROM json_each(?))",
            Self::Course => {
                "c.program_id IN (SELECT value FROM json_each(?)) AND c.explorer_root IS NULL"
            }
        }
    }
}

/// One chat's pool. `keys` is every name it goes by: a folder's pool reaches
/// the folders inside it, and sharing saved before a parent was listed stays
/// shared.
#[derive(Debug, Clone)]
pub(super) struct ChatPool {
    pub(super) kind: PoolKind,
    /// What sharing with the pool saves.
    pub(super) key: String,
    keys: Vec<String>,
}

impl ChatPool {
    /// What `conversation` may read, as a filter on `i`, `a` and `c`. Bind
    /// [`Self::bind_values`] to its four parameters, in order.
    pub(super) fn readable_by_sql(&self) -> String {
        format!(
            "(i.conversation_id=? OR a.scope_kind='personal' \
             OR (a.scope_kind=? AND a.scope_key IN (SELECT value FROM json_each(?))) \
             OR (a.scope_kind IS NULL AND i.state='active' AND COALESCE(a.forgotten,0)=0 \
                 AND i.kind IN ({SHARED_KINDS}) AND {members}))",
            members = self.kind.members(),
        )
    }

    pub(super) fn bind_values(&self, conversation: &str) -> Result<[String; 4]> {
        let keys = serde_json::to_string(&self.keys)?;
        Ok([
            conversation.to_owned(),
            self.kind.name().to_owned(),
            keys.clone(),
            keys,
        ])
    }
}

/// The outermost listed folder holding `root`, so a chat on a subfolder joins
/// its parent's pool.
fn pool_root<'a>(root: &'a str, listed: &'a [String]) -> &'a str {
    listed
        .iter()
        .map(String::as_str)
        .filter(|folder| Path::new(root).starts_with(folder))
        .min_by_key(|folder| Path::new(folder).components().count())
        .unwrap_or(root)
}

impl ConversationRepository {
    pub(super) async fn chat_pool(
        connection: &mut SqliteConnection,
        conversation: &str,
    ) -> Result<ChatPool> {
        let (space, root, course): (String, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT space_id,explorer_root,program_id FROM conversations WHERE id=?",
        )
        .bind(conversation)
        .fetch_optional(&mut *connection)
        .await?
        .ok_or_else(|| AppError::NotFound("Conversation not found".into()))?;
        // In the order a turn reads them: a folder, then a course.
        let Some(root) = root else {
            let (kind, key) = match course {
                Some(course) => (PoolKind::Course, course),
                None => (PoolKind::Space, space),
            };
            return Ok(ChatPool {
                kind,
                keys: vec![key.clone()],
                key,
            });
        };
        let listed: Vec<String> = sqlx::query_scalar("SELECT root FROM explorer_folders")
            .fetch_all(&mut *connection)
            .await?;
        let chats: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT explorer_root FROM conversations WHERE explorer_root IS NOT NULL",
        )
        .fetch_all(&mut *connection)
        .await?;
        let key = pool_root(&root, &listed).to_owned();
        let mut keys: Vec<String> = listed
            .iter()
            .chain(&chats)
            .filter(|folder| Path::new(folder).starts_with(&key))
            .cloned()
            .collect();
        keys.push(key.clone());
        keys.sort();
        keys.dedup();
        Ok(ChatPool {
            kind: PoolKind::Folder,
            key,
            keys,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::pool_root;

    #[test]
    fn a_folder_inside_a_listed_folder_takes_the_outermost() {
        let listed = ["/a".to_owned(), "/a/b".to_owned(), "/z".to_owned()];
        assert_eq!(pool_root("/a/b/c", &listed), "/a");
        assert_eq!(pool_root("/a/b", &listed), "/a");
        assert_eq!(pool_root("/a", &listed), "/a");
    }

    #[test]
    fn a_folder_with_no_listed_parent_is_its_own_pool() {
        let listed = ["/a".to_owned()];
        assert_eq!(pool_root("/ab", &listed), "/ab");
    }
}
