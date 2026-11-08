//! The lesson brief a drill is built from: the units of evidence a lesson's text is cut into, the brief as a model is
//! shown it, and the link an issue makes to the evidence it cites.

use super::*;

fn unit(text: &str, source: Option<&str>) -> LessonSentence {
    LessonSentence {
        text: text.to_owned(),
        source: source.map(str::to_owned),
    }
}

fn brief(evidence: Vec<LessonSentence>) -> LessonBrief {
    LessonBrief {
        lesson_id: "lesson-1".to_owned(),
        course: "Memory science".to_owned(),
        goal: "Learn how memory works".to_owned(),
        prior_knowledge: "Some psychology".to_owned(),
        module: "Spacing".to_owned(),
        module_summary: "How gaps help recall".to_owned(),
        outcomes: vec![
            "Explain the spacing effect".to_owned(),
            "Plan a schedule".to_owned(),
        ],
        lesson: "Spacing study over sessions".to_owned(),
        objective: "Explain why spreading study helps".to_owned(),
        evidence,
    }
}

fn strings(texts: &[&str]) -> Vec<String> {
    texts.iter().map(|text| (*text).to_owned()).collect()
}

// The units of a lesson's text.

#[test]
fn prose_is_cut_into_its_sentences_trimmed_with_no_empty_unit() {
    let text = "  Spacing raises recall. Gaps matter most.\n\nTesting helps too.  \n";
    assert_eq!(
        evidence_units(text),
        strings(&[
            "Spacing raises recall.",
            "Gaps matter most.",
            "Testing helps too."
        ])
    );
    assert!(evidence_units("").is_empty());
    assert!(evidence_units("  \n\n \t").is_empty());
}

#[test]
fn a_fenced_block_is_one_unit_with_its_fences_and_the_prose_around_it_is_cut() {
    let text = "Spacing raises recall. Gaps matter.\n\n```python\nschedule(3)\n\nreview()\n```\n\nThe gap grows. Reviews get rarer.\n";
    assert_eq!(
        evidence_units(text),
        strings(&[
            "Spacing raises recall.",
            "Gaps matter.",
            "```python\nschedule(3)\n\nreview()\n```",
            "The gap grows.",
            "Reviews get rarer."
        ])
    );
}

#[test]
fn a_tilde_fence_and_a_fence_never_closed_are_blocks_too() {
    assert_eq!(
        evidence_units("Before it.\n~~~js\nlet x = 1;\n~~~\nAfter it."),
        strings(&["Before it.", "~~~js\nlet x = 1;\n~~~", "After it."])
    );
    assert_eq!(
        evidence_units("Before it.\n```\nlet x = 1;\nmore code\n"),
        strings(&["Before it.", "```\nlet x = 1;\nmore code"])
    );
}

#[test]
fn a_backtick_fence_inside_an_open_tilde_block_is_code_not_a_close() {
    assert_eq!(
        evidence_units("~~~text\nline one\n```\nline two\n~~~\nAfter it."),
        strings(&["~~~text\nline one\n```\nline two\n~~~", "After it."])
    );
}

#[test]
fn a_fence_indented_by_four_spaces_is_not_a_fence() {
    let units = evidence_units("Intro here.\n\n    ```python\n    x = 1\n    ```\n\nOutro here.");
    assert!(units.contains(&"Intro here.".to_owned()));
    assert!(units.contains(&"Outro here.".to_owned()));
    assert!(units.contains(&"x = 1".to_owned()));
    assert!(units.iter().all(|unit| unit.matches("```").count() < 2));
}

#[test]
fn a_crlf_text_gives_the_units_of_its_lf_form() {
    let lf = "Spacing raises recall. Gaps matter.\n\n```python\nschedule(3)\nreview()\n```\n\nThe gap grows.\n";
    let crlf = lf.replace('\n', "\r\n");
    let units = evidence_units(&crlf);
    assert_eq!(units, evidence_units(lf));
    assert_eq!(units.len(), 4);
    assert_eq!(units[2], "```python\nschedule(3)\nreview()\n```");
    assert!(units.iter().all(|unit| !unit.contains('\r')));
}

// The brief as a model is shown it.

#[test]
fn a_full_brief_shows_every_header_line_then_the_numbered_evidence() {
    let shown = brief(vec![
        unit("Spacing raises recall.", None),
        unit("Gaps matter.", None),
        unit("Recall rose by 14%.", Some("Spacing and memory")),
    ])
    .render();
    assert_eq!(
        shown,
        "<brief>\n\
Course: Memory science\n\
Course goal: Learn how memory works\n\
What the learner already knows: Some psychology\n\
Module: Spacing\n\
Module summary: How gaps help recall\n\
Module outcomes:\n\
- Explain the spacing effect\n\
- Plan a schedule\n\
Lesson: Spacing study over sessions\n\
Lesson objective: Explain why spreading study helps\n\
\n\
Evidence:\n\
From the lesson:\n\
L1 Spacing raises recall.\n\
L2 Gaps matter.\n\
From Spacing and memory:\n\
L3 Recall rose by 14%.\n\
</brief>"
    );
}

#[test]
fn a_heading_comes_before_the_first_unit_and_before_each_unit_whose_source_differs_from_the_one_before(
) {
    let shown = brief(vec![
        unit("One.", None),
        unit("Two.", None),
        unit("Three.", Some("A")),
        unit("Four.", Some("A")),
        unit("Five.", Some("B")),
        unit("Six.", None),
    ])
    .render();
    let evidence: Vec<&str> = shown
        .lines()
        .skip_while(|line| *line != "Evidence:")
        .skip(1)
        .take_while(|line| *line != "</brief>")
        .collect();
    assert_eq!(
        evidence,
        [
            "From the lesson:",
            "L1 One.",
            "L2 Two.",
            "From A:",
            "L3 Three.",
            "L4 Four.",
            "From B:",
            "L5 Five.",
            "From the lesson:",
            "L6 Six."
        ]
    );
}

