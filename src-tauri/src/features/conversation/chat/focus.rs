//! Pinning a chat to particular documents.
//!
//! The composer's `@` lets a reader say "only these files". That is a request,
//! and a request may only ever **narrow** what a turn can read. The ids are
//! intersected with the conversation's space scope — the same allow-list every
//! other retrieval path derives from — and anything outside it is dropped.
//!
//! The empty-after-intersection case is the one that matters. A request that
//! named documents this chat cannot reach must leave the turn searching
//! nothing, not falling back to the whole space: falling back would turn "only
//! the Halvorsen paper" into "everything except the Halvorsen paper", which is
//! the opposite of what was asked and looks exactly like a scope leak.

use std::collections::HashSet;

use tracing::{info, warn};

use crate::features::conversation::repository::ConversationRepository;
use crate::interfaces::di::Container;

/// The documents a turn is confined to, after the space scope has had its say.
#[derive(Debug, Clone, Default)]
pub(super) struct FocusScope {
    /// How many distinct ids the request named. Zero means no focus was asked
    /// for and the turn reads its whole space as usual.
    requested: usize,
    /// Those of them the conversation's space actually allows.
    allowed: HashSet<String>,
}

impl FocusScope {
    /// Resolve a request's ids against the conversation's space scope.
    ///
    /// A closed-book turn ignores focus entirely: it reads nothing from the
    /// vault whatever the request says, and `closed_book` has to keep winning
    /// over every other preference.
    pub(super) async fn resolve(
        container: &Container,
        conversation_id: &str,
        requested_ids: Option<&Vec<String>>,
        closed_book: bool,
    ) -> Self {
        if closed_book || normalize(requested_ids).is_empty() {
            // Nothing to check against, so the space scope is never read: a
            // closed-book turn must not pay for a query it will ignore.
            return Self::default();
        }

        let repository = ConversationRepository::new(container.db_pool().clone());
        let space_scope = match repository.retrieval_document_scope(conversation_id).await {
            Ok(Some((_, ids))) => ids,
            Ok(None) => {
                warn!(
                    conversation_id,
                    "Focus requested for a conversation with no resolvable scope; \
                     the turn reads nothing from the vault"
                );
                HashSet::new()
            }
            Err(error) => {
                warn!(
                    %error,
                    conversation_id,
                    "Could not resolve the space scope to check focus against; failing closed"
                );
                HashSet::new()
            }
        };

        Self::against_scope(requested_ids, &space_scope, closed_book, conversation_id)
    }

    /// The decision itself, with the scope already in hand.
    ///
    /// Separate from the read so the rule — intersect, never widen, fail
    /// closed, and lose to `closed_book` — can be tested without a database.
    pub(super) fn against_scope(
        requested_ids: Option<&Vec<String>>,
        space_scope: &HashSet<String>,
        closed_book: bool,
        conversation_id: &str,
    ) -> Self {
        if closed_book {
            return Self::default();
        }
        let requested = normalize(requested_ids);
        if requested.is_empty() {
            return Self::default();
        }
        let allowed: HashSet<String> = requested
            .iter()
            .filter(|id| space_scope.contains(*id))
            .cloned()
            .collect();
        if allowed.len() < requested.len() {
            info!(
                conversation_id,
                requested = requested.len(),
                kept = allowed.len(),
                "Focus documents outside this chat's space were dropped"
            );
        }
        Self {
            requested: requested.len(),
            allowed,
        }
    }

    /// Did the request ask for a focus at all?
    pub(super) fn is_requested(&self) -> bool {
        self.requested > 0
    }

    /// Does the focus leave the turn anything to read?
    pub(super) fn narrows(&self) -> bool {
        !self.allowed.is_empty()
    }

    /// The request named documents, and none of them are in this chat's space.
    /// The turn searches nothing from the vault and says why.
    pub(super) fn blocks_everything(&self) -> bool {
        self.is_requested() && self.allowed.is_empty()
    }

