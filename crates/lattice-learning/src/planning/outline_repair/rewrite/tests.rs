//! The module rewrite: what the model is shown, how its changes are applied to
//! the saved module and what refuses them.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::kinds::{
    About, Citation, Entry, Estimate, Lesson, Module, NewLesson, Place, Prerequisites, Row,
};
use super::*;
use crate::lessons::content_verification::tests::fixture_prompts;
use crate::lessons::course_generation_tests::request;
use futures::future::BoxFuture;
use lattice_core::application::ports::llm_port::CompletionResponse;
use lattice_core::application::services::model_contract::edit::{consistent, Anchor, Change};
use lattice_core::application::services::model_contract::shape::Why;
use lattice_core::application::services::model_contract::shown::{
    Handle, Internal, Kind, Picked, Told,
};
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
use lattice_core::application::services::model_contract::test_support::{ScriptedModel, Sent};
use lattice_core::shared::error::Result;

const GATE: &str = "The harbour gate opens at first light.";
const TABLES: &str = "Tide tables are revised each spring.";

fn module_json(name: &str, number: usize) -> Value {
    let cited = |text: &str, quote: &str| json!({"text":text,"sourceIndex":if quote.is_empty() {Value::Null} else {json!(0)},"quote":quote});
    let lesson = |at: usize, quote: &str| json!({"title":format!("{name}: lesson {at}"),"objective":format!("Chart tide {at} for {name}."),"estimatedMinutes":30,"sourceIndex":if quote.is_empty() {Value::Null} else {json!(0)},"quote":quote});
    json!({
        "prerequisiteIndices":if number == 0 {vec![]} else {vec![number - 1]},
        "project":{"title":format!("{name} project"),"brief":format!("Record the {name} readings."),"deliverables":[format!("A {name} log."),format!("A {name} chart.")],"successCriteria":[format!("The {name} log is complete."),format!("The {name} chart is legible.")]},
        "title":name,
        "summary":cited(&format!("Read the {name} tables."), GATE),
        "outcomes":[cited(&format!("Read a {name} table."), GATE),cited(&format!("Compare two {name} tables."), TABLES)],
        "lessons":[lesson(1, TABLES),lesson(2, GATE),lesson(3, "")],
    })
}

fn candidate() -> Value {
    json!({"modules":[module_json("Harbour tides", 0),module_json("Kiln firing", 1),module_json("Rota planning", 2)]})
}

fn asked_with(index: usize, findings: &[Finding]) -> Asked {
    Asked::of(
        &request(LearningCourseDepth::Course),
        &candidate(),
        index,
        findings,
        vec![json!({"sourceIndex":0,"title":"Tide notes","passages":[GATE]})],
    )
    .unwrap()
}

fn asked(index: usize) -> Asked {
    asked_with(index, &[])
}

fn pick<K: Kind>(handle: &Handle<K>, reference: K::Ref) -> Picked<K> {
    Picked {
        handle: handle.clone(),
        reference,
    }
}

/// The names a request about `asked` gives what it shows.
struct Names {
    rows: Vec<Picked<Row>>,
    lessons: Vec<Picked<Lesson>>,
    entries: Vec<Picked<Entry>>,
}

fn names(asked: &Asked) -> Names {
    let mut scope = Scope::for_question();
    let mut showing = scope.request();
    let rows = asked.module.rows();
    let index = *asked.index.value();
    let earlier: Vec<Entry> = asked
        .sequence
        .iter()
        .filter(|entry| entry.position < index)
        .cloned()
        .collect();
    Names {
        rows: showing
            .show(&rows)
            .unwrap()
            .iter()
            .map(|(handle, row)| pick(handle, row.reference()))
            .collect(),
        lessons: showing
            .show(&asked.module.lessons)
            .unwrap()
            .iter()
            .map(|(handle, lesson)| pick(handle, lesson.reference()))
            .collect(),
        entries: showing
            .show(&earlier)
            .unwrap()
            .iter()
            .map(|(handle, entry)| pick(handle, entry.reference()))
            .collect(),
    }
}

