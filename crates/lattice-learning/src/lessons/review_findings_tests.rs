#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
use super::*;
use serde_json::json;

#[test]
fn sites_come_from_the_application_s_own_indices() {
    assert_eq!(
        FindingSite::of_pointer("/blocks/3/title"),
        FindingSite::Section(3)
    );
    assert_eq!(
        FindingSite::of_pointer("/questions/1"),
        FindingSite::Question(1)
    );
    assert_eq!(FindingSite::of_pointer("/blocks"), FindingSite::Lesson);
    assert_eq!(FindingSite::of_pointer(""), FindingSite::Lesson);
    assert_eq!(
        FindingSite::of_part("blocks-10"),
        Some(FindingSite::Section(10))
    );
    assert_eq!(
        FindingSite::of_part("questions[2]"),
        Some(FindingSite::Question(2))
    );
    assert_eq!(FindingSite::of_part("reference-0"), None);
    assert_eq!(FindingSite::of_part("requirements"), None);
}

#[test]
fn a_site_is_a_pointer_to_its_block_or_question() {
    assert_eq!(
        FindingSite::Section(3).pointer().as_deref(),
        Some("/blocks/3")
    );
    assert_eq!(
        FindingSite::Question(1).pointer().as_deref(),
        Some("/questions/1")
    );
    assert_eq!(FindingSite::Lesson.pointer(), None);
}

#[test]
fn a_finding_is_also_about_a_second_part_only_when_it_is_another_one() {
    let finding = ReviewFinding::reviewer(1, Some(FindingSite::Section(1)), "x");
    assert_eq!(finding.also, None);
    assert_eq!(
        finding.clone().with_also(FindingSite::Section(1)).also,
        None
    );
    assert_eq!(
        finding.with_also(FindingSite::Section(2)).also,
        Some(FindingSite::Section(2))
    );
}

#[test]
fn a_finding_stored_without_a_second_part_is_read_back() {
    let finding = ReviewFinding::reviewer(1, Some(FindingSite::Section(1)), "x");
    let mut stored = serde_json::to_value(&finding).unwrap();
    stored.as_object_mut().unwrap().remove("also");
    let read: ReviewFinding = serde_json::from_value(stored).unwrap();
    assert_eq!(read, finding);
}

#[test]
fn a_finding_has_its_own_id_and_keeps_it_whatever_its_note_says() {
    let part = FindingSite::Section(2);
    let first = ReviewFinding::reviewer(1, Some(part), "The example returns the string \"4\".");
    let reworded = ReviewFinding::reviewer(
        1,
        Some(part),
        "Section 3 shows a string result where the code returns a number.",
    );
    assert_eq!(first.id(), reworded.id());
    assert_eq!(first.id().as_str(), "reviewer/1");
    assert_eq!(first.id().reviewer_number(), Some(1));
    assert_ne!(first.id(), ReviewFinding::reviewer(2, Some(part), "x").id());
    assert_eq!(first.place().id(), "teaching:section-2");
    assert_ne!(
        ReviewFinding::of_kind(FindingKind::Teaching, part, "x").id(),
        ReviewFinding::of_kind(FindingKind::Quotation, part, "x").id()
    );
    assert_eq!(
        ReviewFinding::reviewer(7, None, "lesson-wide")
            .id()
            .as_str(),
        "reviewer/7"
    );
    assert_eq!(
        ReviewFinding::answer_key(2, "x").id().as_str(),
        "key/question-2"
    );

    let broken = |rule: Rule| ReviewFinding::rule(rule, FindingKind::Structure, part, "x");
    assert_eq!(
        broken(Rule::TitleMissing).id().as_str(),
        "rule/title-missing/section-2"
    );
    assert_ne!(
        broken(Rule::TitleMissing).id(),
        broken(Rule::TitleRepeated).id()
    );

    let about = |text: &str, note: &str| {
        ReviewFinding::rule_about(Rule::Uncited, FindingKind::Quotation, part, text, note)
    };
    assert_ne!(
        about("A first sentence.", "x").id(),
        about("A second sentence.", "x").id()
    );
    assert_eq!(
        about("A first sentence.", "x").id(),
        about("A first sentence.", "said differently").id()
    );
    assert_eq!(
        about("A first sentence.", "x").id().as_str(),
        format!("rule/uncited/section-2/{}", subject_of("A first sentence."))
    );
    assert_eq!(about("  ", "x").id().as_str(), "rule/uncited/section-2");

    let planned = ReviewFinding::plan(PlanCheck::Planned, "idea-1", part, "x");
    assert_eq!(planned.id().as_str(), "plan/planned/idea-1");
    let used = ReviewFinding::plan(PlanCheck::Used, "0123456789abcdef", part, "x");
    assert_eq!(used.id().as_str(), "plan/used/section-2/0123456789abcdef");
}

