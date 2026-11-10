use super::*;
use crate::dto::LearningProjectMilestoneDto;
use crate::lessons::course_generation_tests::length_bounds;

const TITLE: &str = "Experiment project milestone";
const BRIEF: &str = "Develop the next stage of the randomized experiment and revise it.";
const ITEM: &str = "A written protocol identifying the treatment and control groups.";

fn milestone(title: &str, brief: &str, item: &str) -> LearningProjectMilestoneDto {
    serde_json::from_value(json!({"title":title,"brief":brief,
        "deliverables":[item,item],"successCriteria":[item,item]}))
    .unwrap()
}

fn criterion(title: &str, description: &str) -> GeneratedCriterion {
    serde_json::from_value(json!({"title":title,"description":description,"dimension":"recall"}))
        .unwrap()
}

fn saved(title: &str, description: &str) -> LearningPracticeCriterionDto {
    LearningPracticeCriterionDto {
        id: uuid::Uuid::new_v4().to_string(),
        dimension: LearningPracticeEvidenceDimension::Recall,
        title: title.into(),
        description: description.into(),
        max_points: 4,
    }
}

#[test]
fn a_project_milestone_of_any_length_is_read_not_refused() {
    let cases = [
        (
            "a long title",
            milestone(&"A project title that runs on. ".repeat(10), BRIEF, ITEM),
        ),
        ("a short title", milestone("Plan", BRIEF, ITEM)),
        (
            "a long brief",
            milestone(TITLE, &"A brief that runs on and on. ".repeat(70), ITEM),
        ),
        ("a short brief", milestone(TITLE, "Build it.", ITEM)),
        (
            "a long item",
            milestone(TITLE, BRIEF, &"A deliverable that runs on. ".repeat(30)),
        ),
        ("a short item", milestone(TITLE, BRIEF, "A report.")),
    ];
    for (what, project) in &cases {
        let checked = validate_project(project);
        assert!(checked.is_ok(), "{what}: {:?}", checked.as_ref().err());
    }
    for blank in [
        milestone(" ", BRIEF, ITEM),
        milestone(TITLE, " ", ITEM),
        milestone(TITLE, BRIEF, " "),
    ] {
        assert!(validate_project(&blank).is_err());
    }
}

#[test]
fn a_rubric_criterion_of_any_length_is_read_not_refused() {
    let title = "A criterion title that runs on. ".repeat(10);
    let description = "A description of observable work that runs on. ".repeat(40);
    let made = rubric(
        vec![criterion("Hi", "Short."), criterion(&title, &description)],
        true,
    );
    assert!(made.is_ok(), "{:?}", made.as_ref().err());
    assert_eq!(made.unwrap().len(), 2);
    for blank in [criterion(" ", "A description."), criterion("Title", " ")] {
        let other = criterion("Another", "Another description.");
        assert!(rubric(vec![blank, other], true).is_err());
    }
}

#[test]
fn a_saved_criterion_of_any_length_is_read_not_refused() {
    let title = "A criterion title that runs on. ".repeat(10);
    let description = "A description of observable work that runs on. ".repeat(40);
    let checked = validate_saved_rubric(&[saved("Hi", "Short."), saved(&title, &description)]);
    assert!(checked.is_ok(), "{:?}", checked.as_ref().err());
    assert!(validate_saved_rubric(&[saved(" ", "A description.")]).is_err());
    assert!(validate_saved_rubric(&[saved("Title", " ")]).is_err());
}

#[test]
fn the_project_schema_bounds_no_text_but_a_blank() {
    let bounds = length_bounds(&project_schema());
    assert!(bounds.is_empty(), "{bounds:?}");
}

#[test]
fn the_rubric_schema_bounds_no_text_but_a_blank() {
    let bounds = length_bounds(&rubric_schema());
    assert!(bounds.is_empty(), "{bounds:?}");
}