impl Names {
    fn row(&self, place: Place) -> Picked<Row> {
        self.rows
            .iter()
            .find(|row| row.reference == place)
            .unwrap()
            .clone()
    }

    fn lesson(&self, position: usize) -> Picked<Lesson> {
        self.lessons
            .iter()
            .find(|lesson| lesson.reference == position)
            .unwrap()
            .clone()
    }

    fn entry(&self, position: usize) -> Picked<Entry> {
        self.entries
            .iter()
            .find(|entry| entry.reference == position)
            .unwrap()
            .clone()
    }
}

fn nothing() -> Revision {
    changing(Change {
        replace: vec![],
        insert: vec![],
        remove: vec![],
        moved: vec![],
    })
}

fn changing(text: Change<Row, String>) -> Revision {
    Revision {
        text: consistent(text).unwrap(),
        new_lessons: vec![],
        minutes: vec![],
        prerequisites: Prerequisites {
            add: vec![],
            remove: vec![],
        },
    }
}

fn replacing(names: &Names, place: Place, with: &str) -> Revision {
    changing(Change {
        replace: vec![(names.row(place), with.to_owned())],
        insert: vec![],
        remove: vec![],
        moved: vec![],
    })
}

fn refusal_of(asked: &Asked, revision: &Revision) -> String {
    match applied(asked, revision) {
        Err(Refusal(text)) => text,
        Ok(_) => panic!("the change was applied"),
    }
}

fn new_lesson(after: Option<Picked<Lesson>>, title: &str) -> NewLesson {
    NewLesson {
        after,
        title: title.to_owned(),
        objective: format!("Explain {title}."),
        minutes: 20,
    }
}

fn json_of(module: &GeneratedModule) -> Value {
    serde_json::to_value(module).unwrap()
}

#[test]
fn a_module_written_back_from_its_rows_is_the_saved_module() {
    let saved = candidate()["modules"][1].clone();
    let generated: GeneratedModule = serde_json::from_value(saved.clone()).unwrap();
    let written = Module::of(&generated).generated();
    assert_eq!(json_of(&written), saved);
}

#[test]
fn an_answer_with_every_list_empty_leaves_the_module_as_saved() {
    let asked = asked(1);
    let result = applied(&asked, &nothing()).unwrap();
    assert_eq!(json_of(&result.module), candidate()["modules"][1]);
    assert_eq!(result.origins, [Some(0), Some(1), Some(2)]);
}

#[test]
fn a_reworded_outcome_loses_its_citation_and_the_outcome_beside_it_keeps_its_own() {
    let asked = asked(1);
    let names = names(&asked);
    let result = applied(
        &asked,
        &replacing(&names, Place::Outcome(0), "Read a kiln chart."),
    )
    .unwrap();
    let outcomes = json_of(&result.module)["outcomes"].clone();
    assert_eq!(
        outcomes[0],
        json!({"text":"Read a kiln chart.","sourceIndex":null,"quote":""})
    );
    assert_eq!(outcomes[1], candidate()["modules"][1]["outcomes"][1]);
}

#[test]
fn a_summary_keeps_its_citation_unless_its_text_changes() {
    let asked = asked(1);
    let names = names(&asked);
    let reworded = replacing(&names, Place::Summary, "Read the kiln tables.");
    let lost = applied(&asked, &reworded).unwrap();
    assert_eq!(json_of(&lost.module)["summary"]["quote"], "");
    let untouched = applied(&asked, &replacing(&names, Place::Title, "Kiln glazing")).unwrap();
    assert_eq!(json_of(&untouched.module)["summary"]["quote"], GATE);
}

#[test]
fn a_lesson_whose_title_alone_is_reworded_keeps_its_citation_and_one_whose_objective_is_loses_it() {
    let asked = asked(1);
    let names = names(&asked);
    let title = applied(
        &asked,
        &replacing(&names, Place::LessonTitle(0), "Kiln warm-up"),
    )
    .unwrap();
    let lesson = &json_of(&title.module)["lessons"][0];
    assert_eq!(lesson["title"], "Kiln warm-up");
    assert_eq!(
        (lesson["sourceIndex"].clone(), lesson["quote"].clone()),
        (json!(0), json!(TABLES))
    );
    let objective = applied(
        &asked,
        &replacing(&names, Place::LessonObjective(0), "Chart one firing."),
    )
    .unwrap();
    let lesson = &json_of(&objective.module)["lessons"][0];
    assert_eq!(lesson["objective"], "Chart one firing.");
    assert_eq!(
        (lesson["sourceIndex"].clone(), lesson["quote"].clone()),
        (Value::Null, json!(""))
    );
}

