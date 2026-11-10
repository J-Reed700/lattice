#![allow(clippy::indexing_slicing, clippy::unwrap_used)]
use super::*;
use serde_json::json;

fn lesson_rules(candidate: &Value) -> Vec<ReviewFinding> {
    crate::lessons::shape::policy().rules(candidate, crate::lessons::shape::Phase::Draft)
}

fn block(kind: &str, title: &str) -> Value {
    let body = if kind == "check" {
        format!("Case for {title}: what happens to the lid?\n\n{CHECK_REVEAL}\n\nAnswer for {title}: it bulges.")
    } else {
        format!("Body of {title}.")
    };
    json!({"kind":kind,"title":title,"body":body})
}

const CASE: &str = "A sealed jar of air is carried from the beach up a mountain. Predict what happens to its lid, and say why.";
const REVEAL: &str = "The lid bulges: outside pressure falls with altitude while the air inside keeps its pressure. If you said it caves in, you reversed which side lost pressure.";

fn question(kind: &str, prompt: &str) -> Value {
    json!({"kind":kind,"prompt":prompt,"options":["One","Two","Three","Four"],"correctIndex":0,
        "rationales":["","Two reverses the order.","Three mistakes the cause for the effect.","Four states it backwards."],
        "explanation":"Because."})
}

fn lesson() -> Value {
    json!({"blocks":[
        block("explanation","First idea"), block("worked_example","First example"),
        block("contrast","Not to be confused with"), block("check","Your turn"),
        block("explanation","Second idea"), block("worked_example","Second example"),
        block("check","Your turn again"), block("guided_practice","Guided"),
        block("independent_practice","On your own"), block("reflection","From memory"),
        block("recap","Recap"),
    ],"questions":[
        question("practice","P1?"), question("practice","P2?"), question("quiz","Q1?"),
        question("quiz","Q2?"), question("test","T1?"), question("test","T2?"),
    ]})
}

#[test]
fn rule_notes_are_the_findings_notes() {
    let mut candidate = lesson();
    candidate["blocks"].as_array_mut().unwrap().swap(5, 6);
    let notes = lesson_rule_notes(&candidate);
    assert_eq!(notes.len(), 1);
    assert!(notes[0].contains("no worked example has shown"));
    assert!(lesson_rule_notes(&lesson()).is_empty());
}

#[test]
fn a_lesson_in_its_mix_breaks_no_rule() {
    assert!(lesson_rules(&lesson()).is_empty());
}

#[test]
fn a_teaching_sentence_in_two_sections_is_a_finding_at_the_later_one() {
    const SHARED: &str = "You change the location with cd and check it with pwd.";
    let mut candidate = lesson();
    candidate["blocks"][1]["body"] = json!(format!("Opening. {SHARED}"));
    candidate["blocks"][4]["body"] = json!(format!("Closing.  {SHARED}"));
    let findings = lesson_rules(&candidate);
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].place().site, FindingSite::Section(4));
    assert_eq!(findings[0].kind, FindingKind::Teaching);
    assert_eq!(findings[0].passage.as_deref(), Some(SHARED));
}

#[test]
fn each_broken_rule_is_a_finding_at_its_place() {
    let mut candidate = lesson();
    candidate["blocks"][4]["title"] = json!("first  IDEA");
    candidate["blocks"][10]["kind"] = json!("explanation");
    candidate["questions"][3]["prompt"] = json!("q1?");
    candidate["questions"][4]["options"] = json!(["Yes", "yes", "Maybe", "No way"]);
    let findings = lesson_rules(&candidate);
    let sites: Vec<_> = findings.iter().map(|f| f.place().site).collect();
    assert_eq!(
        sites,
        [
            FindingSite::Section(4),
            FindingSite::Lesson,
            FindingSite::Question(3),
            FindingSite::Question(4),
        ]
    );
    assert!(findings[0].note.contains("repeats the title of section 1"));
    assert!(findings[1].note.contains("0 recap"));
    assert!(findings
        .iter()
        .all(|finding| finding.kind == FindingKind::Structure));
}

/// A check that gives its answer away, or has nothing to reason through,
/// is no check: the reveal line must split a case from its answer.
#[test]
fn a_check_holds_its_answer_back_under_one_reveal_line() {
    assert_eq!(
        split_check(&format!("{CASE}\n#### check your thinking\n{REVEAL}")),
        Some((format!("{CASE}\n").as_str(), REVEAL))
    );
    for body in [
        format!("{CASE} {REVEAL}"),
        format!("{CASE}\n{CHECK_REVEAL}\n  "),
        format!("  \n{CHECK_REVEAL}\n{REVEAL}"),
        format!("{CASE}\n{CHECK_REVEAL}\n{REVEAL}\n{CHECK_REVEAL}\nA second answer, worded apart."),
    ] {
        let mut candidate = lesson();
        candidate["blocks"][3]["body"] = json!(body);
        let findings = lesson_rules(&candidate);
        assert_eq!(findings.len(), 1, "{body}");
        assert_eq!(findings[0].place().site, FindingSite::Section(3));
        assert!(findings[0].note.contains(CHECK_REVEAL));
    }
}

