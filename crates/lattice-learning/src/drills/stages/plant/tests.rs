use super::*;
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
use lattice_core::domain::learning::drills::LessonSentence;

fn input(marks: &mut Marks) -> Input {
    let sentence = |text: &str, source: Option<&str>| LessonSentence {
        text: text.to_owned(),
        source: source.map(str::to_owned),
    };
    let evidence = vec![
        sentence("A pilot reads the chart before the tide turns.", None),
        sentence(
            "A berth is cleared an hour before the ship.",
            Some("Harbour almanac"),
        ),
    ];
    let brief = LessonBrief {
        lesson_id: String::new(),
        course: "Harbour pilotage".to_owned(),
        goal: "Bring ships to berth safely".to_owned(),
        prior_knowledge: "Basic seamanship".to_owned(),
        module: "Reading the tide tables".to_owned(),
        module_summary: "How the tide tables are read".to_owned(),
        outcomes: vec!["Read a tide table without error".to_owned()],
        lesson: "The depth at a mark".to_owned(),
        objective: "Read the depth of a mark".to_owned(),
        evidence,
    };
    let _ = marks.told((
        &brief.course,
        &brief.goal,
        &brief.prior_knowledge,
        &brief.module,
        &brief.module_summary,
        &brief.outcomes,
        &brief.lesson,
        &brief.objective,
        &brief.evidence,
    ));
    Input {
        instructions: marks
            .told("Plant a defect for each pattern as a hurried pilot might.".to_owned()),
        frame: Told(PlantFrame {
            before: "Language: python\n\n".to_owned(),
            heading: "The code:\n\n".to_owned(),
            open: "<file path=\"{path}\">".to_owned(),
            close: "</file>\n".to_owned(),
            gap: "\n".to_owned(),
            after: String::new(),
        }),
        brief: Some(Told(brief)),
        files: vec![CodeFile {
            path: marks.told("tide/harbour.py".to_owned()),
            text: marks.told("depth = chart[mark]\nberth = depth + 1\n".to_owned()),
        }],
        targets: vec![Target {
            id: marks.internal("pilotage.tide_table".to_owned()),
            name: marks.told("Tide table misread".to_owned()),
            family: marks.told("correctness".to_owned()),
            definition: marks.told("A tide table is read at the wrong mark.".to_owned()),
            note: Some(marks.told("The change was in a file that is not a test.".to_owned())),
        }],
        catalogue: vec![Catalogued {
            id: marks.internal("pilotage.berth_lag".to_owned()),
            name: marks.told("Berth cleared late".to_owned()),
            family: marks.told("concurrency".to_owned()),
        }],
        others: vec![marks.told(Edit {
            path: "tide/lock.py".to_owned(),
            before: "gate = open_at(low)\n".to_owned(),
            after: "gate = open_at(high)\n".to_owned(),
        })],
        planted: vec![marks.internal("pilotage.lock_order".to_owned())],
        lessons_left: Told(2),
        room: marks.internal(654_321_usize),
    }
}

#[test]
fn the_plant_request_shows_what_it_is_told_and_nothing_internal() {
    let mut marks = Marks::new();
    let input = input(&mut marks);
    holds::<Plant<true, true>>(&input, &marks);
}

#[test]
fn a_plant_request_without_lessons_or_a_runner_shows_what_it_is_told_and_nothing_internal() {
    let mut marks = Marks::new();
    let mut input = input(&mut marks);
    input.brief = None;
    input.catalogue = Vec::new();
    let mut marks = Marks::new();
    input.instructions =
        marks.told("Plant a defect for each pattern as a hurried pilot might.".to_owned());
    input.files = vec![CodeFile {
        path: marks.told("tide/harbour.py".to_owned()),
        text: marks.told("depth = chart[mark]\n".to_owned()),
    }];
    input.targets = vec![Target {
        id: marks.internal("pilotage.tide_table".to_owned()),
        name: marks.told("Tide table misread".to_owned()),
        family: marks.told("correctness".to_owned()),
        definition: marks.told("A tide table is read at the wrong mark.".to_owned()),
        note: None,
    }];
    input.others = Vec::new();
    input.planted = Vec::new();
    input.room = marks.internal(654_321_usize);
    holds::<Plant<false, false>>(&input, &marks);
}

#[test]
fn a_list_of_names_reads_as_a_sentence() {
    assert_eq!(listed(&["`a`"]), "`a`");
    assert_eq!(listed(&["`a`", "`b`"]), "`a` and `b`");
    assert_eq!(listed(&["`a`", "`b`", "`c`"]), "`a`, `b` and `c`");
}