#[test]
fn a_new_lesson_after_the_first_lands_between_the_first_and_second_with_no_origin() {
    let asked = asked(1);
    let names = names(&asked);
    let mut revision = nothing();
    revision.new_lessons = vec![new_lesson(Some(names.lesson(0)), "Choose a glaze")];
    let result = applied(&asked, &revision).unwrap();
    let lessons = json_of(&result.module)["lessons"].clone();
    let titles: Vec<&str> = lessons
        .as_array()
        .unwrap()
        .iter()
        .map(|lesson| lesson["title"].as_str().unwrap())
        .collect();
    assert_eq!(
        titles,
        [
            "Kiln firing: lesson 1",
            "Choose a glaze",
            "Kiln firing: lesson 2",
            "Kiln firing: lesson 3"
        ]
    );
    assert_eq!(result.origins, [Some(0), None, Some(1), Some(2)]);
    assert_eq!(lessons[1]["sourceIndex"], Value::Null);
    assert_eq!(lessons[1]["quote"], "");
    assert_eq!(lessons[1]["estimatedMinutes"], 20);
    assert_eq!(lessons[2]["quote"], GATE);
}

#[test]
fn two_new_lessons_after_one_lesson_stay_in_written_order_and_none_puts_one_first() {
    let asked = asked(1);
    let names = names(&asked);
    let mut revision = nothing();
    revision.new_lessons = vec![
        new_lesson(Some(names.lesson(1)), "Choose a glaze"),
        new_lesson(Some(names.lesson(1)), "Mix a glaze"),
    ];
    let result = applied(&asked, &revision).unwrap();
    assert_eq!(result.origins, [Some(0), Some(1), None, None, Some(2)]);
    let lessons = json_of(&result.module)["lessons"].clone();
    assert_eq!(lessons[2]["title"], "Choose a glaze");
    assert_eq!(lessons[3]["title"], "Mix a glaze");
    let mut first = nothing();
    first.new_lessons = vec![new_lesson(None, "Check the kiln")];
    let result = applied(&asked, &first).unwrap();
    assert_eq!(result.origins, [None, Some(0), Some(1), Some(2)]);
    assert_eq!(
        json_of(&result.module)["lessons"][0]["title"],
        "Check the kiln"
    );
}

#[test]
fn a_new_estimate_replaces_the_saved_one_and_two_different_ones_are_refused() {
    let asked = asked(1);
    let names = names(&asked);
    let mut revision = nothing();
    revision.minutes = vec![Estimate {
        lesson: names.lesson(2),
        minutes: 45,
    }];
    let result = applied(&asked, &revision).unwrap();
    assert_eq!(
        json_of(&result.module)["lessons"][2]["estimatedMinutes"],
        45
    );
    revision.minutes.push(Estimate {
        lesson: names.lesson(2),
        minutes: 45,
    });
    assert!(applied(&asked, &revision).is_ok());
    revision.minutes.push(Estimate {
        lesson: names.lesson(2),
        minutes: 50,
    });
    assert_eq!(
        refusal_of(&asked, &revision),
        format!(
            "{} is given two different estimates.",
            names.lesson(2).handle.written()
        )
    );
}

