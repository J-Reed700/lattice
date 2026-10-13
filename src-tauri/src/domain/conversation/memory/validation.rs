//! Deterministic validation of an untrusted memory patch (§6.2).
use super::{
    EvidencePurpose, EvidenceSpan, MemoryId, MemoryItem, MemoryKind, MemoryPatch, MemoryState,
    MemoryValidationError, ProposedEvidence, SourceMessage, SourceRole, ValidatedAddition,
    ValidatedPatch, ValidatedTransition, ValidatedTransitionTarget, MAX_EVIDENCE_PER_ITEM,
    MAX_LABEL_CHARS, MAX_OPERATIONS_PER_RESPONSE, MAX_PROPOSAL_BYTES, MAX_QUOTE_BYTES,
    MEMORY_SCHEMA_VERSION,
};
use std::collections::{HashMap, HashSet};

/// Sources an extraction pass was allowed to quote, indexed for lookup.
pub struct SourceIndex<'a> {
    by_id: HashMap<&'a str, &'a SourceMessage>,
}

impl<'a> SourceIndex<'a> {
    pub fn new(messages: &'a [SourceMessage]) -> Self {
        Self {
            by_id: messages
                .iter()
                .map(|message| (message.id.as_str(), message))
                .collect(),
        }
    }

    pub fn get(&self, id: &str) -> Option<&'a SourceMessage> {
        self.by_id.get(id).copied()
    }
}

/// Byte offsets of the `occurrence`-th exact match of `needle` in `haystack`.
///
/// Exact substring search over bytes, then a boundary check. Searching bytes
/// rather than characters is deliberate: it is the only way to be sure the
/// offsets we store are the offsets the text actually has.
fn locate_quote(
    haystack: &str,
    needle: &str,
    occurrence: Option<u32>,
    message_id: &str,
) -> std::result::Result<(u32, u32), MemoryValidationError> {
    if needle.is_empty() {
        return Err(MemoryValidationError::QuoteNotFound(message_id.to_string()));
    }
    let mut starts = Vec::new();
    let mut from = 0usize;
    while let Some(found) = haystack[from..].find(needle) {
        let absolute = from + found;
        starts.push(absolute);
        // Overlapping matches are still distinct occurrences, so advance by one
        // character rather than by the needle's length.
        from = absolute
            + haystack[absolute..]
                .chars()
                .next()
                .map_or(1, char::len_utf8);
        if from >= haystack.len() {
            break;
        }
    }
    if starts.is_empty() {
        return Err(MemoryValidationError::QuoteNotFound(message_id.to_string()));
    }
    let index = match occurrence {
        Some(index) => index as usize,
        None if starts.len() == 1 => 0,
        None => {
            return Err(MemoryValidationError::AmbiguousOccurrence(
                message_id.to_string(),
                starts.len(),
            ))
        }
    };
    let start = *starts.get(index).ok_or_else(|| {
        MemoryValidationError::OccurrenceNotFound(
            message_id.to_string(),
            occurrence.unwrap_or_default(),
        )
    })?;
    let end = start + needle.len();
    // `find` returns character-aligned offsets for valid UTF-8, but the check is
    // cheap and this is the invariant every later read depends on.
    if !haystack.is_char_boundary(start) || !haystack.is_char_boundary(end) {
        return Err(MemoryValidationError::QuoteNotFound(message_id.to_string()));
    }
    Ok((start as u32, end as u32))
}

/// Turn one proposed quotation into a span, or say exactly why it cannot be one.
fn validate_evidence(
    proposed: &ProposedEvidence,
    sources: &SourceIndex<'_>,
    owner: &str,
) -> std::result::Result<EvidenceSpan, MemoryValidationError> {
    if proposed.quote.len() > MAX_QUOTE_BYTES {
        return Err(MemoryValidationError::QuoteTooLong(
            owner.to_string(),
            proposed.quote.len(),
        ));
    }
    let purpose: EvidencePurpose = proposed
        .purpose
        .parse()
        .map_err(|_| MemoryValidationError::UnknownEnum("purpose", proposed.purpose.clone()))?;

    let message = sources
        .get(&proposed.message_id)
        .ok_or_else(|| MemoryValidationError::UnknownSource(proposed.message_id.clone()))?;

    if !message.is_eligible_source() {
        return Err(MemoryValidationError::IneligibleSource(
            proposed.message_id.clone(),
        ));
    }
    // The snapshot's digest is what the extractor was shown. If the live row no
    // longer matches it, the text moved under us mid-job.
    if !message.digest_matches() {
        return Err(MemoryValidationError::StaleSource(
            proposed.message_id.clone(),
        ));
    }

    let (start_byte, end_byte) = locate_quote(
        &message.content,
        &proposed.quote,
        proposed.occurrence,
        &proposed.message_id,
    )?;

    Ok(EvidenceSpan {
        message_id: message.id.clone(),
        sequence: message.sequence,
        role: message.role,
        start_byte,
        end_byte,
        content_digest: message.content_digest.clone(),
        purpose,
    })
}