/// A check tests what an example has shown; the learner explains the
/// lesson from memory before the recap they compare it with.
#[test]
fn checks_follow_an_example_and_recall_comes_before_the_recap() {
    let mut candidate = lesson();
    let blocks = candidate["blocks"].as_array_mut().unwrap();
    blocks.swap(1, 3);
    blocks.swap(9, 10);
    let findings = lesson_rules(&candidate);
    let sites: Vec<_> = findings.iter().map(|f| f.place().site).collect();
    assert_eq!(sites, [FindingSite::Section(1), FindingSite::Section(9)]);
    assert!(findings[0].note.contains("no worked example has shown"));
    assert!(findings[1].note.contains("from memory"));
}

/// The second idea's check before the second idea's example tests what
/// the lesson has not worked through, even after the first idea's example.
#[test]
fn a_check_needs_its_own_ideas_example_since_the_last_check() {
    let mut candidate = lesson();
    candidate["blocks"].as_array_mut().unwrap().swap(5, 6);
    let findings = lesson_rules(&candidate);
    let sites: Vec<_> = findings.iter().map(|f| f.place().site).collect();
    assert_eq!(sites, [FindingSite::Section(5)]);
}

const WAITS: &str = "every request waits thirty seconds";
const TIMEOUT: &str = "The guide says \"every request waits thirty seconds\" before it fails.";

fn cited(candidate: &mut Value, quote: &str) {
    candidate["blocks"][0]["citations"] =
        json!([{"sentence":TIMEOUT,"sourceIndex":0,"quote":quote}]);
}

/// Quoted wording stands only in a sentence the writer cited.
#[test]
fn quoted_wording_without_a_citation_is_a_finding_at_its_section() {
    let mut candidate = lesson();
    candidate["blocks"][0]["body"] = json!(format!("{TIMEOUT} It is set for each request."));

    let findings = lesson_rules(&candidate);

    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].place().site, FindingSite::Section(0));
    assert_eq!(findings[0].kind, FindingKind::Quotation);
    assert!(findings[0].note.contains("without citing it"));
    assert!(findings[0].note.contains(TIMEOUT));
    assert!(findings[0].note.contains("citations of /blocks/0"));

    cited(
        &mut candidate,
        "The guide: every request waits thirty seconds before failing.",
    );
    assert!(lesson_rules(&candidate).is_empty());
}

const LONG_WAIT: &str = "thirty seconds of waiting each time";
const LONG_SENTENCE: &str = "The default timeout for every outgoing request in the client library is \"thirty seconds of waiting each time\" unless the caller sets another value.";

/// The user and the repair both read the whole sentence.
#[test]
fn an_uncited_specific_is_summarised_with_its_whole_sentence() {
    let mut candidate = lesson();
    candidate["blocks"][0]["body"] = json!(LONG_SENTENCE);

    let findings = lesson_rules(&candidate);

    assert_eq!(findings.len(), 1);
    assert_eq!(
        findings[0].summary,
        format!("No source is cited for this sentence: \u{201c}{LONG_SENTENCE}\u{201d}")
    );
    assert_eq!(findings[0].passage, None);
    assert_eq!(findings[0].sentence.as_deref(), Some(LONG_SENTENCE));
}

#[test]
fn a_quote_that_lacks_the_wording_is_summarised_with_what_it_lacks_and_the_whole_sentence() {
    let mut candidate = lesson();
    candidate["blocks"][0]["body"] = json!(LONG_SENTENCE);
    candidate["blocks"][0]["citations"] = json!([{
        "sentence":LONG_SENTENCE,"sourceIndex":0,
        "quote":"A timeout applies to each request, in seconds."}]);

    let findings = lesson_rules(&candidate);

    assert_eq!(findings.len(), 1);
    assert_eq!(
        findings[0].summary,
        format!("The quotation cited for this sentence does not contain {LONG_WAIT}: \u{201c}{LONG_SENTENCE}\u{201d}")
    );
    assert_eq!(
        findings[0].passage.as_deref(),
        Some("A timeout applies to each request, in seconds.")
    );
    assert_eq!(findings[0].sentence.as_deref(), Some(LONG_SENTENCE));
}

#[test]
fn only_a_sentence_level_quotation_finding_has_a_sentence() {
    let mut candidate = lesson();
    candidate["blocks"][4]["title"] = json!("first  IDEA");
    let findings = lesson_rules(&candidate);
    assert!(!findings.is_empty());
    assert!(findings.iter().all(|finding| finding.sentence.is_none()));
}

