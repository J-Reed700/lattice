#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
use super::standing::{Round, Standing};
use crate::review_findings::{site_sha256, FindingSite, Held, ReviewFinding};
use crate::teaching_review::{Judged, Reported, Verdict};
use serde_json::{json, Value};

pub(super) fn lesson(bodies: &[&str]) -> Value {
    let blocks: Vec<Value> = bodies
        .iter()
        .map(|body| json!({"kind":"explanation","title":"Part","body":body,"rubric":[],"sourceIndex":0,"quote":"q"}))
        .collect();
    json!({"blocks":blocks,"questions":[]})
}

pub(super) fn report(site: Option<FindingSite>, note: &str, passage: Option<&str>) -> Reported {
    Reported {
        site,
        note: note.to_owned(),
        summary: format!("About: {note}"),
        passage: passage.map(str::to_owned),
        passage_id: None,
        continues: None,
    }
}

fn verdict(finding: &ReviewFinding, present: bool, note: &str) -> Verdict {
    Verdict {
        id: finding.id(),
        present,
        note: note.to_owned(),
    }
}

/// The reviewer's answer to the next round, and what that gives.
fn answer(
    standing: &mut Standing,
    candidate: &Value,
    verdicts: Vec<Verdict>,
    reported: Vec<Reported>,
) -> (Round, Vec<ReviewFinding>) {
    let round = standing.round(candidate).unwrap();
    let fresh = standing.judged(candidate, &round, Judged { verdicts, reported });
    (round, fresh)
}

/// A standing that has reviewed `candidate` and keeps what it reported.
pub(super) fn reviewed(
    candidate: &Value,
    reported: Vec<Reported>,
) -> (Standing, Vec<ReviewFinding>) {
    let mut standing = Standing::new(&Held::default());
    let (_, fresh) = answer(&mut standing, candidate, Vec::new(), reported);
    standing.admit(candidate, fresh.clone());
    (standing, fresh)
}

pub(super) const FIRST: &str = "The first part states one plain fact.";
pub(super) const SECOND: &str = "The second part states another.";

#[test]
fn a_first_review_covers_every_section_and_asks_about_nothing() {
    let standing = Standing::new(&Held::default());
    let round = standing.round(&lesson(&[FIRST, SECOND])).unwrap();
    assert_eq!(round.sections, vec![0, 1]);
    assert!(round.whole);
    assert!(round.first);
    assert!(round.asked.is_empty());
    assert!(round.known.is_empty());
    assert!(round.scope().asked.is_none());
}

#[test]
fn material_the_reviewer_has_seen_gives_no_round() {
    let candidate = lesson(&[FIRST, SECOND]);
    let (standing, _) = reviewed(&candidate, Vec::new());
    assert!(standing.round(&candidate).is_none());
}

#[test]
fn a_changed_section_is_the_only_one_in_the_round_and_its_finding_is_asked_with_its_passage() {
    let candidate = lesson(&[FIRST, SECOND]);
    let (standing, fresh) = reviewed(
        &candidate,
        vec![report(Some(FindingSite::Section(0)), "Wrong.", Some(FIRST))],
    );
    assert_eq!(fresh[0].id().as_str(), "reviewer/1");
    let changed = lesson(&[
        "The first part states one plain fact. It adds a detail.",
        SECOND,
    ]);
    let round = standing.round(&changed).unwrap();
    assert_eq!(round.sections, vec![0]);
    assert!(!round.whole);
    let asked = round.asked;
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].id, fresh[0].id());
    assert_eq!(asked[0].site, FindingSite::Section(0));
    assert_eq!(asked[0].passage_id.as_deref(), Some("section-0-passage-0"));
    assert!(round.known.is_empty());
}

#[test]
fn a_finding_with_a_second_part_is_asked_again_when_only_the_second_part_changed() {
    let candidate = lesson(&[FIRST, SECOND]);
    let (_, fresh) = reviewed(
        &candidate,
        vec![report(Some(FindingSite::Section(0)), "Clash.", Some(FIRST))],
    );
    let both = fresh[0].clone().with_also(FindingSite::Section(1));
    let mut standing = Standing::new(&Held::default());
    standing.admit(&candidate, vec![both.clone()]);
    let round = standing
        .round(&lesson(&[FIRST, "The second part now states a third."]))
        .unwrap();
    assert_eq!(round.sections, vec![1]);
    let asked = round.asked;
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].id, both.id());
    assert!(round.known.is_empty());
}