fn structure(blocks: Value) -> Vec<ReviewFinding> {
    let candidate = json!({"blocks":blocks,"questions":[]});
    structural_findings(
        &json!({"type":"object"}),
        &candidate.to_string(),
        crate::generation::Counts::Asked,
    )
    .unwrap()
}

#[test]
fn a_block_of_any_length_is_no_finding_for_its_length() {
    let rubric = json!([
        {"title":"One","description":"Observable."},
        {"title":"Two","description":"Observable."}
    ]);
    let blocks: Vec<_> = [
        "explanation",
        "worked_example",
        "contrast",
        "check",
        "guided_practice",
        "independent_practice",
        "reflection",
        "recap",
    ]
    .iter()
    .map(|kind| json!({"kind":kind,"title":"Part","body":"Short.","rubric":rubric}))
    .collect();
    let found = structure(json!(blocks));
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_block_without_a_body_is_a_finding_at_its_place() {
    let found = structure(json!([
        {"kind":"explanation","title":"Fine","body":"A crisp, correct explanation."},
        {"kind":"recap","title":"Empty","body":"  "},
        {"kind":"guided_practice","title":"Bare","rubric":[]}
    ]));
    let notes: Vec<_> = found.iter().map(|finding| finding.note.as_str()).collect();
    assert_eq!(
        notes,
        [
            "Teaching block 2 has no body.",
            "Teaching block 3 has no body.",
            "Practice block 3 needs two to four observable, task-specific rubric criteria."
        ]
    );
    let sites: Vec<_> = found.iter().map(|finding| finding.key().site).collect();
    assert_eq!(
        sites,
        [
            FindingSite::Section(1),
            FindingSite::Section(2),
            FindingSite::Section(2)
        ]
    );
}

#[test]
fn a_block_without_a_body_or_a_rubric_is_named_in_plain_words() {
    let found = structure(json!([
        {"kind":"explanation","title":"Fine","body":"A crisp, correct explanation."},
        {"kind":"recap","title":"Empty","body":"  "},
        {"kind":"guided_practice","title":"Bare","rubric":[]}
    ]));
    let summaries: Vec<_> = found
        .iter()
        .map(|finding| finding.summary.as_str())
        .collect();
    assert_eq!(
        summaries,
        [
            "Section 2 has no text.",
            "Section 3 has no text.",
            "Section 3 is a practice section and needs two to four clear criteria for judging an answer."
        ]
    );
}

#[test]
fn material_that_cannot_be_read_is_named_in_plain_words() {
    let found = structural_findings(
        &json!({"type":"object"}),
        "not json",
        crate::generation::Counts::Asked,
    )
    .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].summary, "The lesson could not be read as a whole.");
}

#[test]
fn a_quotation_that_matches_no_saved_reference_shows_the_quote_that_failed() {
    let found = quotation_findings(&[
        json!({"path":"blocks[2]","quote":"A span the source does not hold.","error":"it was not found."}),
        json!({"path":"questions[0]","quote":"","error":"it was not found."}),
    ]);
    let passages: Vec<_> = found
        .iter()
        .map(|finding| finding.passage.as_deref())
        .collect();
    assert_eq!(passages, [Some("A span the source does not hold."), None]);
}

#[test]
fn a_quotation_that_matches_no_saved_reference_is_named_by_its_part() {
    let found = quotation_findings(&[
        json!({"path":"blocks[2]","error":"it was not found."}),
        json!({"path":"questions[0]","error":"it was not found."}),
        json!({"path":"requirements","error":"it was not found."}),
    ]);
    let summaries: Vec<_> = found
        .iter()
        .map(|finding| finding.summary.as_str())
        .collect();
    assert_eq!(
        summaries,
        [
            "The quotation cited in section 3 could not be found in its source.",
            "The quotation cited in question 1 could not be found in its source.",
            "The quotation cited in the lesson could not be found in its source."
        ]
    );
}