#[test]
fn a_sentence_is_named_by_sixteen_hex_characters_of_its_folded_text() {
    let name = subject_of("A first sentence.");
    assert_eq!(name.len(), 16);
    assert!(name.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(name, subject_of("a   FIRST sentence."));
    assert_ne!(name, subject_of("A second sentence."));
}

#[test]
fn a_finding_s_words_are_the_whole_sentences_its_passage_lies_in() {
    let candidate = json!({"blocks":[
        {"title":"Sums","body":"Add two numbers. Then check the second tool carefully. Done."},
        {"title":"Other","body":"Then check the second tool carefully."}
    ],"questions":[]});
    let finding = ReviewFinding::reviewer(1, Some(FindingSite::Section(0)), "x")
        .with_passage("the second tool");
    assert_eq!(
        sentences_of(&candidate, &finding),
        ["Then check the second tool carefully."]
    );
    let absent = ReviewFinding::reviewer(1, Some(FindingSite::Section(0)), "x")
        .with_passage("words that are not there");
    assert!(sentences_of(&candidate, &absent).is_empty());
    let wide = ReviewFinding::reviewer(1, None, "x").with_passage("the second tool");
    assert_eq!(sentences_of(&candidate, &wide).len(), 1);
}

#[test]
fn a_sentence_the_lesson_already_had_elsewhere_is_not_new() {
    let lesson = |first: &str| json!({"title":"A lesson","blocks":[{"body":first},{"body":"Check the sum."}],"questions":[]});
    let previous = lesson("Add two numbers.");
    let moved = lesson("Add two numbers. Check the sum.");
    assert!(new_sentences(&previous, &moved).is_empty());
    let written = lesson("Add two numbers. Say what a sum is.");
    assert_eq!(
        new_sentences(&previous, &written),
        vec![(
            FindingSite::Section(0),
            vec!["Say what a sum is.".to_owned()]
        )]
    );
}

#[test]
fn a_list_reads_as_plain_words() {
    let list =
        |items: &[&str]| join_plain(&items.iter().map(|i| (*i).to_owned()).collect::<Vec<_>>());
    assert_eq!(list(&[]), "");
    assert_eq!(list(&["30"]), "30");
    assert_eq!(list(&["30", "seconds"]), "30 and seconds");
    assert_eq!(
        list(&["30", "seconds", "retry once"]),
        "30, seconds and retry once"
    );
}

#[test]
fn a_part_is_named_by_its_place_in_the_lesson_counted_from_one() {
    assert_eq!(plain_part(FindingSite::Section(2)), "section 3");
    assert_eq!(plain_part(FindingSite::Question(1)), "question 2");
    assert_eq!(plain_part(FindingSite::Lesson), "the lesson");
}

#[test]
fn a_summary_is_set_apart_from_the_note_written_for_the_model() {
    let finding =
        ReviewFinding::reviewer(1, None, "Fix blocks[2].").with_summary("The example is wrong.");
    assert_eq!(finding.note, "Fix blocks[2].");
    assert_eq!(finding.summary, "The example is wrong.");
    assert_eq!(ReviewFinding::reviewer(1, None, "x").summary, "");
}

#[test]
fn a_quotation_finding_is_not_handed_the_part_s_quote_as_its_passage() {
    let candidate = json!({"blocks":[{"title":"Sums","body":"Add.","quote":"The excerpt this whole section cites."}],"questions":[{"prompt":"What is 2+2?","quote":"The excerpt this question cites."}]});
    for site in [FindingSite::Section(0), FindingSite::Question(0)] {
        let finding = ReviewFinding::rule(Rule::Quotation, FindingKind::Quotation, site, "x");
        assert_eq!(unit(&candidate, &finding).passage, None, "{site:?}");
    }
}

#[test]
fn a_unit_s_hash_changes_with_its_text_only() {
    let candidate = json!({"blocks":[{"title":"Sums","body":"Add."},{"title":"Other","body":"B"}],"questions":[{"prompt":"What is 2+2?"}]});
    let finding = ReviewFinding::reviewer(1, Some(FindingSite::Section(0)), "x");
    let before = unit(&candidate, &finding);
    assert_eq!(before.title, "Section 1: Sums");
    let mut other_changed = candidate.clone();
    other_changed["blocks"][1]["body"] = json!("C");
    assert_eq!(unit(&other_changed, &finding).sha256, before.sha256);
    let mut changed = candidate.clone();
    changed["blocks"][0]["body"] = json!("Add twice.");
    assert_ne!(unit(&changed, &finding).sha256, before.sha256);
    let question = ReviewFinding::answer_key(0, "x");
    assert_eq!(
        unit(&candidate, &question).passage.as_deref(),
        Some("What is 2+2?")
    );
}

#[test]
fn each_rule_has_the_remedy_its_fix_needs() {
    use Remedy::{Edits, Whole};
    let table = [
        (Rule::Unreadable, Whole),
        (Rule::Schema, Whole),
        (Rule::BlockBody, Whole),
        (Rule::PracticeRubric, Whole),
        (Rule::Quotation, Edits),
        (Rule::QuoteChecks, Whole),
        (Rule::TitleMissing, Whole),
        (Rule::TitleRepeated, Edits),
        (Rule::CheckHoldback, Edits),
        (Rule::FenceUnclosed, Edits),
        (Rule::PromptMissing, Whole),
        (Rule::PromptRepeated, Edits),
        (Rule::ExplanationMissing, Whole),
        (Rule::Choices, Whole),
        (Rule::KeyLongest, Edits),
        (Rule::KeyGivenAway, Edits),
        (Rule::RationaleCount, Whole),
        (Rule::RationaleMissing, Whole),
        (Rule::Uncited, Edits),
        (Rule::CitationLabel, Edits),
        (Rule::SourceNumber, Edits),
        (Rule::SentenceRepeated, Edits),
        (Rule::UnshownCheck, Whole),
        (Rule::RecapFirst, Whole),
        (Rule::SectionMix, Whole),
        (Rule::QuestionCount, Whole),
        (Rule::NoExplanation, Whole),
        (Rule::QuestionKinds, Whole),
        (Rule::CopiedWording, Edits),
        (Rule::SentenceCitation, Edits),
        (Rule::PartCitation, Edits),
    ];
    for (rule, remedy) in table {
        assert_eq!(rule.remedy(), remedy, "{rule:?}");
        let finding = ReviewFinding::rule(rule, FindingKind::Structure, FindingSite::Lesson, "n");
        assert_eq!(finding.remedy(), remedy, "{rule:?}");
    }
}

#[test]
fn a_finding_that_is_not_a_broken_rule_is_corrected_by_edits() {
    let reviewer = ReviewFinding::reviewer(1, Some(FindingSite::Section(0)), "n");
    let key = ReviewFinding::answer_key(0, "n");
    let plan = ReviewFinding::plan(PlanCheck::Used, "an idea", FindingSite::Section(0), "n");
    for finding in [reviewer, key, plan] {
        assert_eq!(finding.remedy(), Remedy::Edits);
    }
}