#[test]
fn a_finding_in_a_part_that_did_not_change_is_asked_again() {
    let candidate = lesson(&[FIRST, SECOND]);
    let (standing, fresh) = reviewed(
        &candidate,
        vec![report(Some(FindingSite::Section(0)), "Wrong.", Some(FIRST))],
    );
    let round = standing
        .round(&lesson(&[FIRST, "The second part now states a third."]))
        .unwrap();
    assert_eq!(round.sections, vec![1]);
    assert!(!round.first);
    assert_eq!(round.asked.len(), 1);
    assert_eq!(round.asked[0].id, fresh[0].id());
}

#[test]
fn a_resolved_verdict_closes_the_finding() {
    let candidate = lesson(&[FIRST, SECOND]);
    let (mut standing, fresh) = reviewed(
        &candidate,
        vec![report(Some(FindingSite::Section(0)), "Wrong.", Some(FIRST))],
    );
    let changed = lesson(&["The first part is corrected.", SECOND]);
    let (_, new) = answer(
        &mut standing,
        &changed,
        vec![verdict(&fresh[0], false, "")],
        Vec::new(),
    );
    assert!(new.is_empty());
    assert!(standing.open().is_empty());
}

#[test]
fn a_present_verdict_with_a_note_replaces_the_note_and_summary_and_a_blank_one_keeps_them() {
    let candidate = lesson(&[FIRST, SECOND]);
    let (mut standing, fresh) = reviewed(
        &candidate,
        vec![report(Some(FindingSite::Section(0)), "Wrong.", Some(FIRST))],
    );
    let changed = lesson(&["The first part states one plain fact. More.", SECOND]);
    answer(
        &mut standing,
        &changed,
        vec![verdict(&fresh[0], true, "Still wrong, in a new way.")],
        Vec::new(),
    );
    assert_eq!(standing.open()[0].note, "Still wrong, in a new way.");
    assert_eq!(standing.open()[0].summary, "Still wrong, in a new way.");
    let again = lesson(&["The first part states one plain fact. More still.", SECOND]);
    answer(
        &mut standing,
        &again,
        vec![verdict(&fresh[0], true, "")],
        Vec::new(),
    );
    assert_eq!(standing.open()[0].note, "Still wrong, in a new way.");
}

#[test]
fn a_report_on_the_passage_of_a_present_finding_is_that_finding_and_on_a_resolved_one_is_new() {
    let candidate = lesson(&[FIRST, SECOND]);
    let (standing, fresh) = reviewed(
        &candidate,
        vec![report(Some(FindingSite::Section(0)), "Wrong.", Some(FIRST))],
    );
    let changed = lesson(&["The first part states one plain fact. More.", SECOND]);
    let same_passage = || Reported {
        passage_id: Some("section-0-passage-0".to_owned()),
        ..report(Some(FindingSite::Section(0)), "Wrong again.", Some(FIRST))
    };
    let mut present = Standing::new(&Held::default());
    present.resume(&json!({"reviewer": standing.saved().unwrap()}).to_string());
    let (_, new) = answer(
        &mut present,
        &changed,
        vec![verdict(&fresh[0], true, "")],
        vec![same_passage()],
    );
    assert!(new.is_empty());
    assert_eq!(present.open()[0].note, "Wrong again.");
    let mut resolved = standing;
    let (_, new) = answer(
        &mut resolved,
        &changed,
        vec![verdict(&fresh[0], false, "")],
        vec![same_passage()],
    );
    assert_eq!(new.len(), 1);
}

#[test]
fn a_report_gets_the_next_number_and_a_number_is_not_used_again_after_a_resolved_finding() {
    let candidate = lesson(&[FIRST, SECOND]);
    let (mut standing, fresh) = reviewed(
        &candidate,
        vec![report(Some(FindingSite::Section(0)), "Wrong.", Some(FIRST))],
    );
    let changed = lesson(&["The first part is corrected.", SECOND]);
    let (_, new) = answer(
        &mut standing,
        &changed,
        vec![verdict(&fresh[0], false, "")],
        vec![
            report(Some(FindingSite::Section(0)), "Another.", None),
            report(None, "A third.", None),
        ],
    );
    assert_eq!(new[0].id().as_str(), "reviewer/2");
    assert_eq!(new[1].id().as_str(), "reviewer/3");
}

