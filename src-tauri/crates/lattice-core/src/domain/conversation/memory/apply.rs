//! Applying a validated patch to the memory ledger (§8).
use super::{
    MemoryId, MemoryItem, MemoryKind, MemoryReview, MemoryState, ValidatedPatch,
    ValidatedTransitionTarget, MAX_EVIDENCE_PER_ITEM, MAX_RELATED_ITEMS,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The complete, ordered set of changes to commit in one transaction.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryCommit {
    /// Brand new items, IDs already assigned.
    pub inserts: Vec<MemoryItem>,
    /// Existing items whose state/`superseded_by` changes, plus any transition
    /// evidence appended to them.
    pub updates: Vec<MemoryItem>,
    /// New working summary, when this commit also rewrites it.
    pub summary: Option<String>,
    /// Watermark this commit advances to.
    pub processed_through_sequence: i64,
}

impl MemoryCommit {
    pub fn is_empty(&self) -> bool {
        self.inserts.is_empty() && self.updates.is_empty() && self.summary.is_none()
    }
}

/// Verdict from the separate semantic review step (§6.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticVerdict {
    /// The evidence supports the proposed reading.
    Supported,
    /// The spans are real but the meaning is unsettled.
    Ambiguous,
    /// The proposal misreads its own evidence.
    Unsupported,
}