#[test]
fn an_insert_after_an_outcome_joins_the_outcomes_and_a_new_row_has_no_citation() {
    let asked = asked(1);
    let names = names(&asked);
    let revision = changing(Change {
        replace: vec![],
        insert: vec![(
            Anchor::After(names.row(Place::Outcome(0))),
            "Mix a glaze.".to_owned(),
        )],
        remove: vec![],
        moved: vec![],
    });
    let outcomes = json_of(&applied(&asked, &revision).unwrap().module)["outcomes"].clone();
    assert_eq!(outcomes.as_array().unwrap().len(), 3);
    assert_eq!(outcomes[0], candidate()["modules"][1]["outcomes"][0]);
    assert_eq!(
        outcomes[1],
        json!({"text":"Mix a glaze.","sourceIndex":null,"quote":""})
    );
    assert_eq!(outcomes[2], candidate()["modules"][1]["outcomes"][1]);
}

#[test]
fn deliverables_and_criteria_are_edited_apart_from_the_outcomes() {
    let asked = asked(1);
    let names = names(&asked);
    let revision = changing(Change {
        replace: vec![],
        insert: vec![(
            Anchor::After(names.row(Place::Criterion(1))),
            "The kiln log names each firing.".to_owned(),
        )],
        remove: vec![names.row(Place::Deliverable(0))],
        moved: vec![(names.row(Place::Outcome(1)), Anchor::Start)],
    });
    let module = json_of(&applied(&asked, &revision).unwrap().module);
    assert_eq!(
        module["project"]["deliverables"],
        json!(["A Kiln firing chart."])
    );
    assert_eq!(
        module["project"]["successCriteria"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        module["outcomes"][0]["text"],
        "Compare two Kiln firing tables."
    );
    assert_eq!(module["outcomes"][0]["quote"], TABLES);
}

#[test]
fn each_change_a_row_cannot_take_is_refused_with_what_to_do() {
    let asked = asked(1);
    let names = names(&asked);
    let handle = |place: Place| names.row(place).handle.written();
    let one = |change: Change<Row, String>| refusal_of(&asked, &changing(change));
    let empty = || Change {
        replace: vec![],
        insert: vec![],
        remove: vec![],
        moved: vec![],
    };
    assert_eq!(
        one(Change {
            replace: vec![(names.row(Place::Outcome(0)), "  ".to_owned())],
            ..empty()
        }),
        format!(
            "{} would be left blank: give it its new wording, or leave it out of replace.",
            handle(Place::Outcome(0))
        )
    );
    assert_eq!(
        one(Change {
            insert: vec![(Anchor::Start, "A new row.".to_owned())],
            ..empty()
        }),
        "An inserted row names the row it follows in after; \"start\" names no list here."
    );
    assert_eq!(
        one(Change {
            insert: vec![(Anchor::After(names.row(Place::Summary)), "A new row.".to_owned())],
            ..empty()
        }),
        format!(
            "Nothing can be inserted after {}: only outcomes, deliverables and success criteria take new rows.",
            handle(Place::Summary)
        )
    );
    assert_eq!(
        one(Change {
            insert: vec![(Anchor::After(names.row(Place::Outcome(0))), " ".to_owned())],
            ..empty()
        }),
        "An inserted row needs its text."
    );
    assert_eq!(
        one(Change {
            remove: vec![names.row(Place::LessonTitle(0))],
            ..empty()
        }),
        format!(
            "{} cannot be removed: only outcomes, deliverables and success criteria can.",
            handle(Place::LessonTitle(0))
        )
    );
    let cannot_move = |place: Place, anchor: Anchor<Row>| {
        assert_eq!(
            one(Change {
                moved: vec![(names.row(place.clone()), anchor)],
                ..empty()
            }),
            format!(
                "{} can only move after a row of its own list.",
                handle(place)
            )
        );
    };
    cannot_move(Place::Title, Anchor::Start);
    cannot_move(
        Place::Outcome(0),
        Anchor::After(names.row(Place::Deliverable(0))),
    );
}

#[test]
fn a_module_is_not_both_added_and_removed_and_the_set_of_prerequisites_follows_the_rest() {
    let asked = asked(2);
    let names = names(&asked);
    let mut revision = nothing();
    revision.prerequisites.add = vec![names.entry(0)];
    revision.prerequisites.remove = vec![names.entry(1)];
    let result = applied(&asked, &revision).unwrap();
    assert_eq!(json_of(&result.module)["prerequisiteIndices"], json!([0]));
    revision.prerequisites.add = vec![names.entry(0), names.entry(1)];
    revision.prerequisites.remove = vec![];
    let result = applied(&asked, &revision).unwrap();
    assert_eq!(
        json_of(&result.module)["prerequisiteIndices"],
        json!([0, 1])
    );
    revision.prerequisites.remove = vec![names.entry(1)];
    assert_eq!(
        refusal_of(&asked, &revision),
        format!(
            "{} is both added and removed.",
            names.entry(1).handle.written()
        )
    );
}

#[test]
fn removing_every_outcome_is_refused_by_the_modules_rules() {
    let asked = asked(1);
    let names = names(&asked);
    let revision = changing(Change {
        replace: vec![],
        insert: vec![],
        remove: vec![names.row(Place::Outcome(0)), names.row(Place::Outcome(1))],
        moved: vec![],
    });
    assert_eq!(
        refusal_of(&asked, &revision),
        "Module 2 needs at least one outcome."
    );
}

#[test]
fn a_new_lesson_with_a_title_another_module_uses_is_refused_with_the_courses_own_message() {
    let asked = asked(1);
    let names = names(&asked);
    let mut revision = nothing();
    revision.new_lessons = vec![new_lesson(Some(names.lesson(0)), "Harbour tides: lesson 1")];
    assert_eq!(
        refusal_of(&asked, &revision),
        "Lessons 1.1 and 2.2 share the title \"Harbour tides: lesson 1\"; give each lesson its own title."
    );
}

#[test]
fn more_lessons_than_the_course_allows_a_module_is_refused_with_the_count() {
    let asked = asked(1);
    let names = names(&asked);
    let mut revision = nothing();
    revision.new_lessons = ["Choose a glaze", "Mix a glaze", "Test a glaze"]
        .iter()
        .map(|title| new_lesson(Some(names.lesson(0)), title))
        .collect();
    assert_eq!(
        refusal_of(&asked, &revision),
        "Module 2 needs 3 to 5 lessons; it has 6."
    );
}

#[test]
fn the_modules_rules_alone_refuse_what_decoding_an_outline_refuses_with_the_same_words() {
    let mut broken = candidate();
    broken["modules"][1]["prerequisiteIndices"] = json!([1]);
    let from_outline = generation::decode_outline(
        &broken.to_string(),
        &request(LearningCourseDepth::Course),
        &[],
    )
    .unwrap_err();
    let module: GeneratedModule = serde_json::from_value(broken["modules"][1].clone()).unwrap();
    let alone = generation::module_rules(
        2,
        &module,
        Some(LearningCourseDepth::Course),
        &mut generation::Titles::default(),
    )
    .unwrap_err();
    assert_eq!(alone.to_string(), from_outline.to_string());
    assert!(refusal(&alone).contains("prerequisiteIndices"));
}

fn the_request(asked: &Asked) -> (Says, Value) {
    let mut scope = Scope::for_question();
    let mut showing = scope.request();
    let says = OutlineModuleRewrite::says(asked, &mut showing).unwrap();
    let task = serde_json::from_str(&says.task).unwrap();
    (says, task)
}

#[test]
fn the_request_shows_the_module_as_rows_with_names_and_nothing_internal() {
    let findings = [
        Finding::new(
            "/modules/1/outcomes/0",
            "plan",
            "A claim.",
            "A reason.",
            "Fix it.",
            None,
        ),
        Finding::new(
            "/modules/1/lessons/1",
            "factual",
            "Another claim.",
            "Another reason.",
            "Narrow it.",
            Some((Some(0), GATE.to_owned())),
        ),
        Finding::new(
            "/modules/1",
            "plan",
            "The module.",
            "Why.",
            "Change it.",
            None,
        ),
        Finding::new(
            "/modules/1/outcomes/9",
            "plan",
            "Missing.",
            "Why.",
            "Change it.",
            None,
        ),
    ];
    let (says, task) = the_request(&asked_with(1, &findings));
    assert_eq!(says.instructions, REPAIR_SYSTEM);
    assert_eq!(says.output_tokens, 6000);
    let keys: Vec<&str> = task
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        [
            "goal",
            "priorKnowledge",
            "minutesPerSession",
            "moduleNumber",
            "mostLessons",
            "module",
            "courseSequence",
            "findings",
            "sources"
        ]
    );
    assert_eq!(task["moduleNumber"], 2);
    assert_eq!(task["mostLessons"], 5);
    let module = &task["module"];
    assert_eq!(module["title"], json!({"at":"t1","text":"Kiln firing"}));
    assert_eq!(module["summary"]["at"], "t2");
    assert_eq!(module["outcomes"][1]["at"], "t4");
    assert_eq!(module["project"]["title"]["at"], "t5");
    assert_eq!(module["project"]["successCriteria"][1]["at"], "t10");
    assert_eq!(module["prerequisites"], json!(["m1"]));
    assert_eq!(module["lessons"][0]["lesson"], "l1");
    assert_eq!(module["lessons"][0]["title"]["at"], "t11");
    assert_eq!(module["lessons"][0]["objective"]["at"], "t12");
    assert_eq!(module["lessons"][2]["estimatedMinutes"], 30);
    let sequence = task["courseSequence"].as_array().unwrap();
    assert_eq!(sequence[0]["module"], "m1");
    assert_eq!(sequence[0]["moduleNumber"], 1);
    assert_eq!(
        sequence[1],
        json!({"moduleNumber":2,"thisModule":true,"title":"Kiln firing"})
    );
    assert!(sequence[2].get("module").is_none());
    assert_eq!(sequence[2]["lessons"].as_array().unwrap().len(), 3);
    let about: Vec<&Value> = task["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| &f["about"])
        .collect();
    assert_eq!(
        about,
        [
            &json!("t3"),
            &json!("l2"),
            &json!("the module"),
            &json!("the module")
        ]
    );
    assert!(task["findings"][0].get("contradictingPassage").is_none());
    assert_eq!(
        task["findings"][1]["contradictingPassage"],
        json!({"sourceIndex":0,"quote":GATE})
    );
    let shown = says.task.as_str();
    for unseen in [
        "moduleIndex",
        "moduleNumbering",
        "\"id\"",
        "\"path\"",
        "\"from\"",
        "\"quote\":\"\"",
    ] {
        assert!(!shown.contains(unseen), "{unseen}");
    }
}

#[test]
fn the_request_names_no_module_that_could_not_be_a_prerequisite() {
    let first = the_request(&asked(0)).1;
    assert_eq!(first["module"]["prerequisites"], json!([]));
    assert!(first["courseSequence"]
        .as_array()
        .unwrap()
        .iter()
        .all(|entry| entry.get("module").is_none()));
}

#[test]
fn the_request_shows_what_the_model_is_told_and_nothing_internal() {
    let mut marks = Marks::new();
    let row = |marks: &mut Marks, place: Place, text: &str| Row {
        place,
        text: marks.told(text.to_owned()),
    };
    let cited = |marks: &mut Marks, at: usize| Citation {
        source_index: marks.internal(Some(123_456_700_usize + at)),
        quote: marks.internal(format!("An internal quotation number {at}.")),
    };
    let lesson = |marks: &mut Marks, position: usize| Lesson {
        position,
        title: row(
            marks,
            Place::LessonTitle(position),
            &format!("Told lesson title {position}"),
        ),
        objective: row(
            marks,
            Place::LessonObjective(position),
            &format!("Told lesson objective {position}"),
        ),
        minutes: Told(30),
        citation: cited(marks, 10 + position),
    };
    let module = Module {
        title: row(&mut marks, Place::Title, "Told module title"),
        summary: row(&mut marks, Place::Summary, "Told module summary"),
        summary_citation: cited(&mut marks, 0),
        outcomes: vec![(
            row(&mut marks, Place::Outcome(0), "Told module outcome"),
            cited(&mut marks, 1),
        )],
        project_title: row(&mut marks, Place::ProjectTitle, "Told project title"),
        project_brief: row(&mut marks, Place::ProjectBrief, "Told project brief"),
        deliverables: vec![row(
            &mut marks,
            Place::Deliverable(0),
            "Told project deliverable",
        )],
        criteria: vec![row(
            &mut marks,
            Place::Criterion(0),
            "Told success criterion",
        )],
        prerequisites: vec![0],
        lessons: vec![lesson(&mut marks, 0), lesson(&mut marks, 1)],
    };
    let project = |marks: &mut Marks, name: &str| {
        marks.told(LearningProjectMilestoneDto {
            title: format!("Told {name} project title"),
            brief: format!("Told {name} project brief"),
            deliverables: vec![format!("Told {name} deliverable")],
            success_criteria: vec![format!("Told {name} criterion")],
        })
    };
    let entry = |marks: &mut Marks, position: usize, name: &str| Entry {
        position,
        title: marks.told(format!("Told {name} module")),
        outcomes: vec![marks.told(format!("Told {name} outcome"))],
        project: project(marks, name),
        lessons: vec![marks.told(format!("Told {name} lesson"))],
    };
    let this = Entry {
        position: 1,
        title: Told("Told module title".into()),
        outcomes: vec![],
        project: Told(LearningProjectMilestoneDto {
            title: String::new(),
            brief: String::new(),
            deliverables: vec![],
            success_criteria: vec![],
        }),
        lessons: vec![],
    };
    let sequence = vec![
        entry(&mut marks, 0, "earlier"),
        this,
        entry(&mut marks, 2, "later"),
    ];
    let input = Asked {
        goal: marks.told("Told learner goal".to_owned()),
        prior_knowledge: marks.told("Told prior knowledge".to_owned()),
        minutes_per_session: Told(30),
        index: Internal::of(1),
        depth: Internal::of(Some(LearningCourseDepth::Course)),
        most_lessons: Told(5),
        module,
        sequence,
        findings: vec![Finding::new(
            "/modules/1/outcomes/0",
            "plan",
            "Told finding claim",
            "Told finding reason",
            "Told finding instruction",
            Some((Some(0), "Told contradicting quotation".to_owned())),
        )],
        sources: marks.told(json!([{"sourceIndex":0,"title":"Told source title","passages":["Told source passage"]}])),
    };
    holds::<OutlineModuleRewrite>(&input, &marks);
}

fn reply(text: &str) -> BoxFuture<'static, Result<CompletionResponse>> {
    Box::pin(std::future::ready(Ok(CompletionResponse::from_text(text))))
}

