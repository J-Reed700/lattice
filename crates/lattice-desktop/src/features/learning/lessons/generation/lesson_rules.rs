//! The lesson's own rules beyond its schema: distinct, titled sections in the
//! mix the lesson is written for, checks that hold their answer back, an
//! order that tests what was taught and recalls before it recaps, and
//! distinct, answerable questions of every kind, and a specific that stands
//! only in a sentence the writer cited. A broken rule is a finding:
//! the teaching repair is given it, and what the repairs cannot settle waits
//! for the user. It never stops the lesson from publishing.
use super::{normalize, validate_text};
use crate::features::learning::content_verification::{
    figures, needs_citation, Givens, CITED_KINDS,
};
use crate::features::learning::review_findings::{FindingKind, FindingSite, ReviewFinding};
use lattice_core::shared::text::sentences;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// The heading a check section's reveal starts under (learning science
/// lessons §3.3): the learner commits to an answer to what is above it
/// before what is below it is shown. `checkReveal.ts` splits on the same line.
pub(in crate::features::learning) const CHECK_REVEAL: &str = "### Check your thinking";

/// The least a check holds on either side of its reveal: a case to reason
/// through above it, an answer with its reasoning below.
const CHECK_PART_MIN: usize = 80;

/// The sections a lesson is written with: at least this many of each kind,
/// or exactly this many when `exact`.
const SECTION_MIX: [(&str, usize, bool); 8] = [
    ("explanation", 2, false),
    ("worked_example", 2, false),
    ("contrast", 1, false),
    ("check", 2, false),
    ("guided_practice", 1, true),
    ("independent_practice", 1, true),
    ("reflection", 1, true),
    ("recap", 1, true),
];

/// A check's case and its reveal, split at its `CHECK_REVEAL` line: `None`
/// unless exactly one line is that heading (at level two to four). Claim
/// extraction uses it to know a reveal's misconceptions are named as wrong.
pub(in crate::features::learning) fn split_check(body: &str) -> Option<(&str, &str)> {
    let is_reveal = |line: &str| {
        let line = line.trim();
        let level = line.chars().take_while(|c| *c == '#').count();
        (2..=4).contains(&level)
            && line
                .get(level..)
                .is_some_and(|title| title.trim().eq_ignore_ascii_case("Check your thinking"))
    };
    let mut offset = 0;
    let mut found = None;
    for line in body.split_inclusive('\n') {
        if is_reveal(line) {
            if found.is_some() {
                return None;
            }
            found = Some((offset, offset + line.len()));
        }
        offset += line.len();
    }
    let (start, end) = found?;
    Some((body.get(..start)?, body.get(end..)?))
}

/// Questions the writer is asked for of each kind; a lesson is saved with
/// one of each (`LessonQuestionKind::ALL`).
const QUESTIONS_PER_KIND: usize = 2;

fn text<'a>(item: &'a Value, field: &str) -> &'a str {
    item.get(field).and_then(Value::as_str).unwrap_or_default()
}

fn structure(site: FindingSite, note: String) -> ReviewFinding {
    ReviewFinding::new(FindingKind::Structure, Some(site), note)
}

/// A citation a part lacks. Not structure: it does not hold up the content
/// review, and the repair that follows settles it by appending the citation.
fn citation(site: FindingSite, note: String) -> ReviewFinding {
    ReviewFinding::new(FindingKind::Quotation, Some(site), note)
}

/// `sentence` without the Markdown that opens it, a heading's hashes or a
/// list item's marker, which is no part of what it states.
fn without_marker(sentence: &str) -> &str {
    let text = sentence.trim_start_matches('#').trim_start();
    if let Some(rest) = ["- ", "* ", "+ "]
        .iter()
        .find_map(|bullet| text.strip_prefix(bullet))
    {
        return rest.trim_start();
    }
    let digits = text.chars().take_while(char::is_ascii_digit).count();
    let rest = text.get(digits..).unwrap_or_default();
    if digits > 0 && (rest.starts_with(". ") || rest.starts_with(") ")) {
        return rest.get(2..).unwrap_or_default().trim_start();
    }
    text
}

