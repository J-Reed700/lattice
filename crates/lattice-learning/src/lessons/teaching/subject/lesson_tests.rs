//! Which check of a lesson pass is the one that reads it against its plan.
#![allow(clippy::indexing_slicing)]
use super::*;
use crate::review_findings::PlanCheck;

fn contradiction(subject: &str, note: &str) -> ReviewFinding {
    ReviewFinding::plan(
        PlanCheck::Contradiction,
        subject,
        FindingSite::Section(0),
        note,
    )
}

#[test]
fn the_plan_check_stands_where_the_gate_is_told_it_does() {
    assert_eq!(Beside::Plan.decides(), contract::DECIDES);
}

#[test]
fn a_finding_both_asks_give_appears_once() {
    let first = vec![
        contradiction("beads", "The beads are counted twice."),
        contradiction("rope", "The rope is cut twice."),
    ];
    let second = vec![
        contradiction("rope", "The rope is cut twice, once in each section."),
        contradiction("kettle", "The kettle boils twice."),
    ];
    let all = plan_findings_of(first.clone(), second);
    let ids: HashSet<_> = all.iter().map(ReviewFinding::id).collect();
    assert_eq!(ids.len(), 3);
    assert_eq!(all.len(), 3);
    assert_eq!(all[..2], first[..]);
}

const WRITTEN: &str = "This saved reference describes the exact behavior of the example tool.";
const RESEARCHED: &str =
    "Replication copies every write to several machines so that one failure loses nothing at all.";

fn source(id: &str, excerpt: &str) -> crate::dto::LearningSourceDto {
    crate::dto::LearningSourceDto {
        id: id.into(),
        title: id.into(),
        url: None,
        acquired_at: 0,
        excerpt: excerpt.into(),
    }
}

/// The writer's prompt names one source; research then numbered a second.
fn numbered() -> Vec<crate::dto::LearningSourceDto> {
    vec![source("written", WRITTEN), source("researched", RESEARCHED)]
}

fn writer_context() -> Value {
    json!({"sources":[{"sourceIndex":0,"excerpt":WRITTEN}]})
}

fn cited(index: u64, quote: &str) -> Value {
    json!({"blocks":[{"kind":"explanation","title":"Example","body":"A teaching explanation.","sourceIndex":index,"quote":quote}],"questions":[]})
}

fn quote_checks_of(
    numbered: &[crate::dto::LearningSourceDto],
    candidate: &Value,
) -> Vec<QuoteCheck> {
    let references = ReferenceCollection::lexical(&numbered[..1]).unwrap();
    LessonSubject::new(&references, numbered, &[], Held::default())
        .quote_checks(candidate, &writer_context())
}

#[test]
fn a_quotation_of_a_source_research_numbered_holds_though_the_writer_was_not_given_it() {
    let candidate = cited(1, RESEARCHED);
    assert!(quote_checks_of(&numbered(), &candidate).is_empty());

    let numbered = numbered();
    let references = ReferenceCollection::lexical(&numbered[..1]).unwrap();
    let subject = LessonSubject::new(&references, &numbered, &[], Held::default());
    let mut settled = candidate.clone();
    subject.settle_quotes(&mut settled, &writer_context(), 1);
    assert_eq!(settled, candidate);
}

const SHOWN_TIDE: &str = "The harbor town of Veldmoor keeps a brass gauge beside its eastern quay.";
const SHOWN_SLATE: &str =
    "Each morning the keeper reads the gauge and writes the height of the tide on a slate board.";
const UNSHOWN_LEDGER: &str =
    "Triangular lanterns hang from the Veldmoor customs house, and the keeper’s ledger lists every lantern by name.";
const UNSHOWN_GLASS: &str =
    "A lantern is replaced only when its glass has cracked across both panes.";

/// The writer was shown the first paragraph of a saved text of two.
fn harbor() -> (
    Vec<crate::dto::LearningSourceDto>,
    ReferenceCollection<'static>,
) {
    let shown = format!("{SHOWN_TIDE} {SHOWN_SLATE}");
    let saved = source(
        "harbor",
        &format!("{shown}\n\n{UNSHOWN_LEDGER} {UNSHOWN_GLASS}"),
    );
    let references = ReferenceCollection::lexical(&[saved]).unwrap();
    (vec![source("harbor", &shown)], references)
}

fn harbor_block(body: &str, quote: &str) -> Value {
    json!({"blocks":[{"kind":"explanation","title":"Example","body":body,"sourceIndex":0,"quote":quote}],"questions":[]})
}

#[test]
fn a_quotation_from_the_saved_text_the_writer_was_not_shown_holds() {
    let (numbered, references) = harbor();
    let subject = LessonSubject::new(&references, &numbered, &[], Held::default());
    let own = harbor_block("A teaching explanation.", UNSHOWN_LEDGER);
    assert!(subject.quote_checks(&own, &writer_context()).is_empty());

    let mut cited = harbor_block("A teaching explanation.", SHOWN_TIDE);
    cited["blocks"][0]["citations"] = json!([{
        "sentence": "The keeper's ledger lists every lantern.",
        "sourceIndex": 0,
        "quote": UNSHOWN_LEDGER
    }]);
    assert!(subject.quote_checks(&cited, &writer_context()).is_empty());
}

#[test]
fn a_citation_outside_the_passages_the_writer_was_shown_is_left_where_it_is() {
    let (numbered, references) = harbor();
    let subject = LessonSubject::new(&references, &numbered, &[], Held::default());
    let candidate = harbor_block(
        "The keeper reads the brass gauge each morning and writes the tide height down.",
        UNSHOWN_GLASS,
    );
    let mut settled = candidate.clone();
    assert!(!subject.settle_quotes(&mut settled, &writer_context(), 1));
    assert_eq!(settled, candidate);
}

#[test]
fn the_typography_of_a_quotation_the_writer_was_not_shown_is_restored() {
    let (numbered, references) = harbor();
    let subject = LessonSubject::new(&references, &numbered, &[], Held::default());
    let mut candidate = harbor_block("A teaching explanation.", &UNSHOWN_LEDGER.replace('’', "'"));
    assert!(subject.settle_quotes(&mut candidate, &writer_context(), 0));
    assert_eq!(candidate["blocks"][0]["quote"], UNSHOWN_LEDGER);
}

#[test]
fn a_quotation_no_saved_text_holds_is_cited_to_the_nearest_passage_shown() {
    let (numbered, references) = harbor();
    let subject = LessonSubject::new(&references, &numbered, &[], Held::default());
    let mut candidate = harbor_block(
        "A teaching explanation.",
        "Every morning the keeper records the tide height upon a slate board.",
    );
    assert!(subject.settle_quotes(&mut candidate, &writer_context(), 1));
    assert_eq!(candidate["blocks"][0]["quote"], SHOWN_SLATE);
    assert_eq!(candidate["blocks"][0]["sourceIndex"], 0);
}

#[test]
fn a_quotation_the_numbered_source_does_not_hold_still_fails() {
    let checks = quote_checks_of(&numbered(), &cited(1, WRITTEN));
    assert_eq!(checks.len(), 1);
    assert_eq!(checks[0].path, "blocks[0]");
}

#[test]
fn a_source_index_past_the_numbered_sources_still_fails() {
    let checks = quote_checks_of(&numbered(), &cited(2, RESEARCHED));
    assert_eq!(checks.len(), 1);
    assert_eq!(
        checks[0].error,
        "The citation must select a supplied sourceIndex."
    );
}