    /// What the trace reports: the count after the intersection, or nothing at
    /// all when no focus was asked for.
    pub(super) fn reported_count(&self) -> Option<usize> {
        self.is_requested().then_some(self.allowed.len())
    }

    /// Narrow an allow-list to the focus. Never widens it: the result is always
    /// a subset of what came in.
    pub(super) fn confine(&self, allowed: &mut HashSet<String>) {
        if !self.is_requested() {
            return;
        }
        allowed.retain(|id| self.allowed.contains(id));
    }

    /// Why the vault could not be searched, when the focus is what stopped it.
    pub(super) fn unavailable_reason(&self) -> &'static str {
        "the documents this chat is pinned to are not in its space"
    }
}

/// The distinct, non-blank ids a request named.
fn normalize(requested_ids: Option<&Vec<String>>) -> HashSet<String> {
    requested_ids
        .map(|ids| {
            ids.iter()
                .map(|id| id.trim().to_string())
                .filter(|id| !id.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(values: &[&str]) -> HashSet<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    fn named(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    fn focus(requested: &[&str], space: &[&str]) -> FocusScope {
        FocusScope::against_scope(Some(&named(requested)), &ids(space), false, "conv-1")
    }

    /// A document filed into another space is not this chat's to read, whoever
    /// asked for it.
    #[test]
    fn a_focus_id_from_another_space_is_ignored() {
        let focus = focus(&["mine", "theirs"], &["mine", "also_mine"]);
        let mut allowed = ids(&["mine", "also_mine"]);

        focus.confine(&mut allowed);

        assert_eq!(allowed, ids(&["mine"]));
        assert_eq!(focus.reported_count(), Some(1));
    }

    /// The whole invariant in one line: whatever the request says, the result
    /// is a subset of what the space already allowed.
    #[test]
    fn focus_never_widens_an_allow_list() {
        let focus = focus(&["a", "b"], &["a", "b", "c"]);
        let mut allowed = ids(&["a", "c"]);

        focus.confine(&mut allowed);

        assert_eq!(allowed, ids(&["a"]));
    }

    #[test]
    fn focus_narrows_the_turn_to_what_it_named() {
        let focus = focus(&["a"], &["a", "b", "c"]);
        let mut allowed = ids(&["a", "b", "c"]);

        focus.confine(&mut allowed);

        assert!(focus.narrows());
        assert_eq!(allowed, ids(&["a"]));
    }

    #[test]
    fn no_focus_leaves_the_allow_list_alone() {
        let focus = FocusScope::against_scope(None, &ids(&["a", "b"]), false, "conv-1");
        let mut allowed = ids(&["a", "b"]);

        focus.confine(&mut allowed);

        assert_eq!(allowed, ids(&["a", "b"]));
        assert!(!focus.is_requested());
        assert!(!focus.blocks_everything());
        assert_eq!(focus.reported_count(), None);
    }

    /// The fail-closed case. Naming only documents from another space must
    /// leave nothing readable, never the whole space.
    #[test]
    fn a_focus_that_survives_nothing_blocks_everything() {
        let focus = focus(&["theirs", "also_theirs"], &["mine"]);
        let mut allowed = ids(&["mine"]);

        focus.confine(&mut allowed);

        assert!(focus.blocks_everything());
        assert!(allowed.is_empty());
        assert_eq!(focus.reported_count(), Some(0));
    }

    /// `closed_book` has to beat every other preference, focus included: a
    /// journal synthesis carries its material with it and reads nothing.
    #[test]
    fn closed_book_wins_over_focus() {
        let focus =
            FocusScope::against_scope(Some(&named(&["a"])), &ids(&["a", "b"]), true, "conv-1");

        assert!(!focus.is_requested());
        assert!(!focus.narrows());
        assert!(!focus.blocks_everything());
        assert_eq!(focus.reported_count(), None);
    }

    #[test]
    fn blank_ids_are_not_a_focus() {
        let focus =
            FocusScope::against_scope(Some(&named(&["  ", ""])), &ids(&["a"]), false, "conv-1");

        assert!(!focus.is_requested());
    }
}