#[test]
fn each_broken_rule_says_what_is_wrong_in_plain_words() {
    let mut candidate = lesson();
    candidate["blocks"][4]["title"] = json!("first  IDEA");
    candidate["blocks"][10]["kind"] = json!("explanation");
    candidate["questions"][3]["prompt"] = json!("q1?");
    candidate["questions"][4]["options"] = json!(["Yes", "yes", "Maybe", "No way"]);

    let summaries: Vec<_> = lesson_rules(&candidate)
        .into_iter()
        .map(|finding| finding.summary)
        .collect();

    assert_eq!(
        summaries,
        [
            "Section 5 has the same title as section 1.",
            "The lesson should have at least two explanations, two worked examples, one contrast and two checks, and one each of guided practice, independent practice, reflection and recap. It has 3 explanations, 2 worked examples, 1 contrast, 2 checks, 1 guided practice, 1 independent practice, 1 reflection, 0 recaps.",
            "Question 4 asks the same thing as question 3.",
            "Question 5 needs four distinct answer choices.",
        ]
    );
}

#[test]
fn a_missing_title_prompt_or_explanation_is_named_in_plain_words() {
    let mut candidate = lesson();
    candidate["blocks"][0]["title"] = json!("");
    candidate["questions"][0]["explanation"] = json!("");
    candidate["questions"][1]["prompt"] = json!("  ");

    let summaries: Vec<_> = lesson_rules(&candidate)
        .into_iter()
        .map(|finding| finding.summary)
        .collect();

    assert_eq!(
        summaries,
        [
            "Section 1 needs a title.",
            "Question 1 needs an answer explanation.",
            "Question 2 needs a prompt.",
        ]
    );
}

#[test]
fn a_check_and_a_recap_out_of_place_are_named_in_plain_words() {
    let mut candidate = lesson();
    let blocks = candidate["blocks"].as_array_mut().unwrap();
    blocks.swap(1, 3);
    blocks.swap(9, 10);
    let summaries: Vec<_> = lesson_rules(&candidate)
        .into_iter()
        .map(|finding| finding.summary)
        .collect();
    assert_eq!(
        summaries,
        [
            "Section 2 is a check on an idea that no worked example has shown since the previous check.",
            "Section 10 recaps the lesson before the learner is asked to explain it from memory.",
        ]
    );

    let mut giveaway = lesson();
    giveaway["blocks"][3]["body"] = json!(format!("{CASE} {REVEAL}"));
    let findings = lesson_rules(&giveaway);
    assert_eq!(findings.len(), 1);
    assert_eq!(
        findings[0].summary,
        "Section 4 is a check, but it does not hold its answer back until the learner has tried."
    );
}

#[test]
fn too_few_questions_of_a_kind_are_named_in_plain_words() {
    let mut candidate = lesson();
    candidate["questions"].as_array_mut().unwrap().truncate(5);
    let findings = lesson_rules(&candidate);
    assert_eq!(findings.len(), 1);
    assert_eq!(
        findings[0].summary,
        "The lesson needs at least two questions each for practice, quiz and test. It has 2 practice questions, 2 quiz questions, 1 test question."
    );
}

#[test]
fn a_quote_that_lacks_the_wording_does_not_cite_it() {
    let mut candidate = lesson();
    candidate["blocks"][0]["body"] = json!(TIMEOUT);
    cited(
        &mut candidate,
        "A timeout applies to each request, in seconds.",
    );

    let findings = lesson_rules(&candidate);

    assert_eq!(findings.len(), 1);
    assert!(findings[0]
        .note
        .contains(&format!("does not contain {WAITS}")));
}

/// A worked example's case, code and a list's markers need no citation.
#[test]
fn hypothetical_cases_code_and_list_markers_need_no_citation() {
    let mut candidate = lesson();
    candidate["blocks"][1]["body"] = json!("Suppose a list holds 5 items and 3 are removed.");
    candidate["blocks"][0]["body"] = json!(
        "Read the file like this:\n```python\nrows = 100\nprint(rows)\n```\n1. Open the file.\n2) Read it row by row.\n- Close it."
    );
    assert!(lesson_rules(&candidate).is_empty());
}

/// A figure is the classifier's question, never a citation finding: not the
/// question's givens an answer restates, nor one of its own.
#[test]
fn an_answer_s_figures_are_no_citation_finding() {
    let mut candidate = lesson();
    candidate["questions"][0]["prompt"] = json!(
        "A classifier outputs a confidence score of 0.75. Your threshold for automatic routing is 0.80. What should the system do?"
    );
    candidate["questions"][0]["options"] = json!([
        "Route it automatically",
        "Send it to review",
        "Retry",
        "Lower it to 0.70"
    ]);
    candidate["questions"][0]["explanation"] = json!(
        "Since 0.75 is below the 0.80 threshold, the document goes to review. Lowering the threshold to 0.70 would route it automatically."
    );
    assert!(lesson_rules(&candidate).is_empty());

    candidate["questions"][0]["explanation"] = json!(
        "Since 0.75 is below the 0.80 threshold, the document goes to review, as 30% of documents do."
    );
    assert!(lesson_rules(&candidate).is_empty());
}

const SOURCE_SAYS: &str = "The guide: every request waits thirty seconds before failing.";