fn prompt_of(sent: &Sent) -> String {
    fixture_prompts(&sent.request).join(
        "
",
    )
}

async fn asked_of(model: &ScriptedModel, asked: &Asked) -> Settled<Revision> {
    ask::<OutlineModuleRewrite>(
        model,
        &Asking {
            progress: None,
            reasoning: None,
        },
        &mut Scope::for_question(),
        asked,
        &crate::model_call::answer_options(),
    )
    .await
    .unwrap()
}

const COMPLETE: &str = r#"{"text":{"replace":[{"at":"t3","with":"Read a kiln chart."}],"insert":[],"remove":[],"move":[]},"newLessons":[{"after":"l1","title":"Choose a glaze","objective":"Pick a glaze for the firing.","minutes":20}],"minutes":[{"lesson":"l2","minutes":45}],"prerequisites":{"add":[],"remove":["m1"]}}"#;

#[tokio::test]
async fn a_complete_answer_is_read_and_applied_to_the_saved_module() {
    let model = ScriptedModel::new().on_request::<OutlineModuleRewrite>(|_, _| reply(COMPLETE));
    let asked = asked(1);
    let Settled::Answered(revision) = asked_of(&model, &asked).await else {
        panic!("the answer was not read");
    };
    let result = applied(&asked, &revision).unwrap();
    let module = json_of(&result.module);
    assert_eq!(module["outcomes"][0]["text"], "Read a kiln chart.");
    assert_eq!(module["lessons"][1]["title"], "Choose a glaze");
    assert_eq!(module["lessons"][2]["estimatedMinutes"], 45);
    assert_eq!(module["prerequisiteIndices"], json!([]));
    assert_eq!(result.origins, [Some(0), None, Some(1), Some(2)]);
    assert_eq!(model.count::<OutlineModuleRewrite>(), 1);
}

