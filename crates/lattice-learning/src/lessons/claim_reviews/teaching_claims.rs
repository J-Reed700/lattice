//! The claims a teaching note's sentence stands on, so the user can take the
//! sentence out or add a source for it from the note. A note names the live
//! claims whose sentence in the published section is exactly the note's. When
//! the section no longer holds the sentence and the user's removal of it
//! stands, the note shows as removed and names the claims an undo of that removal
//! takes back, none when the removal came with an edit of the whole section.
//! Draft lessons have none.

use std::collections::HashMap;

use lattice_core::domain::learning::claim_review::{
    span_of, Actor, ClaimLedgerEntry, ClaimState, Ledger,
};

use super::actions::in_section_body;
use super::published_edit::placed;
use super::published_restore::undoable_in_published;
use super::teaching_findings::{
    TeachingFindingDto, TeachingFindingSiteDto, TeachingFindingStateDto, TeachingResolutionDto,
};
use crate::content_verification::narrow;
use lattice_core::shared::text::same_text::whole_sentences;

/// Where the sentence of a waiting section note is in `body` when "Remove this
/// sentence" can take it out: it occurs once and is a whole sentence.
pub(super) fn removable_span(finding: &TeachingFindingDto, body: &str) -> Option<(usize, usize)> {
    if finding.state != TeachingFindingStateDto::Waiting
        || finding.site != TeachingFindingSiteDto::Section
        || finding.unit_index.is_none()
    {
        return None;
    }
    let span = span_of(body, finding.sentence.as_deref()?.trim())?;
    whole_sentences(body, span).then_some(span)
}

