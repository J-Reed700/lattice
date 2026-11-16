use super::*;
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};

fn input(marks: &mut Marks, continued: bool) -> ReviewInput {
    let files = vec![
        CodeFile {
            path: marks.told("tides/almanac.py".to_owned()),
            text: marks.told("def high(tide): return tide > 4".to_owned()),
        },
        CodeFile {
            path: marks.told("tides/lamp.py".to_owned()),
            text: marks.told("lamp = None  # unlit".to_owned()),
        },
    ];
    let (reported, failed) = if continued {
        (
            vec![marks.told("high ignores the kiln gauge".to_owned())],
            Some(Failure {
                repairs: vec![marks.told("a repair the suite rejected".to_owned())],
                output: marks.told("AssertionError: tide table".to_owned()),
            }),
        )
    } else {
        (Vec::new(), None)
    };
    ReviewInput {
        language: marks.told("harbour-script".to_owned()),
        files,
        reported,
        failed,
        room: marks.internal(123_456usize),
    }
}

#[test]
fn the_review_tells_what_it_holds_and_shows_nothing_internal() {
    for continued in [false, true] {
        let mut marks = Marks::new();
        let input = input(&mut marks, continued);
        holds::<CleanReviewAsk>(&input, &marks);
    }
}