#[tokio::test]
async fn an_answer_missing_a_list_is_asked_again_in_the_form_and_the_second_stands() {
    let incomplete = r#"{"text":{"replace":[],"insert":[],"remove":[],"move":[]},"newLessons":[],"prerequisites":{"add":[],"remove":[]}}"#;
    let model = ScriptedModel::new().on_request::<OutlineModuleRewrite>(move |_, nth| {
        reply(if nth == 0 { incomplete } else { COMPLETE })
    });
    let asked = asked(1);
    assert!(matches!(
        asked_of(&model, &asked).await,
        Settled::Answered(_)
    ));
    let sent = model.sent();
    assert_eq!(sent.len(), 2);
    assert!(prompt_of(&sent[1]).contains("Answer again in the form asked for."));
}

#[tokio::test]
async fn an_answer_the_app_refuses_is_corrected_with_what_to_do_and_then_applied() {
    let blank = r#"{"text":{"replace":[{"at":"t3","with":" "}],"insert":[],"remove":[],"move":[]},"newLessons":[],"minutes":[],"prerequisites":{"add":[],"remove":[]}}"#;
    let model = ScriptedModel::new().on_request::<OutlineModuleRewrite>(move |_, nth| {
        reply(if nth == 0 { blank } else { COMPLETE })
    });
    let asked = asked(1);
    assert!(matches!(
        asked_of(&model, &asked).await,
        Settled::Answered(_)
    ));
    let sent = model.sent();
    assert_eq!(sent.len(), 2);
    let again = prompt_of(&sent[1]);
    assert!(
        again.contains(
            "t3 would be left blank: give it its new wording, or leave it out of replace."
        ),
        "{again}"
    );
}

