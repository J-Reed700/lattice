#![allow(clippy::indexing_slicing, clippy::unwrap_used)]
use super::*;
use serde_json::json;

fn block(kind: &str, title: &str) -> Value {
    let body = if kind == "check" {
        format!("{CASE}\n\n{CHECK_REVEAL}\n\n{REVEAL}")
    } else {
        "Body.".to_owned()
    };
    json!({"kind":kind,"title":title,"body":body})
}

const CASE: &str = "A sealed jar of air is carried from the beach up a mountain. Predict what happens to its lid, and say why.";
const REVEAL: &str = "The lid bulges: outside pressure falls with altitude while the air inside keeps its pressure. If you said it caves in, you reversed which side lost pressure.";

fn question(kind: &str, prompt: &str) -> Value {
    json!({"kind":kind,"prompt":prompt,"options":["One","Two","Three","Four"],"explanation":"Because."})
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
fn each_broken_rule_is_a_finding_at_its_place() {
    let mut candidate = lesson();
    candidate["blocks"][4]["title"] = json!("first  IDEA");
    candidate["blocks"][10]["kind"] = json!("explanation");
    candidate["questions"][3]["prompt"] = json!("q1?");
    candidate["questions"][4]["options"] = json!(["Yes", "yes", "Maybe", "No way"]);
    let findings = lesson_rules(&candidate);
    let sites: Vec<_> = findings.iter().map(|f| f.key().site).collect();
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
        format!("{CASE}\n{CHECK_REVEAL}\n{REVEAL}\n{CHECK_REVEAL}\n{REVEAL}"),
    ] {
        let mut candidate = lesson();
        candidate["blocks"][3]["body"] = json!(body);
        let findings = lesson_rules(&candidate);
        assert_eq!(findings.len(), 1, "{body}");
        assert_eq!(findings[0].key().site, FindingSite::Section(3));
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
    let sites: Vec<_> = findings.iter().map(|f| f.key().site).collect();
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
    let sites: Vec<_> = findings.iter().map(|f| f.key().site).collect();
    assert_eq!(sites, [FindingSite::Section(5)]);
}

const TIMEOUT: &str = "The default timeout is 30 seconds.";

fn cited(candidate: &mut Value, quote: &str) {
    candidate["blocks"][0]["citations"] =
        json!([{"sentence":TIMEOUT,"sourceIndex":0,"quote":quote}]);
}

/// A specific stands only in a sentence the writer cited.
#[test]
fn a_specific_without_a_citation_is_a_finding_at_its_section() {
    let mut candidate = lesson();
    candidate["blocks"][0]["body"] = json!(format!("{TIMEOUT} It is set for each request."));

    let findings = lesson_rules(&candidate);

    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].key().site, FindingSite::Section(0));
    assert_eq!(findings[0].kind, FindingKind::Quotation);
    assert!(findings[0].note.contains("without citing it"));
    assert!(findings[0].note.contains(TIMEOUT));
    assert!(findings[0].note.contains("citations of /blocks/0"));

    cited(
        &mut candidate,
        "A timeout of 30 seconds applies to each request.",
    );
    assert!(lesson_rules(&candidate).is_empty());
}

const LONG_SENTENCE: &str = "The default timeout for every outgoing request in the client library is 30 seconds unless the caller sets another value.";