fn uncited_lesson() -> Value {
    let mut candidate = lesson();
    candidate["blocks"][0]["body"] = json!(format!("{TIMEOUT} It is set for each request."));
    candidate["blocks"][0]["citations"] = json!([]);
    candidate
}

fn patch(patch: Value) -> Value {
    crate::lessons::text_edits::apply(&uncited_lesson(), &patch.to_string()).unwrap()
}

/// Uncited wording the supplied excerpt holds ends cited, its sentence
/// as it was.
#[test]
fn uncited_wording_a_source_contains_is_settled_by_citing_it() {
    let before = uncited_lesson();
    assert_eq!(lesson_rules(&before).len(), 1);

    let revised = patch(json!({"edits":[],"citations":[
        {"path":"/blocks/0","sentence":TIMEOUT,"sourceIndex":0,"quote":SOURCE_SAYS}]}));

    assert_eq!(revised["blocks"][0]["body"], before["blocks"][0]["body"]);
    assert_eq!(
        revised["blocks"][0]["citations"],
        json!([{"sentence":TIMEOUT,"sourceIndex":0,"quote":SOURCE_SAYS}])
    );
    assert!(lesson_rules(&revised).is_empty());
}

/// Wording no excerpt holds ends as the lesson's own sentence, with nothing cited.
#[test]
fn uncited_wording_no_source_contains_is_settled_by_removing_the_quotation_marks() {
    let revised = patch(json!({"edits":[
        {"path":"/blocks/0/body","before":TIMEOUT,"after":"The default timeout is set by the caller."}],
        "citations":[]}));

    assert!(lesson_rules(&revised).is_empty());
    assert_eq!(revised["blocks"][0]["citations"], json!([]));
}

/// A citation whose quote lacks the figure settles nothing: the quote
/// has to contain what the sentence states, whoever appended it.
#[test]
fn a_citation_appended_with_a_quote_that_lacks_the_wording_settles_nothing() {
    let revised = patch(json!({"edits":[],"citations":[
        {"path":"/blocks/0","sentence":TIMEOUT,"sourceIndex":0,
         "quote":"A timeout applies to each request, in seconds."}]}));

    let findings = lesson_rules(&revised);

    assert_eq!(findings.len(), 1);
    assert!(findings[0]
        .note
        .contains(&format!("does not contain {WAITS}")));
    // A second citation whose quote holds the figure settles it.
    let mut again = revised;
    again["blocks"][0]["citations"]
        .as_array_mut()
        .unwrap()
        .push(json!({"sentence":TIMEOUT,"sourceIndex":0,"quote":SOURCE_SAYS}));
    assert!(lesson_rules(&again).is_empty());
}

#[test]
fn a_question_needs_one_reason_per_choice() {
    for reasons in [
        json!(["", "Two fails.", "Three fails."]),
        json!([
            "",
            "Two fails.",
            "Three fails.",
            "Four fails.",
            "Five fails."
        ]),
        json!([]),
    ] {
        let mut candidate = lesson();
        candidate["questions"][1]["rationales"] = reasons.clone();
        let findings = lesson_rules(&candidate);
        assert_eq!(findings.len(), 1, "{reasons}");
        assert_eq!(findings[0].place().site, FindingSite::Question(1));
        assert_eq!(findings[0].kind, FindingKind::Structure);
    }
    let mut missing = lesson();
    missing["questions"][1]
        .as_object_mut()
        .unwrap()
        .remove("rationales");
    assert_eq!(lesson_rules(&missing).len(), 1);
}

#[test]
fn a_wrong_choice_without_a_reason_is_a_finding_and_the_right_choices_empty_entry_is_not() {
    let mut candidate = lesson();
    candidate["questions"][2]["rationales"] = json!(["", "", "Three fails.", "  "]);
    let findings = lesson_rules(&candidate);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].place().site, FindingSite::Question(2));
    assert_eq!(findings[0].kind, FindingKind::Structure);
    assert!(
        findings[0].summary.contains("choice 2"),
        "{}",
        findings[0].summary
    );
    assert!(
        findings[0].summary.contains("choice 4"),
        "{}",
        findings[0].summary
    );
    assert!(!findings[0].summary.contains("choice 1"));

    let mut moved = lesson();
    moved["questions"][2]["correctIndex"] = json!(2);
    moved["questions"][2]["rationales"] = json!(["One fails.", "Two fails.", "", "Four fails."]);
    assert!(lesson_rules(&moved).is_empty());
    moved["questions"][2]["rationales"] = json!(["", "Two fails.", "", "Four fails."]);
    assert_eq!(lesson_rules(&moved).len(), 1);
}

#[test]
fn a_reason_is_held_to_the_citation_rule_like_the_explanation() {
    let mut candidate = lesson();
    candidate["questions"][3]["rationales"] = json!([
        "",
        "The guide says \"the cache holds at most 128 entries\" in each node.",
        "Three fails.",
        "Four fails."
    ]);
    let findings = lesson_rules(&candidate);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].place().site, FindingSite::Question(3));
    assert_eq!(findings[0].kind, FindingKind::Quotation);

    candidate["questions"][3]["citations"] = json!([
        {"sentence":"The guide says \"the cache holds at most 128 entries\" in each node.","sourceIndex":0,"quote":"The cache holds at most 128 entries at a time."}
    ]);
    assert!(lesson_rules(&candidate).is_empty());
}