/// `text` without its code fences.
fn prose(text: &str) -> String {
    let mut prose = String::new();
    let mut fenced = false;
    for line in text.split_inclusive('\n') {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        } else if !fenced {
            prose.push_str(line);
        }
    }
    prose
}

/// The sentences of `text` that teach: its prose outside code fences, without
/// headings (claim classes design §3.10).
pub(in crate::features::learning) fn teaching_sentences(text: &str) -> Vec<String> {
    let prose = prose(text);
    sentences(&prose)
        .into_iter()
        .filter_map(|(from, to)| prose.get(from..to))
        .filter(|sentence| !sentence.starts_with('#'))
        .map(|sentence| without_marker(sentence.trim()).to_owned())
        .filter(|sentence| sentence.chars().any(char::is_alphabetic))
        .collect()
}

/// The sentences of `text` outside its code fences, as a writer lists them.
fn prose_sentences(text: &str) -> Vec<String> {
    let prose = prose(text);
    sentences(&prose)
        .into_iter()
        .filter_map(|(from, to)| prose.get(from..to))
        .map(|sentence| without_marker(sentence.trim()).to_owned())
        .filter(|sentence| sentence.chars().any(char::is_alphabetic))
        .collect()
}

/// The figures beyond its part's `givens` and the quoted wording of
/// `sentence`, which the quote cited for it must hold.
fn specifics(sentence: &str, givens: &Givens) -> Vec<String> {
    let own = figures(sentence)
        .filter(|figure| !givens.holds(figure))
        .map(str::to_owned);
    let quoted = lazy_regex::regex!(r#"['"‘’“”]([^'"‘’“”]{12,})['"‘’“”]"#)
        .captures_iter(sentence)
        .filter_map(|captures| captures.get(1))
        .map(|found| found.as_str().to_owned());
    own.chain(quoted).collect()
}

fn same_sentence(a: &str, b: &str) -> bool {
    let (a, b) = (normalize(a), normalize(b));
    !a.is_empty() && !b.is_empty() && (a == b || a.contains(&b) || b.contains(&a))
}

/// A specific stands only in a sentence the writer lists in the part's
/// citations, with a quote that contains it. Otherwise it is cited, when a
/// supplied excerpt states it, or the part is written in its general form,
/// when none does (claim classes design §3.4, §3.5). Code is exempt.
fn uncited_specifics(
    item: &Value,
    field: &str,
    (site, label): (FindingSite, &str),
    findings: &mut Vec<ReviewFinding>,
) {
    let path = match site {
        FindingSite::Section(index) => format!("/blocks/{index}"),
        FindingSite::Question(index) => format!("/questions/{index}"),
        FindingSite::Lesson => return,
    };
    let cited: Vec<(&str, &str)> = item
        .get("citations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|entry| (text(entry, "sentence"), text(entry, "quote")))
        .collect();
    let givens = Givens::of(item);
    for sentence in prose_sentences(text(item, field)) {
        if !needs_citation(&sentence, &givens) {
            continue;
        }
        let shown: String = sentence.chars().take(90).collect();
        let quotes: Vec<&str> = cited
            .iter()
            .filter(|(cited, _)| same_sentence(cited, &sentence))
            .map(|(_, quote)| *quote)
            .collect();
        let wanted = specifics(&sentence, &givens);
        let lacking = |quote: &str| -> Vec<String> {
            let held = normalize(quote);
            wanted
                .iter()
                .filter(|specific| !held.contains(&normalize(specific)))
                .cloned()
                .collect()
        };
        let note = if quotes.is_empty() {
            format!("{label} states a specific in \"{shown}\" without citing it. Cite it when a supplied excerpt states it: add an entry to the citations of {path} with the sentence, the excerpt's sourceIndex and an exact quote of the excerpt that contains the figure or wording. Write the general form only when no supplied excerpt does.")
        } else {
            let missing = quotes
                .iter()
                .map(|quote| lacking(quote))
                .min_by_key(Vec::len)
                .unwrap_or_default();
            if missing.is_empty() {
                continue;
            }
            format!("{label}: the quote cited for \"{shown}\" does not contain {}. Cite a quote of a supplied excerpt that does, in an entry of the citations of {path}, or write the general form when no excerpt states it.", missing.join(", "))
        };
        findings.push(citation(site, note));
    }
}

/// What `lesson_rules` reports, as its notes: for evals outside the crate,
/// which assert a reviewed lesson keeps the lesson's form.
pub fn lesson_rule_notes(candidate: &Value) -> Vec<String> {
    lesson_rules(candidate)
        .into_iter()
        .map(|finding| finding.note)
        .collect()
}

pub(in crate::features::learning) fn lesson_rules(candidate: &Value) -> Vec<ReviewFinding> {
    let items = |field| {
        candidate
            .get(field)
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
    };
    let (blocks, questions) = (items("blocks"), items("questions"));
    let mut findings = Vec::new();
    let mut titles = HashMap::new();
    for (index, block) in blocks.iter().enumerate() {
        let section = format!("Section {} (candidate blocks-{index})", index + 1);
        let title = text(block, "title");
        if !validate_text(title) {
            findings.push(structure(
                FindingSite::Section(index),
                format!("{section} needs a title."),
            ));
        } else if let Some(first) = titles.get(&normalize(title)) {
            findings.push(structure(
                FindingSite::Section(index),
                format!("{section} repeats the title of section {}; give each section a distinct title.", first + 1),
            ));
        } else {
            titles.insert(normalize(title), index);
        }
    }
    let kinds = |items: &[Value], kind: &str| {
        items
            .iter()
            .filter(|item| item.get("kind").and_then(Value::as_str) == Some(kind))
            .count()
    };
    let counts: Vec<_> = SECTION_MIX
        .iter()
        .map(|(kind, minimum, exact)| (*kind, kinds(blocks, kind), *minimum, *exact))
        .collect();
    if counts
        .iter()
        .any(|(_, count, minimum, exact)| *count < *minimum || (*exact && *count != *minimum))
    {
        let has: Vec<_> = counts
            .iter()
            .map(|(kind, count, _, _)| format!("{count} {}", kind.replace('_', " ")))
            .collect();
        findings.push(structure(
            FindingSite::Lesson,
            format!("The lesson needs at least two explanations, two worked examples, one contrast and two checks, and exactly one each of guided practice, independent practice, reflection and recap; it has {}.", has.join(", ")),
        ));
    }
    let kind_at = |index: usize| blocks.get(index).map(|block| text(block, "kind"));
    let first = |kind: &str| (0..blocks.len()).find(|index| kind_at(*index) == Some(kind));
    // Each check applies an idea a worked example has shown since the
    // previous check: two checks in a row, or a check before its idea's
    // example, test what the lesson has not yet worked through.
    let mut example_since_check = false;
    for (index, block) in blocks.iter().enumerate() {
        match text(block, "kind") {
            "check" => {}
            "worked_example" => {
                example_since_check = true;
                continue;
            }
            _ => continue,
        }
        let section = format!("Section {} (candidate blocks-{index})", index + 1);
        let held_back = split_check(text(block, "body")).is_some_and(|(case, reveal)| {
            case.trim().chars().count() >= CHECK_PART_MIN
                && reveal.trim().chars().count() >= CHECK_PART_MIN
        });
        if !held_back {
            findings.push(structure(
                FindingSite::Section(index),
                format!("{section} is a check: write the case to reason through first, then one line `{CHECK_REVEAL}`, then the answer, the reasoning and the misconception each plausible wrong answer reveals. Nothing above that line may give the answer away."),
            ));
        }
        if !example_since_check {
            findings.push(structure(
                FindingSite::Section(index),
                format!("{section} checks an idea no worked example has shown since the previous check; place each check after the example of the idea it asks the learner to apply."),
            ));
        }
        example_since_check = false;
    }
    if let (Some(reflection), Some(recap)) = (first("reflection"), first("recap")) {
        if reflection > recap {
            findings.push(structure(
                FindingSite::Section(recap),
                format!("Section {} (candidate blocks-{recap}) recaps before the reflection asks the learner to explain the lesson from memory; put the reflection first, so the recap is what they compare their explanation with.", recap + 1),
            ));
        }
    }
    let mut prompts = HashMap::new();
    for (index, question) in questions.iter().enumerate() {
        let label = format!("Question {}", index + 1);
        let site = FindingSite::Question(index);
        let prompt = text(question, "prompt");
        if !validate_text(prompt) {
            findings.push(structure(site, format!("{label} needs a prompt.")));
        } else if let Some(first) = prompts.get(&normalize(prompt)) {
            findings.push(structure(
                site,
                format!(
                    "{label} repeats question {}; ask something distinct.",
                    first + 1
                ),
            ));
        } else {
            prompts.insert(normalize(prompt), index);
        }
        if !validate_text(text(question, "explanation")) {
            findings.push(structure(
                site,
                format!("{label} needs an answer explanation."),
            ));
        }
        let options: Vec<_> = question
            .get("options")
            .and_then(Value::as_array)
            .map(|options| {
                options
                    .iter()
                    .map(|o| o.as_str().unwrap_or_default())
                    .collect()
            })
            .unwrap_or_default();
        let mut choices = HashSet::new();
        if options.len() != 4
            || options
                .iter()
                .any(|option| !validate_text(option) || !choices.insert(normalize(option)))
        {
            findings.push(structure(
                site,
                format!("{label} needs four distinct answer choices."),
            ));
        }
    }
    for (index, block) in blocks.iter().enumerate() {
        if CITED_KINDS.contains(&text(block, "kind")) {
            let label = format!("Section {} (candidate blocks-{index})", index + 1);
            uncited_specifics(
                block,
                "body",
                (FindingSite::Section(index), &label),
                &mut findings,
            );
        }
    }
    for (index, question) in questions.iter().enumerate() {
        let label = format!("Question {}", index + 1);
        uncited_specifics(
            question,
            "explanation",
            (FindingSite::Question(index), &label),
            &mut findings,
        );
    }
    let per_kind: Vec<_> = crate::features::learning::dto::LessonQuestionKind::ALL
        .iter()
        .map(|kind| (kind.as_str(), kinds(questions, kind.as_str())))
        .collect();
    if per_kind
        .iter()
        .any(|(_, count)| *count < QUESTIONS_PER_KIND)
    {
        let has: Vec<_> = per_kind
            .iter()
            .map(|(kind, count)| format!("{count} {kind}"))
            .collect();
        findings.push(structure(
            FindingSite::Lesson,
            format!("The lesson needs at least two distinct questions each for practice, quiz and test; it has {}.", has.join(", ")),
        ));
    }
    findings
}

#[cfg(test)]
mod tests {
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
            format!("{CASE}\n{CHECK_REVEAL}\nToo short."),
            format!("Too short.\n{CHECK_REVEAL}\n{REVEAL}"),
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
        crate::features::learning::lessons::text_edits::apply(&uncited_lesson(), &patch.to_string())
            .unwrap()
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
    fn a_block_title_longer_than_160_characters_or_of_two_letters_is_no_finding() {
        let mut candidate = lesson();
        candidate["blocks"][2]["title"] = json!("A title that runs on and on. ".repeat(10));
        candidate["blocks"][3]["title"] = json!("Go");
        assert!(lesson_rules(&candidate).is_empty());
        candidate["blocks"][3]["title"] = json!("   ");
        assert_eq!(lesson_rules(&candidate).len(), 1);
    }
}
