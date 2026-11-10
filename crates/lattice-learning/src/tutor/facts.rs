//! The per-turn fact that comes from the conversation: how long ago the tutor
//! last checked what the learner took in.

use super::repository::assistant_replies;
use futures::TryStreamExt;
use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::DatabaseHandle;

/// Whether a reply put a question to the learner: its closing paragraph asks
/// one. A rhetorical close counts too; the number is a nudge, not a score.
fn asks_the_learner(reply: &str) -> bool {
    reply
        .trim_end()
        .rsplit("\n\n")
        .next()
        .is_some_and(|closing| closing.contains('?'))
}

/// Tutor replies since the last one that asked the learner something, newest
/// first, read only as far back as that one.
pub(super) async fn replies_since_check(
    pool: &impl DatabaseHandle,
    conversation_id: &str,
) -> Result<i64> {
    let mut replies = assistant_replies(pool, conversation_id);
    let mut since = 0;
    while let Some(reply) = replies.try_next().await? {
        if asks_the_learner(&reply) {
            break;
        }
        since += 1;
    }
    Ok(since)
}

#[cfg(test)]
mod tests {
    use super::asks_the_learner;

    #[test]
    fn a_reply_asks_when_its_closing_paragraph_holds_a_question() {
        assert!(asks_the_learner(
            "Fusion sums ranks [1].\n\nWhat does k do?"
        ));
        assert!(asks_the_learner("So why does it help? Try it.\n"));
        assert!(!asks_the_learner(
            "What does k do? It damps the top ranks [1].\n\nNext, weights."
        ));
        assert!(!asks_the_learner(""));
    }
}