/// What the user reads is the whole sentence, not the first 90
/// characters the note for the model keeps.
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
fn a_quote_that_lacks_the_figure_is_summarised_with_what_it_lacks_and_the_whole_sentence() {
    let mut candidate = lesson();
    candidate["blocks"][0]["body"] = json!(LONG_SENTENCE);
    candidate["blocks"][0]["citations"] = json!([{
        "sentence":LONG_SENTENCE,"sourceIndex":0,
        "quote":"A timeout applies to each request, in seconds."}]);

    let findings = lesson_rules(&candidate);

    assert_eq!(findings.len(), 1);
    assert_eq!(
        findings[0].summary,
        format!("The quotation cited for this sentence does not contain 30: \u{201c}{LONG_SENTENCE}\u{201d}")
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
fn a_quote_that_lacks_the_figure_does_not_cite_it() {
    let mut candidate = lesson();
    candidate["blocks"][0]["body"] = json!(TIMEOUT);
    cited(
        &mut candidate,
        "A timeout applies to each request, in seconds.",
    );

    let findings = lesson_rules(&candidate);

    assert_eq!(findings.len(), 1);
    assert!(findings[0].note.contains("does not contain 30"));
}

#[test]
fn an_attribution_needs_a_citation_but_no_figure_in_its_quote() {
    let mut candidate = lesson();
    let sentence = "The documentation describes a retry for failed requests.";
    candidate["blocks"][0]["body"] = json!(sentence);
    assert_eq!(lesson_rules(&candidate).len(), 1);

    candidate["blocks"][0]["citations"] = json!([{
        "sentence":sentence,"sourceIndex":0,
        "quote":"Failed requests are retried once, as the reference describes."}]);
    assert!(lesson_rules(&candidate).is_empty());
}

/// A worked example's case, code and a list's markers are not specifics a
/// source has to hold.
#[test]
fn hypothetical_cases_code_and_list_markers_need_no_citation() {
    let mut candidate = lesson();
    candidate["blocks"][1]["body"] = json!("Suppose a list holds 5 items and 3 are removed.");
    candidate["blocks"][0]["body"] = json!(
        "Read the file like this:\n```python\nrows = 100\nprint(rows)\n```\n1. Open the file.\n2) Read it row by row.\n- Close it."
    );
    assert!(lesson_rules(&candidate).is_empty());
}

/// An answer explanation reasons from its question's givens: the figures
/// its prompt and options state need no citation there, and a figure of
/// its own still does.
#[test]
fn an_answer_that_restates_its_questions_figures_needs_no_citation_for_them() {
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
    assert_eq!(lesson_rules(&candidate).len(), 1);
}

const SOURCE_SAYS: &str = "A timeout of 30 seconds applies to each request.";

fn uncited_lesson() -> Value {
    let mut candidate = lesson();
    candidate["blocks"][0]["body"] = json!(format!("{TIMEOUT} It is set for each request."));
    candidate["blocks"][0]["citations"] = json!([]);
    candidate
}

fn patch(patch: Value) -> Value {
    crate::lessons::text_edits::apply(&uncited_lesson(), &patch.to_string()).unwrap()
}

/// An uncited figure the supplied excerpt states ends cited, its sentence
/// as it was.
#[test]
fn an_uncited_figure_a_source_contains_is_settled_by_citing_it() {
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

/// One no excerpt states ends in the general form, with nothing cited.
#[test]
fn an_uncited_figure_no_source_contains_is_settled_by_the_general_form() {
    let revised = patch(json!({"edits":[
        {"path":"/blocks/0/body","before":TIMEOUT,"after":"The default timeout is set by the caller."}],
        "citations":[]}));

    assert!(lesson_rules(&revised).is_empty());
    assert_eq!(revised["blocks"][0]["citations"], json!([]));
}

/// A citation whose quote lacks the figure settles nothing: the quote
/// has to contain what the sentence states, whoever appended it.
#[test]
fn a_citation_appended_with_a_quote_that_lacks_the_figure_settles_nothing() {
    let revised = patch(json!({"edits":[],"citations":[
        {"path":"/blocks/0","sentence":TIMEOUT,"sourceIndex":0,
         "quote":"A timeout applies to each request, in seconds."}]}));

    let findings = lesson_rules(&revised);

    assert_eq!(findings.len(), 1);
    assert!(findings[0].note.contains("does not contain 30"));
    // A second citation whose quote holds the figure settles it.
    let mut again = revised;
    again["blocks"][0]["citations"]
        .as_array_mut()
        .unwrap()
        .push(json!({"sentence":TIMEOUT,"sourceIndex":0,"quote":SOURCE_SAYS}));
    assert!(lesson_rules(&again).is_empty());
}

#[test]
fn a_question_explanation_is_held_to_the_same_rule() {
    let mut candidate = lesson();
    candidate["questions"][2]["explanation"] = json!("The cache holds 128 entries.");

    let findings = lesson_rules(&candidate);

    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].key().site, FindingSite::Question(2));
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
    assert_eq!(findings[0].key().site, FindingSite::Question(1));
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
    candidate["blocks"][0]["body"] =
        json!("A long explanation of the idea, with no figures in it. ".repeat(120));
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
        assert_eq!(findings[0].key().site, FindingSite::Section(3));
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
    assert_eq!(findings[0].key().site, FindingSite::Section(0));
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
    assert_eq!(findings[0].key().site, FindingSite::Question(0));
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
fn an_explanation_naming_a_choice_by_its_number_is_a_finding() {
    let sentence = "Option 1 is wrong because it reverses the order.";
    let mut candidate = lesson();
    candidate["questions"][0]["explanation"] = json!(format!("Two follows. {sentence}"));

    let findings = lesson_rules(&candidate);

    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].key().site, FindingSite::Question(0));
    assert_eq!(
        findings[0].summary,
        format!("This explanation names a choice by its number, letter or position, and the choices are shuffled before learners see them: \u{201c}{sentence}\u{201d}")
    );
    assert_eq!(findings[0].sentence.as_deref(), Some(sentence));
}