#[tokio::test]
async fn an_answer_refused_twice_is_unanswered_with_the_last_reason() {
    let blank = r#"{"text":{"replace":[{"at":"t3","with":""}],"insert":[],"remove":[],"move":[]},"newLessons":[],"minutes":[],"prerequisites":{"add":[],"remove":[]}}"#;
    let model = ScriptedModel::new().on_request::<OutlineModuleRewrite>(move |_, _| reply(blank));
    let asked = asked(1);
    match asked_of(&model, &asked).await {
        Settled::Unanswered(Why::Refused(reason)) => {
            assert!(reason.contains("would be left blank"))
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(model.count::<OutlineModuleRewrite>(), 2);
}

#[tokio::test]
async fn a_module_after_this_one_has_no_name_so_naming_it_adds_nothing() {
    let later = r#"{"text":{"replace":[],"insert":[],"remove":[],"move":[]},"newLessons":[],"minutes":[],"prerequisites":{"add":["m3"],"remove":[]}}"#;
    let model = ScriptedModel::new().on_request::<OutlineModuleRewrite>(move |_, _| reply(later));
    let asked = asked(1);
    let Settled::Answered(revision) = asked_of(&model, &asked).await else {
        panic!("the answer was not read");
    };
    assert!(revision.prerequisites.add.is_empty());
    let result = applied(&asked, &revision).unwrap();
    assert_eq!(json_of(&result.module), candidate()["modules"][1]);
}

#[test]
fn a_finding_is_about_the_row_the_lesson_or_the_module_its_path_names() {
    let about = |path: &str| Finding::new(path, "plan", "", "", "", None).about;
    assert_eq!(about("/modules/1/summary"), About::Row(Place::Summary));
    assert_eq!(
        about("/modules/1/outcomes/2"),
        About::Row(Place::Outcome(2))
    );
    assert_eq!(about("/modules/1/lessons/0"), About::Lesson(0));
    assert_eq!(about("/modules/1"), About::Module);
    assert_eq!(about("/modules/1/lessons/0/objective"), About::Module);
    assert_eq!(about(""), About::Module);
    let outcomes_in_the_module = asked_with(
        1,
        &[Finding::new(
            "/modules/1/outcomes/7",
            "plan",
            "",
            "",
            "",
            None,
        )],
    );
    assert_eq!(outcomes_in_the_module.findings[0].about, About::Module);
}