#[test]
fn a_question_explanation_is_held_to_the_same_rule() {
    let mut candidate = lesson();
    candidate["questions"][2]["explanation"] =
        json!("The guide says \"the cache holds at most 128 entries\" in each node.");

    let findings = lesson_rules(&candidate);

    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].place().site, FindingSite::Question(2));
}

/// What a section is left to teach with: prose, not headings, markers or
/// code.
#[test]
fn a_sections_teaching_sentences_are_its_prose_without_headings_and_code() {
    let body = "### A heading\nFirst point. Second point.\n```python\nprint(1)\n```\n- A listed point.\n\n1. ...";
    assert_eq!(
        teaching_sentences(body),
        ["First point.", "Second point.", "A listed point."]
    );
    assert!(teaching_sentences("```text\nOnly code.\n```\n## Heading").is_empty());
}

#[test]
fn too_few_questions_of_a_kind_is_one_lesson_finding() {
    let mut candidate = lesson();
    candidate["questions"].as_array_mut().unwrap().truncate(5);
    let findings = lesson_rules(&candidate);
    assert_eq!(findings.len(), 1);
    assert!(findings[0].note.ends_with("2 practice, 2 quiz, 1 test."));
}

#[test]
fn a_question_prompt_longer_than_1200_characters_is_no_finding() {
    let mut candidate = lesson();
    candidate["questions"][1]["prompt"] =
        json!("Which choice follows from the premise given here? ".repeat(30));
    assert!(lesson_rules(&candidate).is_empty());
    candidate["questions"][1]["prompt"] = json!("  ");
    let findings = lesson_rules(&candidate);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].place().site, FindingSite::Question(1));
}

#[test]
fn an_answer_explanation_longer_than_1600_characters_is_no_finding() {
    let mut candidate = lesson();
    candidate["questions"][2]["explanation"] =
        json!("Because the premise fixes the outcome. ".repeat(60));
    assert!(lesson_rules(&candidate).is_empty());
    candidate["questions"][2]["explanation"] = json!("");
    assert_eq!(lesson_rules(&candidate).len(), 1);
}

#[test]
fn an_answer_choice_longer_than_500_characters_or_of_two_letters_is_no_finding() {
    let mut candidate = lesson();
    candidate["questions"][0]["options"] = json!(["Yes", "No", "Maybe", "Perhaps"]);
    candidate["questions"][1]["options"] =
        json!(["One", "Two", "Three", "A long answer choice. ".repeat(40)]);
    assert!(lesson_rules(&candidate).is_empty());
    candidate["questions"][2]["options"] = json!(["One", "Two", "Three", "  "]);
    assert_eq!(lesson_rules(&candidate).len(), 1);
}

#[test]
fn a_section_longer_than_5000_characters_is_no_finding() {
    let mut candidate = lesson();
    candidate["blocks"][0]["body"] = json!((0..120u8)
        .map(|n| {
            let (high, low) = (n / 26, n % 26);
            format!(
                "A long explanation of the idea {}{}, with no figures in it. ",
                char::from(b'a' + high),
                char::from(b'a' + low)
            )
        })
        .collect::<String>());
    assert!(lesson_rules(&candidate).is_empty());
}

#[test]
fn a_check_with_a_few_words_on_each_side_of_its_reveal_is_held_back() {
    let mut candidate = lesson();
    candidate["blocks"][3]["body"] = json!(format!("Which one?\n{CHECK_REVEAL}\nThe first."));
    assert!(lesson_rules(&candidate).is_empty());
    for body in [
        format!("{CASE}\n{CHECK_REVEAL}\n"),
        format!("\n{CHECK_REVEAL}\n{REVEAL}"),
    ] {
        candidate["blocks"][3]["body"] = json!(body);
        let findings = lesson_rules(&candidate);
        assert_eq!(findings.len(), 1, "{body}");
        assert_eq!(findings[0].place().site, FindingSite::Section(3));
    }
}

#[test]
fn a_block_title_longer_than_160_characters_or_of_two_letters_is_no_finding() {
    let mut candidate = lesson();
    candidate["blocks"][2]["title"] = json!("A title that runs on and on. ".repeat(10));
    candidate["blocks"][3]["title"] = json!("Go");
    assert!(lesson_rules(&candidate).is_empty());
    candidate["blocks"][3]["title"] = json!("   ");
    assert_eq!(lesson_rules(&candidate).len(), 1);
}

fn findings_of(candidate: &Value) -> Vec<(String, Option<String>)> {
    lesson_rules(candidate)
        .into_iter()
        .map(|finding| (finding.summary, finding.sentence))
        .collect()
}