#[test]
fn a_unit_of_several_lines_starts_on_the_line_after_its_number() {
    let shown = brief(vec![
        unit("Spacing raises recall.", None),
        unit("```python\nschedule(3)\n```", None),
        unit("Gaps matter.", None),
    ])
    .render();
    assert!(shown.contains(
        "L1 Spacing raises recall.\nL2\n```python\nschedule(3)\n```\nL3 Gaps matter.\n</brief>"
    ));
}

#[test]
fn a_brief_with_no_evidence_has_no_evidence_lines() {
    let shown = brief(Vec::new()).render();
    assert!(!shown.contains("Evidence:"));
    assert!(!shown.contains("From the lesson"));
    assert!(shown.starts_with("<brief>\nCourse: Memory science\n"));
    assert!(shown.ends_with("Lesson objective: Explain why spreading study helps\n</brief>"));
}

#[test]
fn blank_values_and_blank_outcomes_leave_their_lines_out() {
    let mut thin = brief(Vec::new());
    thin.goal = "  ".to_owned();
    thin.prior_knowledge = String::new();
    thin.module_summary = "\n".to_owned();
    thin.outcomes = vec![" ".to_owned(), "Plan a schedule".to_owned(), String::new()];
    assert_eq!(
        thin.render(),
        "<brief>\n\
Course: Memory science\n\
Module: Spacing\n\
Module outcomes:\n\
- Plan a schedule\n\
Lesson: Spacing study over sessions\n\
Lesson objective: Explain why spreading study helps\n\
</brief>"
    );
    thin.outcomes = vec![" ".to_owned()];
    assert!(!thin.render().contains("Module outcomes:"));
    thin.course = " ".to_owned();
    thin.module = String::new();
    thin.lesson = "\t".to_owned();
    thin.objective = String::new();
    assert_eq!(thin.render(), "<brief>\n</brief>");
}

// The link of an issue.

#[test]
fn a_link_copies_the_cited_units_with_their_sources_in_the_order_cited() {
    let known = brief(vec![
        unit("One.", None),
        unit("Two.", Some("A")),
        unit("Three.", Some("B")),
    ]);
    let link = known.link(&[3, 1]).unwrap();
    assert_eq!(link.lesson_id, "lesson-1");
    assert_eq!(
        link.sentences,
        vec![unit("Three.", Some("B")), unit("One.", None)]
    );
}

#[test]
fn a_link_needs_a_number_of_the_evidence() {
    let known = brief(vec![unit("One.", None), unit("Two.", None)]);
    assert_eq!(known.link(&[]), None);
    assert_eq!(known.link(&[0]), None);
    assert_eq!(known.link(&[1, 3]), None);
    assert!(known.link(&[2]).is_some());
    assert_eq!(brief(Vec::new()).link(&[1]), None);
}

// The units an issue cites, as the evidence is shown.

#[test]
fn cited_units_keep_their_own_numbers_in_the_order_cited_with_a_heading_where_the_source_changes() {
    let known = brief(vec![
        unit("One.", None),
        unit("Two.", Some("A")),
        unit("Three.", Some("B")),
        unit("Four.", Some("B")),
    ]);
    assert_eq!(
        known.cited(&[3, 1]).as_deref(),
        Some("From B:\nL3 Three.\nFrom the lesson:\nL1 One.")
    );
    assert_eq!(
        known.cited(&[3, 4, 2]).as_deref(),
        Some("From B:\nL3 Three.\nL4 Four.\nFrom A:\nL2 Two.")
    );
    assert_eq!(known.cited(&[2]).as_deref(), Some("From A:\nL2 Two."));
}

#[test]
fn a_cited_unit_of_several_lines_starts_on_the_line_after_its_number() {
    let known = brief(vec![
        unit("One.", None),
        unit("```python\nschedule(3)\n```", None),
    ]);
    assert_eq!(
        known.cited(&[2, 1]).as_deref(),
        Some("From the lesson:\nL2\n```python\nschedule(3)\n```\nL1 One.")
    );
}

#[test]
fn nothing_is_cited_exactly_when_the_link_would_be_nothing() {
    let known = brief(vec![unit("One.", None), unit("Two.", None)]);
    for numbers in [&[][..], &[0], &[1, 3], &[3]] {
        assert_eq!(known.cited(numbers), None, "{numbers:?}");
        assert!(known.link(numbers).is_none(), "{numbers:?}");
    }
    assert_eq!(brief(Vec::new()).cited(&[1]), None);
    assert!(known.link(&[2]).is_some());
    assert!(known.cited(&[2]).is_some());
}

// The stored form.

#[test]
fn a_brief_round_trips_through_serde_with_camel_case_keys() {
    let known = brief(vec![unit("One.", None), unit("Two.", Some("A"))]);
    let stored = serde_json::to_value(&known).unwrap();
    assert_eq!(stored["lessonId"], "lesson-1");
    assert_eq!(stored["priorKnowledge"], "Some psychology");
    assert_eq!(stored["moduleSummary"], "How gaps help recall");
    assert_eq!(stored["evidence"][1]["source"], "A");
    assert!(stored["evidence"][0].get("source").is_none());
    assert_eq!(
        serde_json::from_value::<LessonBrief>(stored).unwrap(),
        known
    );
}