/// Fills in the claim ids of each section note that is about a sentence, says
/// whether its sentence can be removed, and shows a note removed while the
/// user's removal of its sentence stands.
pub(super) fn name_claims(
    ledger: &Ledger,
    bodies: &HashMap<usize, String>,
    findings: &mut [TeachingFindingDto],
) {
    for finding in findings {
        if let Some(body) = finding
            .unit_index
            .and_then(|unit| usize::try_from(unit).ok())
            .and_then(|unit| bodies.get(&unit))
        {
            finding.removable = removable_span(finding, body).is_some();
        }
        let (Some(sentence), Some(unit), TeachingFindingSiteDto::Section) = (
            finding.sentence.as_deref().map(str::trim),
            finding
                .unit_index
                .and_then(|unit| usize::try_from(unit).ok()),
            finding.site,
        ) else {
            continue;
        };
        let body = bodies.get(&unit).map_or("", String::as_str);
        let in_unit = |entry: &&ClaimLedgerEntry| {
            in_section_body(entry) && entry.location().unit_index == unit
        };
        let live: Vec<String> = ledger
            .claims
            .iter()
            .filter(in_unit)
            .filter(|entry| !entry.state().is_gone())
            .filter(|entry| {
                placed(entry, body)
                    .and_then(|(start, end)| body.get(start..end))
                    .is_some_and(|text| text.trim() == sentence)
            })
            .map(|entry| entry.claim_id().0.clone())
            .collect();
        if !live.is_empty() {
            finding.claim_ids = live;
            continue;
        }
        if body.contains(sentence) {
            continue;
        }
        let narrowed = |entry: &ClaimLedgerEntry| {
            let quote = &entry.location().quote;
            narrow(quote, None, entry.statement(), quote).0
        };
        let removals: Vec<(String, i64, bool)> = ledger
            .claims
            .iter()
            .filter(in_unit)
            .filter_map(|entry| match entry.state() {
                ClaimState::Removed {
                    at,
                    by: Actor::User,
                } if narrowed(entry).trim() == sentence => Some((
                    entry.claim_id().0.clone(),
                    *at,
                    undoable_in_published(entry),
                )),
                _ => None,
            })
            .collect();
        let Some(latest) = removals.iter().map(|(_, at, _)| *at).max() else {
            continue;
        };
        finding.state = TeachingFindingStateDto::Resolved;
        finding.resolution = Some(TeachingResolutionDto::Removed);
        finding.resolved_at = Some(latest);
        finding.claim_ids = removals
            .into_iter()
            .filter_map(|(id, _, undoable)| undoable.then_some(id))
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lessons::claim_reviews::teaching_findings::{
        TeachingFindingKindDto, TeachingPassageRoleDto,
    };
    use lattice_core::domain::learning::claim_review::{
        ClaimEvent, ClaimId, ClaimLocation, EventProvenance, Revision, UnitKind, UserAction,
    };

    const SENTENCE: &str = "The reader keeps every field name exactly as written.";
    const NEIGHBOUR: &str = "A second sentence stays in the section.";

    fn provenance(actor: Actor, at: i64) -> EventProvenance {
        EventProvenance {
            actor,
            at,
            job_id: None,
            app_version: "test".into(),
            checker_model: None,
        }
    }

    fn claim(id: &str, unit: usize, quote: &str, body: &str) -> ClaimLedgerEntry {
        ClaimLedgerEntry::extracted(
            ClaimId(id.into()),
            provenance(Actor::Machine, 1),
            ClaimLocation::locate(unit, UnitKind::Block, "body", quote, body),
            SENTENCE.into(),
        )
        .unwrap()
    }

    fn removed_by_user(mut entry: ClaimLedgerEntry, at: i64) -> (ClaimLedgerEntry, u32) {
        entry
            .append(
                provenance(Actor::Machine, at),
                ClaimEvent::TextRemoved {
                    text: SENTENCE.into(),
                    follows: None,
                    precedes: Some(NEIGHBOUR.into()),
                },
            )
            .unwrap();
        let ordinal = entry
            .append(
                provenance(Actor::User, at),
                ClaimEvent::UserAction {
                    action: UserAction::Remove,
                    note: None,
                },
            )
            .unwrap();
        (entry, ordinal)
    }

    fn removed_by_section_edit(mut entry: ClaimLedgerEntry, at: i64) -> ClaimLedgerEntry {
        entry
            .append(provenance(Actor::Machine, at), ClaimEvent::SectionEdited {})
            .unwrap();
        entry
            .append(
                provenance(Actor::User, at),
                ClaimEvent::UserAction {
                    action: UserAction::Remove,
                    note: None,
                },
            )
            .unwrap();
        entry
    }

    fn removed_without_a_record(mut entry: ClaimLedgerEntry, at: i64) -> ClaimLedgerEntry {
        entry
            .append(
                provenance(Actor::User, at),
                ClaimEvent::UserAction {
                    action: UserAction::Remove,
                    note: None,
                },
            )
            .unwrap();
        entry
    }

    fn undone(mut entry: ClaimLedgerEntry, ordinal: u32, at: i64) -> ClaimLedgerEntry {
        entry
            .append(
                provenance(Actor::User, at),
                ClaimEvent::Undone {
                    event_ordinal: ordinal,
                },
            )
            .unwrap();
        entry
    }

    fn removed_by_machine(mut entry: ClaimLedgerEntry) -> ClaimLedgerEntry {
        entry
            .append(
                provenance(Actor::Machine, 5),
                ClaimEvent::Superseded {
                    successor: None,
                    by: Actor::Machine,
                },
            )
            .unwrap();
        entry
    }

    fn ledger(claims: Vec<ClaimLedgerEntry>) -> Ledger {
        Ledger {
            lesson_id: "l1".into(),
            review: None,
            revision: Revision(1),
            claims,
        }
    }

    fn note(
        unit: u32,
        key: &str,
        state: TeachingFindingStateDto,
        resolution: Option<TeachingResolutionDto>,
        resolved_at: Option<i64>,
    ) -> TeachingFindingDto {
        TeachingFindingDto {
            finding_key: key.into(),
            kind: TeachingFindingKindDto::Quotation,
            site: TeachingFindingSiteDto::Section,
            unit_index: Some(unit),
            unit_title: "Section".into(),
            summary: "No source is cited for this sentence.".into(),
            passage: None,
            passage_role: TeachingPassageRoleDto::Lesson,
            sentence: Some(SENTENCE.into()),
            claim_ids: Vec::new(),
            removable: false,
            resolution,
            resolved_at,
            part_changed: false,
            repairs: 0,
            state,
        }
    }

    fn waiting(unit: u32, key: &str) -> TeachingFindingDto {
        note(unit, key, TeachingFindingStateDto::Waiting, None, None)
    }

    fn bodies(unit: usize, body: &str) -> HashMap<usize, String> {
        HashMap::from([(unit, body.to_owned())])
    }

    fn named(findings: &[TeachingFindingDto]) -> Vec<Vec<String>> {
        findings
            .iter()
            .map(|finding| {
                let mut ids = finding.claim_ids.clone();
                ids.sort();
                ids
            })
            .collect()
    }

    fn both() -> String {
        format!("{SENTENCE} {NEIGHBOUR}")
    }

    #[test]
    fn a_removed_sentence_shows_its_note_removed_with_the_claims_and_the_latest_removal_time() {
        let before = both();
        let (first, _) = removed_by_user(claim("a", 0, SENTENCE, &before), 100);
        let (second, _) = removed_by_user(claim("b", 0, SENTENCE, &before), 250);
        let bystander = claim("c", 0, NEIGHBOUR, &before);
        let mut findings = [waiting(0, "quotation:section-0")];
        name_claims(
            &ledger(vec![first, second, bystander]),
            &bodies(0, NEIGHBOUR),
            &mut findings,
        );
        assert_eq!(findings[0].state, TeachingFindingStateDto::Resolved);
        assert_eq!(findings[0].resolution, Some(TeachingResolutionDto::Removed));
        assert_eq!(findings[0].resolved_at, Some(250));
        assert_eq!(named(&findings), [vec!["a".to_owned(), "b".to_owned()]]);
    }

    #[test]
    fn a_claim_stored_with_a_two_sentence_quote_is_named_when_its_sentence_is_removed() {
        let before = both();
        let (wide, _) = removed_by_user(claim("wide", 0, &before, &before), 100);
        let mut findings = [waiting(0, "quotation:section-0")];
        name_claims(&ledger(vec![wide]), &bodies(0, NEIGHBOUR), &mut findings);
        assert_eq!(findings[0].state, TeachingFindingStateDto::Resolved);
        assert_eq!(findings[0].resolution, Some(TeachingResolutionDto::Removed));
        assert_eq!(named(&findings), [vec!["wide".to_owned()]]);
    }

    #[test]
    fn an_undone_removal_leaves_the_note_waiting_with_the_live_claims() {
        let before = both();
        let (entry, ordinal) = removed_by_user(claim("a", 0, SENTENCE, &before), 100);
        let entry = undone(entry, ordinal, 200);
        let mut findings = [waiting(0, "quotation:section-0")];
        name_claims(&ledger(vec![entry]), &bodies(0, &before), &mut findings);
        assert_eq!(findings[0].state, TeachingFindingStateDto::Waiting);
        assert_eq!(findings[0].resolution, None);
        assert_eq!(named(&findings), [vec!["a".to_owned()]]);
    }

    #[test]
    fn a_kept_or_source_added_note_shows_removed_while_its_removal_stands_and_as_it_was_after_the_undo(
    ) {
        let before = both();
        let stored = || {
            [
                note(
                    0,
                    "quotation:section-0",
                    TeachingFindingStateDto::Kept,
                    None,
                    None,
                ),
                note(
                    0,
                    "teaching:section-0",
                    TeachingFindingStateDto::Resolved,
                    Some(TeachingResolutionDto::SourceAdded),
                    Some(40),
                ),
            ]
        };
        let (entry, ordinal) = removed_by_user(claim("a", 0, SENTENCE, &before), 100);

        let mut findings = stored();
        name_claims(
            &ledger(vec![entry.clone()]),
            &bodies(0, NEIGHBOUR),
            &mut findings,
        );
        for finding in &findings {
            assert_eq!(finding.state, TeachingFindingStateDto::Resolved);
            assert_eq!(finding.resolution, Some(TeachingResolutionDto::Removed));
            assert_eq!(finding.resolved_at, Some(100));
            assert_eq!(finding.claim_ids, ["a"]);
        }

        let mut findings = stored();
        name_claims(
            &ledger(vec![undone(entry, ordinal, 200)]),
            &bodies(0, &before),
            &mut findings,
        );
        assert_eq!(findings[0].state, TeachingFindingStateDto::Kept);
        assert_eq!(findings[0].resolution, None);
        assert_eq!(findings[1].state, TeachingFindingStateDto::Resolved);
        assert_eq!(
            findings[1].resolution,
            Some(TeachingResolutionDto::SourceAdded)
        );
        assert_eq!(findings[1].resolved_at, Some(40));
        assert_eq!(
            named(&findings),
            [vec!["a".to_owned()], vec!["a".to_owned()]]
        );
    }

    #[test]
    fn two_notes_on_one_sentence_both_show_removed_when_it_is_removed_once() {
        let before = both();
        let (entry, _) = removed_by_user(claim("a", 0, SENTENCE, &before), 100);
        let mut findings = [
            waiting(0, "quotation:section-0"),
            waiting(0, "teaching:section-0"),
        ];
        name_claims(&ledger(vec![entry]), &bodies(0, NEIGHBOUR), &mut findings);
        for finding in &findings {
            assert_eq!(finding.state, TeachingFindingStateDto::Resolved);
            assert_eq!(finding.resolution, Some(TeachingResolutionDto::Removed));
        }
        assert_eq!(
            named(&findings),
            [vec!["a".to_owned()], vec!["a".to_owned()]]
        );
    }

    #[test]
    fn a_sentence_still_in_the_section_is_not_removed() {
        let before = both();
        let (entry, _) = removed_by_user(claim("a", 0, SENTENCE, &before), 100);
        let mut findings = [waiting(0, "quotation:section-0")];
        name_claims(&ledger(vec![entry]), &bodies(0, &before), &mut findings);
        assert_eq!(findings[0].state, TeachingFindingStateDto::Waiting);
        assert_eq!(findings[0].resolution, None);
        assert_eq!(named(&findings), [Vec::<String>::new()]);
    }

    #[test]
    fn a_claim_the_machine_removed_does_not_show_the_note_removed() {
        let before = both();
        let entry = removed_by_machine(claim("a", 0, SENTENCE, &before));
        let mut findings = [waiting(0, "quotation:section-0")];
        name_claims(&ledger(vec![entry]), &bodies(0, NEIGHBOUR), &mut findings);
        assert_eq!(findings[0].state, TeachingFindingStateDto::Waiting);
        assert_eq!(findings[0].resolution, None);
        assert_eq!(named(&findings), [Vec::<String>::new()]);
    }

    #[test]
    fn a_removal_in_another_section_does_not_show_the_note_removed() {
        let before = both();
        let (entry, _) = removed_by_user(claim("a", 1, SENTENCE, &before), 100);
        let mut findings = [waiting(0, "quotation:section-0")];
        name_claims(&ledger(vec![entry]), &bodies(0, NEIGHBOUR), &mut findings);
        assert_eq!(findings[0].state, TeachingFindingStateDto::Waiting);
        assert_eq!(named(&findings), [Vec::<String>::new()]);
    }

    #[test]
    fn a_sentence_a_section_edit_deleted_shows_removed_with_its_latest_removal_time_and_no_claim_ids(
    ) {
        let before = both();
        let first = removed_by_section_edit(claim("a", 0, SENTENCE, &before), 100);
        let second = removed_by_section_edit(claim("b", 0, SENTENCE, &before), 250);
        let mut findings = [waiting(0, "quotation:section-0")];
        name_claims(
            &ledger(vec![first, second]),
            &bodies(0, NEIGHBOUR),
            &mut findings,
        );
        assert_eq!(findings[0].state, TeachingFindingStateDto::Resolved);
        assert_eq!(findings[0].resolution, Some(TeachingResolutionDto::Removed));
        assert_eq!(findings[0].resolved_at, Some(250));
        assert_eq!(named(&findings), [Vec::<String>::new()]);
    }

    #[test]
    fn a_removal_with_no_recorded_text_shows_removed_with_no_claim_ids() {
        let before = both();
        let entry = removed_without_a_record(claim("a", 0, SENTENCE, &before), 100);
        let mut findings = [waiting(0, "quotation:section-0")];
        name_claims(&ledger(vec![entry]), &bodies(0, NEIGHBOUR), &mut findings);
        assert_eq!(findings[0].state, TeachingFindingStateDto::Resolved);
        assert_eq!(findings[0].resolution, Some(TeachingResolutionDto::Removed));
        assert_eq!(findings[0].resolved_at, Some(100));
        assert_eq!(named(&findings), [Vec::<String>::new()]);
    }

    fn about(mut finding: TeachingFindingDto, sentence: &str) -> TeachingFindingDto {
        finding.sentence = Some(sentence.into());
        finding
    }

    fn removable_after(
        finding: TeachingFindingDto,
        claims: Vec<ClaimLedgerEntry>,
        body: &str,
    ) -> bool {
        let mut findings = [finding];
        name_claims(&ledger(claims), &bodies(0, body), &mut findings);
        findings[0].removable
    }

    #[test]
    fn a_whole_sentence_no_claim_stands_on_is_removable_and_names_no_claim() {
        let mut findings = [waiting(0, "quotation:section-0")];
        name_claims(&ledger(Vec::new()), &bodies(0, &both()), &mut findings);
        assert!(findings[0].removable);
        assert_eq!(named(&findings), [Vec::<String>::new()]);
    }

    #[test]
    fn a_sentence_with_an_exact_claim_is_removable_and_still_names_that_claim() {
        let before = both();
        let mut findings = [waiting(0, "quotation:section-0")];
        name_claims(
            &ledger(vec![claim("a", 0, SENTENCE, &before)]),
            &bodies(0, &before),
            &mut findings,
        );
        assert!(findings[0].removable);
        assert_eq!(named(&findings), [vec!["a".to_owned()]]);
    }

    #[test]
    fn a_fragment_of_a_sentence_is_not_removable() {
        let fragment = about(
            waiting(0, "quotation:section-0"),
            "keeps every field name exactly",
        );
        assert!(!removable_after(fragment, Vec::new(), &both()));
    }

    #[test]
    fn a_part_of_a_segmenter_sentence_is_not_removable() {
        let after_an_abbreviation = about(waiting(0, "quotation:section-0"), "Go.");
        assert!(!removable_after(
            after_an_abbreviation,
            Vec::new(),
            "Pick Rust vs. Go. Both are fast."
        ));
        let after_a_colon = about(waiting(0, "quotation:section-0"), "Rust is fast.");
        assert!(!removable_after(
            after_a_colon,
            Vec::new(),
            "Note: Rust is fast."
        ));
    }

    #[test]
    fn a_sentence_that_occurs_twice_in_the_section_is_not_removable() {
        let twice = format!("{SENTENCE} {NEIGHBOUR} {SENTENCE}");
        assert!(!removable_after(
            waiting(0, "quotation:section-0"),
            Vec::new(),
            &twice
        ));
    }

    #[test]
    fn a_sentence_the_section_does_not_hold_is_not_removable() {
        assert!(!removable_after(
            waiting(0, "quotation:section-0"),
            Vec::new(),
            NEIGHBOUR
        ));
    }

    #[test]
    fn a_note_with_no_sentence_or_a_blank_one_is_not_removable() {
        let mut none = waiting(0, "teaching:section-0");
        none.sentence = None;
        assert!(!removable_after(none, Vec::new(), &both()));
        let blank = about(waiting(0, "teaching:section-0"), "  ");
        assert!(!removable_after(blank, Vec::new(), &both()));
    }

    #[test]
    fn a_question_note_and_a_lesson_note_are_not_removable() {
        let mut question = waiting(0, "teaching:question-0");
        question.site = TeachingFindingSiteDto::Question;
        assert!(!removable_after(question, Vec::new(), &both()));
        let mut lesson = waiting(0, "structure:lesson");
        lesson.site = TeachingFindingSiteDto::Lesson;
        lesson.unit_index = None;
        assert!(!removable_after(lesson, Vec::new(), &both()));
    }

    #[test]
    fn a_kept_note_is_not_removable() {
        let kept = note(
            0,
            "quotation:section-0",
            TeachingFindingStateDto::Kept,
            None,
            None,
        );
        assert!(!removable_after(kept, Vec::new(), &both()));
    }
}