#[test]
fn a_source_label_needs_no_citation_and_has_no_quote_to_lack_its_digit() {
    let sentence = "Source 0 describes the usual pattern.";
    let mut candidate = lesson();
    candidate["blocks"][0]["body"] = json!(sentence);
    assert!(!findings_of(&candidate)
        .iter()
        .any(|(summary, _)| summary.starts_with("No source is cited")));

    candidate["blocks"][0]["citations"] = json!([{
        "sentence":sentence,"sourceIndex":0,
        "quote":"A root prompt and a user prompt set the pattern."}]);
    assert!(!findings_of(&candidate)
        .iter()
        .any(|(summary, _)| summary.contains("does not contain")));
}

#[test]
fn a_sentence_naming_a_source_by_number_is_a_finding_with_its_whole_sentence() {
    let sentence = "Source 2 shows that the shell replies with results.";
    let mut candidate = lesson();
    candidate["blocks"][0]["body"] = json!(format!("The shell runs commands. {sentence}"));

    let findings = lesson_rules(&candidate);

    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].kind, FindingKind::Quotation);
    assert_eq!(findings[0].place().site, FindingSite::Section(0));
    assert_eq!(
        findings[0].summary,
        format!("This sentence names a source by its number, which learners never see: \u{201c}{sentence}\u{201d}")
    );
    assert_eq!(findings[0].sentence.as_deref(), Some(sentence));
    assert!(findings[0].note.contains("citations list"));
}

#[test]
fn a_source_named_in_a_question_is_a_finding_at_the_question() {
    let mut candidate = lesson();
    candidate["questions"][0]["prompt"] = json!("What does source 1 say the shell does?");
    let findings = lesson_rules(&candidate);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].place().site, FindingSite::Question(0));
    assert!(findings[0]
        .summary
        .starts_with("This sentence names a source by its number"));
}

#[test]
fn a_citation_label_line_is_one_finding_not_two() {
    let line = "Citation: Source 2 describes a root prompt and a user prompt.";
    let mut candidate = lesson();
    candidate["blocks"][0]["body"] = json!(format!("The shell runs commands.\n\n{line}"));

    let found = findings_of(&candidate);

    assert_eq!(
        found,
        [(
            format!(
                "This line is a citation written into the lesson's text: \u{201c}{line}\u{201d}"
            ),
            Some(line.to_owned())
        )]
    );
    for form in [
        "**Citation:** the manual",
        "- Citations: the manual",
        "**Citation**: the manual",
    ] {
        candidate["blocks"][0]["body"] = json!(format!("The shell runs commands.\n\n{form}"));
        assert_eq!(findings_of(&candidate).len(), 1, "{form}");
    }
}

#[test]
fn scaffolding_inside_a_code_fence_or_ordinary_prose_is_no_finding() {
    let mut candidate = lesson();
    candidate["blocks"][0]["body"] =
        json!("```text\nSource 2 shows that x.\nCitation: the manual\n```\n\nThe shell replies.");
    assert!(lesson_rules(&candidate).is_empty());
    candidate["blocks"][0]["body"] =
        json!("The second option is wrong. Two sources agree that the shell replies.");
    assert!(lesson_rules(&candidate).is_empty());
}

#[test]
fn labels_that_open_paragraphs_are_no_repeated_sentences() {
    let mut candidate = lesson();
    for (index, name) in [(0, "first"), (4, "fifth")] {
        candidate["blocks"][index]["body"] = json!(format!(
            "**Predict first.** Opening of the {name} section.\n\n**The model.** Model of the {name} section."
        ));
    }
    let findings = lesson_rules(&candidate);
    assert!(findings.is_empty(), "{findings:?}");
}

#[test]
fn the_steps_of_two_methods_are_no_repeated_sentences() {
    let mut candidate = lesson();
    candidate["blocks"][1]["body"] = json!("Method A\n\n1. Type `pwd`.\n2. Expected output: `/home/learner/Music`.\n\nMethod B\n\n1. Type `pwd`.\n2. Expected output: `/home/learner/Music`.");
    let findings = lesson_rules(&candidate);
    assert!(findings.is_empty(), "{findings:?}");
}

#[test]
fn a_prose_sentence_after_a_label_is_still_compared() {
    const SHARED: &str = "You change the location with cd and check it with pwd.";
    let mut candidate = lesson();
    candidate["blocks"][1]["body"] = json!(format!("**Case.** Opening. {SHARED}"));
    candidate["blocks"][4]["body"] = json!(format!("**Case.** Fresh start here. {SHARED}"));
    let findings = lesson_rules(&candidate);
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].place().site, FindingSite::Section(4));
    assert_eq!(findings[0].passage.as_deref(), Some(SHARED));
}

#[test]
fn what_stands_under_a_step_after_its_fence_is_no_repeated_sentence() {
    let method = |name: &str| {
        format!(
            "{name}\n\n1. Run:\n   ```\n   pwd\n   ```\n   Expected output: `/home/learner/Music`."
        )
    };
    let mut candidate = lesson();
    candidate["blocks"][1]["body"] =
        json!(format!("{}\n\n{}", method("Method A"), method("Method B")));
    let findings = lesson_rules(&candidate);
    assert!(findings.is_empty(), "{findings:?}");
}