#[test]
fn a_new_finding_at_a_part_that_did_not_change_is_dropped_and_one_about_the_whole_is_kept() {
    let candidate = lesson(&[FIRST, SECOND]);
    let (mut standing, _) = reviewed(&candidate, Vec::new());
    let changed = lesson(&[FIRST, "The second part now states a third."]);
    let (_, new) = answer(
        &mut standing,
        &changed,
        Vec::new(),
        vec![
            report(Some(FindingSite::Section(0)), "In the old part.", None),
            report(Some(FindingSite::Section(1)), "In the new part.", None),
            report(Some(FindingSite::Lesson), "About the whole.", None),
        ],
    );
    assert_eq!(new.len(), 3);
    standing.admit(&changed, new);
    let notes: Vec<&str> = standing.open().iter().map(|f| f.note.as_str()).collect();
    assert_eq!(notes, ["In the new part.", "About the whole."]);
}

#[test]
fn a_findings_passage_follows_the_sentence_a_repair_wrote_for_it_and_is_none_when_its_words_are_gone(
) {
    let two = "Alpha one.\n\nAlpha two.";
    let candidate = lesson(&[two]);
    let (mut standing, fresh) = reviewed(
        &candidate,
        vec![report(
            Some(FindingSite::Section(0)),
            "Wrong.",
            Some("Alpha one."),
        )],
    );
    let id = fresh[0].id();
    let moved = lesson(&["Alpha two.\n\nAlpha one rewritten."]);
    standing.worked_on(&id, ["Alpha one rewritten.".to_owned()]);
    let asked = standing.round(&moved).unwrap().asked;
    assert_eq!(asked[0].passage_id.as_deref(), Some("section-0-passage-1"));
    let gone = lesson(&["Beta."]);
    let asked = standing.round(&gone).unwrap().asked;
    assert_eq!(asked[0].passage_id, None);
}

#[test]
fn worked_on_a_finding_that_is_not_open_changes_nothing() {
    let candidate = lesson(&["Alpha one.\n\nAlpha two."]);
    let (mut standing, _) = reviewed(
        &candidate,
        vec![report(
            Some(FindingSite::Section(0)),
            "Wrong.",
            Some("Alpha one."),
        )],
    );
    let other = ReviewFinding::reviewer(9, Some(FindingSite::Section(0)), "x");
    standing.worked_on(&other.id(), ["Alpha two.".to_owned()]);
    let moved = lesson(&["Alpha two.\n\nAlpha one."]);
    let asked = standing.round(&moved).unwrap().asked;
    assert_eq!(asked[0].passage_id.as_deref(), Some("section-0-passage-1"));
}

#[test]
fn a_standing_seeded_from_stored_findings_asks_about_them_in_a_first_review_and_numbers_after_them()
{
    let candidate = lesson(&[FIRST, SECOND]);
    let stored =
        ReviewFinding::reviewer(3, Some(FindingSite::Section(0)), "Stored.").with_passage(FIRST);
    let held = Held {
        reviewer: vec![(
            stored.clone(),
            site_sha256(&candidate, FindingSite::Section(0)),
        )],
        next_number: 6,
        ..Held::default()
    };
    let mut standing = Standing::new(&held);
    let round = standing.round(&candidate).unwrap();
    assert!(round.first);
    assert_eq!(round.sections, vec![0, 1]);
    assert!(round.whole);
    assert_eq!(round.asked.len(), 1);
    assert_eq!(round.asked[0].id, stored.id());
    assert!(round.known.is_empty());
    assert_eq!(round.scope().asked.map(<[_]>::len), Some(1));
    let (_, new) = answer(
        &mut standing,
        &candidate,
        Vec::new(),
        vec![report(Some(FindingSite::Section(1)), "New.", None)],
    );
    assert_eq!(new[0].id().as_str(), "reviewer/6");
}

#[test]
fn saved_state_round_trips_and_a_saved_string_without_the_key_leaves_the_seeded_state() {
    let candidate = lesson(&[FIRST, SECOND]);
    let (standing, fresh) = reviewed(
        &candidate,
        vec![report(Some(FindingSite::Section(0)), "Wrong.", Some(FIRST))],
    );
    let saved = json!({"reviewer": standing.saved().unwrap(), "spent": []}).to_string();
    let mut resumed = Standing::new(&Held::default());
    resumed.resume(&saved);
    assert!(resumed.round(&candidate).is_none());
    assert_eq!(resumed.open(), standing.open());
    let (_, new) = answer(
        &mut resumed,
        &lesson(&["Changed.", SECOND]),
        vec![verdict(&fresh[0], true, "")],
        vec![report(None, "Next.", None)],
    );
    assert_eq!(new[0].id().as_str(), "reviewer/2");

    let stored = ReviewFinding::reviewer(1, Some(FindingSite::Section(0)), "Stored.");
    let held = Held {
        reviewer: vec![(stored, site_sha256(&candidate, FindingSite::Section(0)))],
        next_number: 2,
        ..Held::default()
    };
    let mut seeded = Standing::new(&held);
    seeded.resume(&json!({"spent": []}).to_string());
    assert!(seeded.round(&candidate).unwrap().first);
    assert_eq!(seeded.open().len(), 1);
}

