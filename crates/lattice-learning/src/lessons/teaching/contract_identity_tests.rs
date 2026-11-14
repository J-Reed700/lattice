//! Whether the lesson's own words bear out a quote the check gives.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;

fn found(quote: &str) -> bool {
    let lesson = json!({"blocks":[{"kind":"explanation","title":"Opening",
        "body":"The tool prints the working folder when it runs."}],"questions":[]});
    locate(&parts_of(&lesson)[0], quote).is_some()
}

#[test]
fn a_quote_that_differs_only_in_the_case_of_a_letter_is_found_in_the_part() {
    assert!(found("Prints the working folder"));
    assert!(found("prints the working folder"));
}

#[test]
fn a_quote_in_other_words_is_still_not_found() {
    assert!(!found("Prints the current directory"));
}

#[test]
fn the_check_contract_reads_what_it_renders() {
    let answer = |reports: Vec<Report>| Answer {
        planned: vec![Planned {
            item: "Boiling".into(),
            taught: true,
            quote: "Water boils.".into(),
        }],
        used_before_taught: vec![Used {
            what: "A kettle".into(),
            uses: vec!["Fill the kettle.".into()],
            taught_later: "A kettle holds water.".into(),
        }],
        contradictions: vec![Contradiction {
            quote: "It is full.".into(),
            other_quote: "It is empty.".into(),
        }],
        unseen: vec![Unseen {
            quote: "See the chart.".into(),
            what: "the chart".into(),
            never_shown: true,
        }],
        reports,
    };
    lattice_core::application::services::model_contract::test_support::assert_round_trip::<
        LessonContractCheck,
    >(&answer(vec![]));
    lattice_core::application::services::model_contract::test_support::assert_round_trip::<
        LessonContractCheck,
    >(&answer(vec![
        Report {
            report: "A step is missing.".into(),
            about: true,
        },
        Report {
            report: "The tone is dry.".into(),
            about: false,
        },
    ]));
}