/// Everything §6.2 requires, applied to one parsed patch.
///
/// The patch is accepted or rejected as a unit. There is no partial application:
/// a half-applied patch is a ledger that claims a coverage it does not have.
pub fn validate_patch(
    patch: &MemoryPatch,
    sources: &SourceIndex<'_>,
    active_items: &[MemoryItem],
    submitted_segment_ids: &[String],
) -> std::result::Result<ValidatedPatch, MemoryValidationError> {
    if patch.schema_version != MEMORY_SCHEMA_VERSION {
        return Err(MemoryValidationError::UnsupportedSchemaVersion(
            patch.schema_version,
        ));
    }
    let operations = patch.add.len() + patch.transitions.len();
    if operations > MAX_OPERATIONS_PER_RESPONSE {
        return Err(MemoryValidationError::TooManyOperations(operations));
    }

    // --- additions -------------------------------------------------------
    let mut seen_candidates: HashSet<&str> = HashSet::new();
    let mut additions = Vec::with_capacity(patch.add.len());
    for proposed in &patch.add {
        if !seen_candidates.insert(proposed.candidate_id.as_str()) {
            return Err(MemoryValidationError::DuplicateCandidate(
                proposed.candidate_id.clone(),
            ));
        }
        if proposed.label.chars().count() > MAX_LABEL_CHARS {
            return Err(MemoryValidationError::LabelTooLong(
                proposed.candidate_id.clone(),
            ));
        }
        if proposed.evidence.is_empty() {
            return Err(MemoryValidationError::NoEvidence(
                proposed.candidate_id.clone(),
            ));
        }
        if proposed.evidence.len() > MAX_EVIDENCE_PER_ITEM {
            return Err(MemoryValidationError::TooMuchEvidence(
                proposed.candidate_id.clone(),
                proposed.evidence.len(),
            ));
        }
        let kind: MemoryKind = proposed
            .kind
            .parse()
            .map_err(|_| MemoryValidationError::UnknownEnum("kind", proposed.kind.clone()))?;
        // The host owns this kind. A model claiming to have found an unresolved
        // change is a model deciding its own uncertainty is settled policy.
        if kind == MemoryKind::UnresolvedChange {
            return Err(MemoryValidationError::UnknownEnum(
                "kind",
                proposed.kind.clone(),
            ));
        }

        let mut evidence = Vec::with_capacity(proposed.evidence.len());
        for item in &proposed.evidence {
            let span = validate_evidence(item, sources, &proposed.candidate_id)?;
            match span.purpose {
                // An established fact may stand on a sourced assistant passage:
                // the passage names where the claim came from, so the quote
                // itself is the authority. Every other kind still needs the
                // user's own words.
                EvidencePurpose::Assertion
                    if span.role != SourceRole::User && kind != MemoryKind::EstablishedFact =>
                {
                    return Err(MemoryValidationError::NonUserAuthority(
                        proposed.candidate_id.clone(),
                    ));
                }
                EvidencePurpose::Assertion | EvidencePurpose::Antecedent => evidence.push(span),
                EvidencePurpose::Transition => {
                    return Err(MemoryValidationError::InvalidEvidencePurpose {
                        item: proposed.candidate_id.clone(),
                        expected: "assertion or antecedent",
                        actual: span.purpose.as_str().to_string(),
                    });
                }
            }
        }

        let addition = ValidatedAddition {
            candidate_id: proposed.candidate_id.clone(),
            kind,
            label: proposed.label.trim().to_string(),
            evidence,
        };
        // Rule 7: assistant text, tool output and quoted third parties can
        // explain a user memory but cannot create one. Established facts are
        // the carve-out: they are created by the sourced statement, whoever
        // brought it into the conversation.
        if kind == MemoryKind::EstablishedFact {
            if !addition.has_assertion() {
                return Err(MemoryValidationError::NoAssertion(
                    proposed.candidate_id.clone(),
                ));
            }
        } else if !addition.has_user_assertion() {
            return Err(MemoryValidationError::NoUserAssertion(
                proposed.candidate_id.clone(),
            ));
        }
        if let Some(existing) = additions.iter().find(|existing: &&ValidatedAddition| {
            existing.kind == addition.kind
                && existing.label.eq_ignore_ascii_case(&addition.label)
                && existing.evidence == addition.evidence
        }) {
            return Err(MemoryValidationError::DuplicateAddition(
                existing.candidate_id.clone(),
                addition.candidate_id.clone(),
            ));
        }
        additions.push(addition);
    }

    // --- transitions -----------------------------------------------------
    let active_by_id: HashMap<&str, &MemoryItem> = active_items
        .iter()
        .filter(|item| item.state == MemoryState::Active)
        .map(|item| (item.id.as_str(), item))
        .collect();
    let candidates_by_id: HashMap<&str, &ValidatedAddition> = additions
        .iter()
        .map(|addition| (addition.candidate_id.as_str(), addition))
        .collect();

    let mut targeted: HashSet<&str> = HashSet::new();
    let mut transitions = Vec::with_capacity(patch.transitions.len());
    for proposed in &patch.transitions {
        let (target, target_changed_at, target_kind) =
            if let Some(item) = active_by_id.get(proposed.item_id.as_str()) {
                (
                    ValidatedTransitionTarget::Existing(item.id.clone()),
                    item.changed_at_sequence,
                    item.kind,
                )
            } else if let Some(candidate) = candidates_by_id.get(proposed.item_id.as_str()) {
                (
                    ValidatedTransitionTarget::Candidate(candidate.candidate_id.clone()),
                    candidate.changed_at_sequence(),
                    candidate.kind,
                )
            } else {
                return Err(MemoryValidationError::UnknownTransitionTarget(
                    proposed.item_id.clone(),
                ));
            };
        if !targeted.insert(proposed.item_id.as_str()) {
            return Err(MemoryValidationError::ConflictingTransitions(
                proposed.item_id.clone(),
            ));
        }
        let state: MemoryState = proposed
            .state
            .parse()
            .map_err(|_| MemoryValidationError::UnknownEnum("state", proposed.state.clone()))?;
        if state == MemoryState::Active {
            return Err(MemoryValidationError::UnknownEnum(
                "state",
                proposed.state.clone(),
            ));
        }
        if state == MemoryState::Superseded && proposed.replacement_candidate_id.is_none() {
            return Err(MemoryValidationError::SupersessionWithoutReplacement(
                proposed.item_id.clone(),
            ));
        }
        if proposed.evidence.is_empty() {
            return Err(MemoryValidationError::NoEvidence(proposed.item_id.clone()));
        }
        if proposed.evidence.len() > MAX_EVIDENCE_PER_ITEM {
            return Err(MemoryValidationError::TooMuchEvidence(
                proposed.item_id.clone(),
                proposed.evidence.len(),
            ));
        }
        if let Some(replacement) = &proposed.replacement_candidate_id {
            if !candidates_by_id.contains_key(replacement.as_str()) {
                return Err(MemoryValidationError::UnknownReplacement(
                    proposed.item_id.clone(),
                    replacement.clone(),
                ));
            }
            if target.as_str() == replacement {
                return Err(MemoryValidationError::SelfSupersession(
                    proposed.item_id.clone(),
                ));
            }
        }

        let mut evidence = Vec::with_capacity(proposed.evidence.len());
        for item in &proposed.evidence {
            let span = validate_evidence(item, sources, &proposed.item_id)?;
            if span.purpose != EvidencePurpose::Transition {
                return Err(MemoryValidationError::InvalidEvidencePurpose {
                    item: proposed.item_id.clone(),
                    expected: "transition",
                    actual: span.purpose.as_str().to_string(),
                });
            }
            if span.role != SourceRole::User && target_kind != MemoryKind::EstablishedFact {
                return Err(MemoryValidationError::NonUserAuthority(
                    proposed.item_id.clone(),
                ));
            }
            evidence.push(span);
        }
        // Rule 9: a correction has to come *after* what it corrects. Without
        // this, wording similar to an earlier message could retire a later one.
        let latest = evidence
            .iter()
            .map(|span| span.sequence)
            .max()
            .unwrap_or_default();
        if latest <= target_changed_at {
            return Err(MemoryValidationError::TransitionNotLater(
                proposed.item_id.clone(),
            ));
        }
        // A user retirement needs a user source, for the same reason an
        // assertion does — unless the target is an established fact, which a
        // later sourced passage may correct whoever brought it.
        transitions.push(ValidatedTransition {
            target,
            state,
            replacement_candidate_id: proposed.replacement_candidate_id.clone(),
            evidence,
        });
    }

    check_supersession_acyclic(&transitions, active_items)?;

    // --- coverage --------------------------------------------------------
    // Rule 12. This catches a truncated or skipped response. It cannot catch a
    // model that read every segment and noticed nothing in them.
    if !submitted_segment_ids.is_empty() {
        let reported: HashSet<&str> = patch
            .processed_segment_ids
            .iter()
            .map(String::as_str)
            .collect();
        let missing = submitted_segment_ids
            .iter()
            .filter(|id| !reported.contains(id.as_str()))
            .count();
        if missing > 0 {
            return Err(MemoryValidationError::IncompleteCoverage {
                reported: submitted_segment_ids.len() - missing,
                expected: submitted_segment_ids.len(),
            });
        }
    }

    Ok(ValidatedPatch {
        additions,
        transitions,
        processed_segment_ids: patch.processed_segment_ids.clone(),
    })
}