#[test]
fn a_second_paragraph_indented_under_an_item_is_no_repeated_sentence() {
    let mut candidate = lesson();
    candidate["blocks"][1]["body"] = json!("Method A\n\n- Step one.\n\n  Check the result with care.\n\nMethod B\n\n- Step two.\n\n  Check the result with care.");
    let findings = lesson_rules(&candidate);
    assert!(findings.is_empty(), "{findings:?}");
}

#[test]
fn lines_inside_tilde_fences_are_no_repeated_sentences() {
    let mut candidate = lesson();
    candidate["blocks"][1]["body"] = json!("~~~\nRepeat this line of text.\n~~~\n\nUnrelated opening.\n\n~~~\nRepeat this line of text.\n~~~");
    let findings = lesson_rules(&candidate);
    assert!(findings.is_empty(), "{findings:?}");
}

#[test]
fn prose_after_a_list_has_ended_is_compared_again() {
    const STEP: &str =
        "1. Run:\n   ```\n   pwd\n   ```\n   Expected output: `/home/learner/Music`.";
    const SAID: &str = "Say this one thing only once here.";
    let mut candidate = lesson();
    candidate["blocks"][1]["body"] = json!(format!("{SAID}\n\n{STEP}"));
    candidate["blocks"][4]["body"] = json!(format!("{STEP}\n\n{SAID}"));
    let findings = lesson_rules(&candidate);
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].place().site, FindingSite::Section(4));
    assert_eq!(findings[0].passage.as_deref(), Some(SAID));
}

#[test]
fn an_apostrophe_is_no_quoted_wording_to_cite() {
    for sentence in [
        "Run pwd before ls so you see the shell's folder rather than the window's.",
        "Don't run rm until you've checked the folder's contents with ls.",
        "Run pwd before ls so you see the shell\u{2019}s folder rather than the window\u{2019}s.",
    ] {
        let mut candidate = lesson();
        candidate["blocks"][10]["body"] = json!(sentence);
        assert!(lesson_rules(&candidate).is_empty(), "{sentence}");
    }
}

#[test]
fn a_plural_possessive_is_no_quoted_wording_to_cite() {
    for sentence in [
        "The users' files and the admins' settings stay apart.",
        "The users\u{2019} files and the admins\u{2019} settings stay apart.",
        "The manual says 'rm deletes files permanently'.",
    ] {
        let mut candidate = lesson();
        candidate["blocks"][10]["body"] = json!(sentence);
        assert!(lesson_rules(&candidate).is_empty(), "{sentence}");
    }
}

#[test]
fn a_real_quotation_is_still_quoted_wording_to_cite() {
    for sentence in [
        "The manual says \"rm deletes files permanently, with no undo\".",
        "Remember \"the shell keeps a working directory\" always.",
    ] {
        let mut candidate = lesson();
        candidate["blocks"][10]["body"] = json!(sentence);
        let findings = lesson_rules(&candidate);
        assert_eq!(findings.len(), 1, "{sentence}");
        assert!(findings[0].note.contains("without citing it"), "{sentence}");
    }
}

#[test]
fn a_sentence_that_only_names_a_source_is_no_specific_to_cite() {
    for sentence in [
        "The documentation is available without leaving the program.",
        "According to the survey, most readers skim.",
    ] {
        let mut candidate = lesson();
        candidate["blocks"][10]["body"] = json!(sentence);
        assert!(lesson_rules(&candidate).is_empty(), "{sentence}");
    }
}

const MISCONCEPTION: &str =
    "Tempting wrong guess: \"the projects folder is where notes.txt always lives\".";

#[test]
fn a_misconception_in_quotation_marks_is_told_to_drop_them() {
    let mut candidate = lesson();
    candidate["blocks"][10]["body"] = json!(MISCONCEPTION);

    let findings = lesson_rules(&candidate);

    assert_eq!(findings.len(), 1);
    assert!(findings[0].note.contains("without citing it"));
    assert!(findings[0]
        .note
        .contains("remove the quotation marks and write it as the lesson's own sentence"));
}

#[test]
fn a_misconception_without_quotation_marks_is_no_finding() {
    let mut candidate = lesson();
    candidate["blocks"][10]["body"] =
        json!("Tempting wrong guess: the projects folder is where notes.txt always lives.");
    assert!(lesson_rules(&candidate).is_empty());
}

fn question_case(prompt: &str, options: [&str; 4], key: usize) -> Value {
    let rationales: Vec<&str> = (0..4)
        .map(|at| {
            if at == key {
                ""
            } else {
                "This reflects a plausible misconception and fails."
            }
        })
        .collect();
    let mut candidate = lesson();
    candidate["questions"][0] = json!({"kind":"practice","prompt":prompt,
        "options":options,"correctIndex":key,
        "rationales":rationales,"explanation":"Because."});
    candidate
}