#[test]
fn a_choice_named_by_number_in_a_prompt_or_an_option_is_no_choice_finding() {
    let mut candidate = lesson();
    candidate["questions"][0]["prompt"] = json!("Why is option 2 wrong here?");
    candidate["questions"][0]["options"] = json!(["Option 2 reverses it", "Two", "Three", "Four"]);
    assert!(!findings_of(&candidate)
        .iter()
        .any(|(summary, _)| summary.contains("names a choice by its number")));
}

fn explanation_findings(explanation: &str) -> Vec<ReviewFinding> {
    let mut candidate = lesson();
    candidate["questions"][0]["explanation"] = json!(explanation);
    lesson_rules(&candidate)
}

#[test]
fn an_explanation_naming_a_choice_by_its_letter_or_position_is_a_finding() {
    for explanation in [
        "Option B confuses ls with a system search.",
        "Answer C mistakes the parent folder for the working directory.",
        "That is the misconception behind the first choice.",
        "The second option is not supported by any excerpt.",
        "The last answer describes destructive behaviour.",
        "Choices 2 and 3 are relative paths.",
    ] {
        let findings = explanation_findings(explanation);
        assert_eq!(findings.len(), 1, "{explanation}");
        assert_eq!(findings[0].sentence.as_deref(), Some(explanation));
        assert!(
            findings[0].summary.contains("number, letter or position"),
            "{explanation}"
        );
    }
}

#[test]
fn an_explanation_naming_no_choice_by_letter_or_position_is_no_finding() {
    for explanation in [
        "The first .. moves you from /var/log to /var.",
        "An option a learner might pick is to ignore the error.",
        "Plan B is to use an absolute path.",
        "The second step creates the folder.",
        "Choosing the parent folder is the tempting wrong answer.",
    ] {
        assert!(
            explanation_findings(explanation).is_empty(),
            "{explanation}"
        );
    }
}

#[test]
fn a_choice_named_by_letter_or_position_in_a_prompt_or_an_option_is_no_choice_finding() {
    let mut candidate = lesson();
    candidate["questions"][0]["prompt"] = json!("Why is option B wrong here?");
    candidate["questions"][0]["options"] =
        json!(["The first choice reverses it", "Two", "Three", "Four"]);
    assert!(!findings_of(&candidate)
        .iter()
        .any(|(summary, _)| summary.contains("names a choice by its")));
}

#[test]
fn scaffolding_inside_a_code_fence_or_ordinary_prose_is_no_finding() {
    let mut candidate = lesson();
    candidate["blocks"][0]["body"] =
        json!("```text\nSource 2 shows that x.\nCitation: the manual\n```\n\nThe shell replies.");
    assert!(lesson_rules(&candidate).is_empty());
    candidate["blocks"][0]["body"] =
        json!("The second option is wrong. Two sources agree that the shell replies.");
    candidate["questions"][0]["explanation"] =
        json!("The parent folder is wrong because it reverses the order.");
    assert!(lesson_rules(&candidate).is_empty());
}