#[test]
fn a_stored_finding_follows_its_passage_when_its_section_changes_around_it() {
    let candidate = lesson(&[FIRST]);
    let stored =
        ReviewFinding::reviewer(1, Some(FindingSite::Section(0)), "Stored.").with_passage(FIRST);
    let held = Held {
        reviewer: vec![(
            stored.clone(),
            site_sha256(&candidate, FindingSite::Section(0)),
        )],
        next_number: 2,
        ..Held::default()
    };
    let mut standing = Standing::new(&held);
    standing.admit(&candidate, Vec::new());
    let longer = lesson(&[&format!(
        "{SECOND}

{FIRST}"
    )]);
    let round = standing.round(&longer).unwrap();
    let asked = &round.asked;
    assert_eq!(asked[0].passage_id.as_deref(), Some("section-0-passage-1"));
    let fresh = standing.judged(
        &longer,
        &round,
        Judged {
            verdicts: vec![verdict(&stored, true, "")],
            reported: Vec::new(),
        },
    );
    assert!(fresh.is_empty());
    assert_eq!(standing.open()[0].passage.as_deref(), Some(FIRST));
}

/// A report with another passage than the open finding's, which says it continues that finding.
fn continuing(finding: &ReviewFinding, note: &str, passage: Option<&str>) -> Reported {
    Reported {
        passage_id: Some("section-0-passage-1".to_owned()),
        continues: Some(finding.id()),
        ..report(Some(FindingSite::Section(0)), note, passage)
    }
}

#[test]
fn a_report_that_continues_an_open_finding_is_that_finding_whatever_passage_it_chose() {
    let candidate = lesson(&[FIRST, SECOND]);
    let (mut standing, fresh) = reviewed(
        &candidate,
        vec![report(Some(FindingSite::Section(0)), "Wrong.", Some(FIRST))],
    );
    let changed = lesson(&["The first part states one plain fact. More.", SECOND]);
    let (_, new) = answer(
        &mut standing,
        &changed,
        vec![verdict(&fresh[0], true, "")],
        vec![continuing(&fresh[0], "Wrong, in new words.", None)],
    );
    assert!(new.is_empty());
    assert_eq!(standing.open().len(), 1);
    assert_eq!(standing.open()[0].id(), fresh[0].id());
}

#[test]
fn a_report_that_continues_a_finding_the_verdict_called_resolved_replaces_what_the_finding_says() {
    let candidate = lesson(&[FIRST, SECOND]);
    let (mut standing, fresh) = reviewed(
        &candidate,
        vec![report(Some(FindingSite::Section(0)), "Wrong.", Some(FIRST))],
    );
    let changed = lesson(&["The first part states one plain fact. More.", SECOND]);
    let (_, new) = answer(
        &mut standing,
        &changed,
        vec![verdict(&fresh[0], false, "It was fixed.")],
        vec![continuing(&fresh[0], "Still wrong.", Some("More."))],
    );
    assert!(new.is_empty());
    assert_eq!(standing.open().len(), 1);
    let open = &standing.open()[0];
    assert_eq!(open.id(), fresh[0].id());
    assert_eq!(open.note, "Still wrong.");
    assert_eq!(open.summary, "About: Still wrong.");
    assert_eq!(open.site, Some(FindingSite::Section(0)));
    assert_eq!(open.passage.as_deref(), Some("More."));
}

#[test]
fn a_continued_finding_follows_the_report_s_passage() {
    let first = "The first part states one plain fact.\n\nSecond paragraph here.";
    let candidate = lesson(&[first, SECOND]);
    let (mut standing, fresh) = reviewed(
        &candidate,
        vec![report(Some(FindingSite::Section(0)), "Wrong.", Some(FIRST))],
    );
    let changed = lesson(&[
        "The first part states one plain fact.\n\nSecond paragraph here, now.",
        SECOND,
    ]);
    answer(
        &mut standing,
        &changed,
        vec![verdict(&fresh[0], true, "")],
        vec![continuing(
            &fresh[0],
            "Wrong, further down.",
            Some("Second paragraph here, now."),
        )],
    );
    let moved = lesson(&[
        "Second paragraph here, now.\n\nThe first part states one plain fact.",
        SECOND,
    ]);
    let asked = standing.round(&moved).unwrap().asked;
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].passage_id.as_deref(), Some("section-0-passage-0"));
}