fn first_question_notes(candidate: &Value) -> Vec<String> {
    lesson_rules(candidate)
        .into_iter()
        .filter(|finding| finding.place().site == FindingSite::Question(0))
        .map(|finding| finding.note)
        .collect()
}

#[test]
fn a_right_choice_much_longer_than_the_others_is_a_finding_at_its_question() {
    let candidate = question_case(
        "Which command shows hidden files?",
        [
            "ls",
            "ls -a, which lists every entry in the directory, including the ones whose names begin with a dot",
            "pwd",
            "cd",
        ],
        1,
    );
    let notes = first_question_notes(&candidate);
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].contains("Shorten the right choice"), "{notes:?}");
}

fn lengths_case(key_length: usize) -> Vec<String> {
    let options = [
        "a".repeat(45),
        "b".repeat(28),
        "c".repeat(key_length),
        "d".repeat(45),
    ];
    let candidate = question_case(
        "Which one is right?",
        options.each_ref().map(String::as_str),
        2,
    );
    first_question_notes(&candidate)
}

#[test]
fn a_long_right_choice_is_told_the_measured_lengths_and_its_limit() {
    let notes = lengths_case(73);
    assert_eq!(notes.len(), 1, "{notes:?}");
    for expected in ["73", "45", "72 characters or fewer"] {
        assert!(notes[0].contains(expected), "{expected}: {notes:?}");
    }
}

#[test]
fn a_right_choice_exactly_at_its_limit_is_no_finding() {
    let notes = lengths_case(72);
    assert!(notes.is_empty(), "{notes:?}");
}

#[test]
fn a_right_choice_repeating_the_prompts_words_is_a_finding_at_its_question() {
    let candidate = question_case(
        "Which command opens the notes folder inside the learner home?",
        [
            "opens the notes folder",
            "Lists the files there",
            "Deletes it for good",
            "Prints the current location",
        ],
        0,
    );
    let notes = first_question_notes(&candidate);
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].contains("gives the answer away"), "{notes:?}");
}

#[test]
fn path_like_choices_break_no_question_rule() {
    let candidate = question_case(
        "Which command enters the folder one level up?",
        ["cd notes", "cd ../../notes", "cd /notes", "cd ../notes"],
        3,
    );
    assert!(first_question_notes(&candidate).is_empty());
}

#[test]
fn a_span_between_double_quotes_under_the_quotation_floor_needs_no_citation() {
    let mut candidate = lesson();
    candidate["blocks"][10]["body"] = json!("The tool shows \"Changes saved\" when it is done.");
    assert!(lesson_rules(&candidate).is_empty());
}

#[test]
fn the_prose_between_two_short_quotations_needs_no_citation() {
    let mut candidate = lesson();
    candidate["blocks"][10]["body"] = json!(
        "Choose \"Open\" from the menu and then pick \"Save as copy\" from the list that opens."
    );
    assert!(lesson_rules(&candidate).is_empty());
}

const OWN_CASE: &str = "Dividing 12 by 48 gives 0.25.";

fn citation_findings(candidate: &Value) -> Vec<ReviewFinding> {
    lesson_rules(candidate)
        .into_iter()
        .filter(|finding| finding.kind == FindingKind::Quotation)
        .collect()
}

#[test]
fn a_figure_without_a_citation_is_not_a_citation_finding() {
    let mut section = lesson();
    section["blocks"][0]["body"] = json!(OWN_CASE);
    let found = citation_findings(&section);
    assert!(found.is_empty(), "section: {found:?}");

    let mut explanation = lesson();
    explanation["questions"][0]["explanation"] = json!(OWN_CASE);
    let found = citation_findings(&explanation);
    assert!(found.is_empty(), "answer explanation: {found:?}");

    let mut reason = lesson();
    reason["questions"][0]["rationales"][1] = json!("This divides 12 by 48 and gets 0.25.");
    let found = citation_findings(&reason);
    assert!(found.is_empty(), "choice reason: {found:?}");
}

#[test]
fn a_cited_sentence_s_quote_need_not_hold_its_figures() {
    let mut candidate = lesson();
    candidate["blocks"][0]["body"] = json!(OWN_CASE);
    candidate["blocks"][0]["citations"] = json!([{
        "sentence":OWN_CASE,"sourceIndex":0,
        "quote":"A ratio compares a part with a whole."}]);
    let found = citation_findings(&candidate);
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn quoted_wording_without_a_citation_is_still_a_finding_and_names_no_general_form() {
    let mut candidate = lesson();
    candidate["blocks"][0]["body"] =
        json!("The page reads \"Total due on receipt of goods\" at the foot.");
    let found = citation_findings(&candidate);
    assert_eq!(found.len(), 1, "{found:?}");
    let note = &found[0].note;
    assert!(
        note.starts_with("Section 1 (candidate blocks-0) states a specific in \""),
        "{note}"
    );
    assert!(note.contains("\" without citing it."), "{note}");
    assert!(!note.contains("general form"), "{note}");
}