/// Reject self-replacement and any cycle through the existing chain.
///
/// New additions have no durable ID yet, so a proposed replacement can only
/// close a loop by pointing back through items that already exist — which is
/// what this walks.
fn check_supersession_acyclic(
    transitions: &[ValidatedTransition],
    active_items: &[MemoryItem],
) -> std::result::Result<(), MemoryValidationError> {
    let existing: HashMap<&str, Option<&MemoryId>> = active_items
        .iter()
        .map(|item| (item.id.as_str(), item.superseded_by.as_ref()))
        .collect();

    let proposed: HashMap<&str, &str> = transitions
        .iter()
        .filter_map(|transition| {
            transition
                .replacement_candidate_id
                .as_deref()
                .map(|replacement| (transition.target.as_str(), replacement))
        })
        .collect();

    for transition in transitions {
        // Walk both the proposed same-response edges and the stored durable
        // chain. Candidate-to-candidate transitions make cycles possible
        // before either candidate has a durable ID, so both graphs matter.
        let mut seen = HashSet::new();
        let mut cursor = Some(transition.target.as_str());
        while let Some(current) = cursor {
            if !seen.insert(current) {
                return Err(MemoryValidationError::SupersessionCycle(
                    transition.target.as_str().to_string(),
                ));
            }
            cursor = proposed.get(current).copied().or_else(|| {
                existing
                    .get(current)
                    .and_then(|next| next.map(MemoryId::as_str))
            });
        }
    }
    Ok(())
}

/// Parse a model response into a patch, tolerating exactly one Markdown fence.
///
/// No other repair: trimming a malformed response until it parses is how a
/// truncated list of constraints becomes an accepted one (§6.2 rule 2).
pub fn parse_patch(raw: &str) -> std::result::Result<MemoryPatch, MemoryValidationError> {
    if raw.len() > MAX_PROPOSAL_BYTES {
        return Err(MemoryValidationError::ResponseTooLarge);
    }
    let trimmed = raw.trim();
    let body = match trimmed.strip_prefix("```") {
        Some(rest) => {
            // ```json\n...\n``` — drop the language tag and the closing fence.
            let rest = rest.split_once('\n').map_or(rest, |(_, body)| body);
            rest.trim_end()
                .strip_suffix("```")
                .ok_or_else(|| MemoryValidationError::Malformed("unterminated code fence".into()))?
        }
        None => trimmed,
    };
    serde_json::from_str(body.trim())
        .map_err(|error| MemoryValidationError::Malformed(error.to_string()))
}