#[test]
fn two_reports_that_continue_one_finding_leave_one_finding_and_no_new_number() {
    let candidate = lesson(&[FIRST, SECOND]);
    let (mut standing, fresh) = reviewed(
        &candidate,
        vec![report(Some(FindingSite::Section(0)), "Wrong.", Some(FIRST))],
    );
    let changed = lesson(&["The first part states one plain fact. More.", SECOND]);
    let (_, new) = answer(
        &mut standing,
        &changed,
        vec![verdict(&fresh[0], true, "")],
        vec![
            continuing(&fresh[0], "First words.", None),
            continuing(&fresh[0], "Second words.", None),
        ],
    );
    assert!(new.is_empty());
    assert_eq!(standing.open().len(), 1);
    assert_eq!(standing.open()[0].id(), fresh[0].id());
    assert_eq!(standing.open()[0].note, "First words.");
    let (_, next) = answer(
        &mut standing,
        &lesson(&["Changed again.", SECOND]),
        vec![verdict(&fresh[0], true, "")],
        vec![report(None, "Another.", None)],
    );
    assert_eq!(next[0].id().as_str(), "reviewer/2");
}

#[test]
fn a_report_that_continues_a_finding_in_an_unchanged_part_is_that_finding_restated_and_spends_no_number(
) {
    let candidate = lesson(&[FIRST, SECOND]);
    let (mut standing, fresh) = reviewed(
        &candidate,
        vec![report(Some(FindingSite::Section(0)), "Wrong.", Some(FIRST))],
    );
    let changed = lesson(&[FIRST, "The second part now states a third."]);
    let (round, new) = answer(
        &mut standing,
        &changed,
        Vec::new(),
        vec![continuing(&fresh[0], "Wrong, in new words.", None)],
    );
    assert_eq!(round.asked.len(), 1);
    assert!(new.is_empty());
    assert_eq!(standing.open().len(), 1);
    assert_eq!(standing.open()[0].id(), fresh[0].id());
    assert_eq!(standing.open()[0].note, "Wrong, in new words.");
    standing.admit(&changed, new);
    let again = lesson(&[FIRST, "The second part now states a fourth."]);
    let (_, later) = answer(
        &mut standing,
        &again,
        Vec::new(),
        vec![report(Some(FindingSite::Section(1)), "Another.", None)],
    );
    assert_eq!(later.len(), 1);
    assert_eq!(later[0].id().as_str(), "reviewer/2");
}

#[test]
fn a_report_that_continues_an_id_that_is_not_open_is_raised_as_new() {
    let candidate = lesson(&[FIRST, SECOND]);
    let (mut standing, fresh) = reviewed(
        &candidate,
        vec![report(Some(FindingSite::Section(0)), "Wrong.", Some(FIRST))],
    );
    let gone = ReviewFinding::reviewer(7, Some(FindingSite::Section(0)), "x");
    let changed = lesson(&["The first part states one plain fact. More.", SECOND]);
    let (_, new) = answer(
        &mut standing,
        &changed,
        vec![verdict(&fresh[0], true, "")],
        vec![continuing(&gone, "Elsewhere.", None)],
    );
    assert_eq!(new.len(), 1);
    assert_eq!(new[0].id().as_str(), "reviewer/2");
    assert_eq!(standing.open().len(), 1);
}

#[test]
fn a_site_less_report_that_continues_a_finding_changes_what_it_says_and_not_where_it_is() {
    let candidate = lesson(&[FIRST, SECOND]);
    let (mut standing, fresh) = reviewed(
        &candidate,
        vec![report(Some(FindingSite::Section(0)), "Wrong.", Some(FIRST))],
    );
    let changed = lesson(&["The first part states one plain fact. More.", SECOND]);
    let issue = Reported {
        continues: Some(fresh[0].id()),
        ..report(None, "Wrong, in an issue.", None)
    };
    let (_, new) = answer(
        &mut standing,
        &changed,
        vec![verdict(&fresh[0], false, "It was fixed.")],
        vec![issue],
    );
    assert!(new.is_empty());
    assert_eq!(standing.open().len(), 1);
    let open = &standing.open()[0];
    assert_eq!(open.id(), fresh[0].id());
    assert_eq!(open.site, Some(FindingSite::Section(0)));
    assert_eq!(open.note, "Wrong, in an issue.");
    assert_eq!(open.summary, "About: Wrong, in an issue.");
    assert_eq!(
        open.passage.as_deref(),
        Some("The first part states one plain fact. More.")
    );
}
