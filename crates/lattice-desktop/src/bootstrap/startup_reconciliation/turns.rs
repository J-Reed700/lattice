//! Conversation messages the last run left mid-flight: unfinished turns and grounding checks.

/// Close out grounding checks the last run never finished.
///
/// An answer is persisted with `verification.pending = true` and checked in a
/// background task; if the app quits under that task the marker stays, and the
/// message would show "Checking…" forever. Nothing re-runs the check (its
/// evidence lived in the turn), so the honest state is "not checked".
pub async fn reconcile_interrupted_verifications(
    pool: &sqlx::SqlitePool,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE conversation_messages \
         SET metadata = json_set(metadata, '$.verification.pending', json('false'), \
                                          '$.verification.interrupted', json('true')) \
         WHERE json_valid(metadata) \
           AND json_extract(metadata, '$.verification.pending') = 1",
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Close out turns the last run never finished.
///
/// A question is written `pending` before generation and settled when the
/// answer commits or the turn fails. A crash or force-quit in between leaves it
/// `pending`, which renders as still in flight with nothing left to finish it.
/// No turn survives a restart, so every such row is failed and can be retried.
pub async fn reconcile_stuck_turns(pool: &sqlx::SqlitePool) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE conversation_messages SET status = 'failed' \
         WHERE status IN ('pending', 'streaming')",
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_check_the_last_run_never_finished_is_marked_not_checked() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO conversations (id, title) VALUES ('c', 'c')")
            .execute(&pool)
            .await
            .unwrap();
        for (id, metadata) in [
            (
                "waiting",
                r#"{"verification":{"enabled":true,"pending":true},"turn":{"x":1}}"#,
            ),
            (
                "checked",
                r#"{"verification":{"enabled":true,"pending":false,"claimsEvaluated":3}}"#,
            ),
            ("off", r#"{"verification":{"enabled":false}}"#),
        ] {
            sqlx::query(
                "INSERT INTO conversation_messages (id, conversation_id, role, content, metadata) \
                 VALUES (?, 'c', 'assistant', 'a', ?)",
            )
            .bind(id)
            .bind(metadata)
            .execute(&pool)
            .await
            .unwrap();
        }

        let marked = reconcile_interrupted_verifications(&pool).await.unwrap();
        assert_eq!(marked, 1);

        let waiting: String =
            sqlx::query_scalar("SELECT metadata FROM conversation_messages WHERE id = 'waiting'")
                .fetch_one(&pool)
                .await
                .unwrap();
        let waiting: serde_json::Value = serde_json::from_str(&waiting).unwrap();
        assert_eq!(waiting["verification"]["pending"], false);
        assert_eq!(waiting["verification"]["interrupted"], true);
        // The rest of the metadata is untouched.
        assert_eq!(waiting["turn"]["x"], 1);

        let checked: String =
            sqlx::query_scalar("SELECT metadata FROM conversation_messages WHERE id = 'checked'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(!checked.contains("interrupted"));
    }

    #[tokio::test]
    async fn a_turn_the_last_run_never_finished_is_marked_failed() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO conversations (id, title) VALUES ('c', 'c')")
            .execute(&pool)
            .await
            .unwrap();
        for (id, role, status) in [
            ("asked", "user", "pending"),
            ("half-written", "assistant", "streaming"),
            ("answered", "user", "completed"),
            ("already-failed", "user", "failed"),
        ] {
            sqlx::query(
                "INSERT INTO conversation_messages (id, conversation_id, role, content, status) \
                 VALUES (?, 'c', ?, 'text', ?)",
            )
            .bind(id)
            .bind(role)
            .bind(status)
            .execute(&pool)
            .await
            .unwrap();
        }

        assert_eq!(reconcile_stuck_turns(&pool).await.unwrap(), 2);

        let statuses: Vec<(String, String)> =
            sqlx::query_as("SELECT id, status FROM conversation_messages ORDER BY id")
                .fetch_all(&pool)
                .await
                .unwrap();
        let status = |id: &str| {
            statuses
                .iter()
                .find(|(row, _)| row == id)
                .map(|(_, status)| status.as_str())
                .unwrap()
        };
        assert_eq!(status("asked"), "failed");
        assert_eq!(status("half-written"), "failed");
        assert_eq!(status("answered"), "completed");
        assert_eq!(status("already-failed"), "failed");
    }
}