/// Fold a validated patch and its reviews into a committable change set.
///
/// The rules this encodes, all of them from §8:
///
/// * An item the model did not mention stays exactly as it was. Omission is
///   never deletion (invariant 6).
/// * An `Unsupported` addition is dropped; an `Ambiguous` one is kept but marked
///   [`MemoryReview::Ambiguous`], because throwing away a requirement we could
///   not interpret is worse than carrying it with its raw passages.
/// * An `Ambiguous` *transition* does not retire anything. It produces a
///   host-owned [`MemoryKind::UnresolvedChange`] holding both the old item's
///   evidence and the proposed transition evidence, so the conflict is visible
///   instead of resolved by guess. An `Unsupported` transition is ignored; its
///   source remains covered by the working summary without letting the rejected
///   operation stall other supported memory from the same batch.
pub fn apply_patch(
    conversation_id: &str,
    patch: &ValidatedPatch,
    active_items: &[MemoryItem],
    addition_verdicts: &HashMap<String, SemanticVerdict>,
    transition_verdicts: &HashMap<String, SemanticVerdict>,
    next_revision: i64,
    processed_through_sequence: i64,
) -> MemoryCommit {
    let now = Utc::now();
    let mut commit = MemoryCommit {
        processed_through_sequence,
        ..Default::default()
    };

    // Assign durable IDs first: a transition may name an addition as its
    // replacement, and that link has to point at the ID we actually persist.
    let mut assigned: HashMap<&str, MemoryId> = HashMap::new();
    for addition in &patch.additions {
        let verdict = addition_verdicts
            .get(&addition.candidate_id)
            .copied()
            .unwrap_or(SemanticVerdict::Supported);
        if verdict == SemanticVerdict::Unsupported {
            continue;
        }
        let id = MemoryId::new();
        assigned.insert(addition.candidate_id.as_str(), id.clone());
        commit.inserts.push(MemoryItem {
            id,
            conversation_id: conversation_id.to_string(),
            kind: addition.kind,
            state: MemoryState::Active,
            label: addition.label.clone(),
            evidence: addition.evidence.clone(),
            created_at_sequence: addition.created_at_sequence(),
            changed_at_sequence: addition.changed_at_sequence(),
            superseded_by: None,
            revision: next_revision,
            review: match verdict {
                SemanticVerdict::Supported => MemoryReview::Supported,
                _ => MemoryReview::Ambiguous,
            },
            related_item_ids: Vec::new(),
            created_at: now,
            updated_at: now,
        });
    }

    let by_id: HashMap<&str, &MemoryItem> = active_items
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect();

    for transition in &patch.transitions {
        // A replacement transition is one atomic claim. If semantic review
        // rejected the proposed replacement, retiring the old item would leave
        // the ledger with neither side active. Ignore the transition instead.
        if transition
            .replacement_candidate_id
            .as_deref()
            .is_some_and(|candidate| !assigned.contains_key(candidate))
        {
            continue;
        }
        let target_id = match &transition.target {
            ValidatedTransitionTarget::Existing(id) => id.clone(),
            ValidatedTransitionTarget::Candidate(candidate) => {
                let Some(id) = assigned.get(candidate.as_str()) else {
                    // The target addition was rejected by semantic review, so
                    // there is no item whose state can be changed.
                    continue;
                };
                id.clone()
            }
        };
        let existing = match &transition.target {
            ValidatedTransitionTarget::Existing(id) => {
                let Some(item) = by_id.get(id.as_str()) else {
                    continue;
                };
                (*item).clone()
            }
            ValidatedTransitionTarget::Candidate(_) => {
                let Some(item) = commit.inserts.iter().find(|item| item.id == target_id) else {
                    continue;
                };
                item.clone()
            }
        };
        let verdict = transition_verdicts
            .get(transition.target.as_str())
            .copied()
            .unwrap_or(SemanticVerdict::Ambiguous);

        if verdict == SemanticVerdict::Supported {
            let mut updated = existing.clone();
            updated.state = transition.state;
            updated.superseded_by = transition
                .replacement_candidate_id
                .as_deref()
                .and_then(|candidate| assigned.get(candidate).cloned());
            updated.changed_at_sequence = transition
                .evidence
                .iter()
                .map(|span| span.sequence)
                .max()
                .unwrap_or(existing.changed_at_sequence)
                .max(existing.changed_at_sequence);
            // The transition passage joins the item's own evidence, so the
            // history view can show what retired it without a second lookup.
            updated.evidence.extend(transition.evidence.iter().cloned());
            updated.evidence.truncate(MAX_EVIDENCE_PER_ITEM * 2);
            updated.revision = next_revision;
            updated.updated_at = now;
            match transition.target {
                ValidatedTransitionTarget::Existing(_) => commit.updates.push(updated),
                ValidatedTransitionTarget::Candidate(_) => {
                    if let Some(pending) =
                        commit.inserts.iter_mut().find(|item| item.id == target_id)
                    {
                        *pending = updated;
                    }
                }
            }
            continue;
        }

        if verdict == SemanticVerdict::Unsupported {
            continue;
        }

        // The evidence is real but its meaning is ambiguous: record the
        // conflict without changing the existing item.
        let mut evidence = existing.evidence.clone();
        evidence.extend(transition.evidence.iter().cloned());
        evidence.truncate(MAX_EVIDENCE_PER_ITEM * 2);
        let mut related = vec![existing.id.clone()];
        if let Some(replacement) = transition
            .replacement_candidate_id
            .as_deref()
            .and_then(|candidate| assigned.get(candidate).cloned())
        {
            related.push(replacement);
        }
        related.truncate(MAX_RELATED_ITEMS);

        let changed_at = evidence
            .iter()
            .map(|span| span.sequence)
            .max()
            .unwrap_or(existing.changed_at_sequence);
        commit.inserts.push(MemoryItem {
            id: MemoryId::new(),
            conversation_id: conversation_id.to_string(),
            kind: MemoryKind::UnresolvedChange,
            state: MemoryState::Active,
            label: format!("Unresolved change to: {}", existing.label),
            evidence,
            created_at_sequence: existing.created_at_sequence,
            changed_at_sequence: changed_at,
            superseded_by: None,
            revision: next_revision,
            review: MemoryReview::Ambiguous,
            related_item_ids: related,
            created_at: now,
            updated_at: now,
        });
        // The original item is untouched. A restriction stays in force until a
        // correction we actually understood retires it.
    }

    commit
}
