use super::*;
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};

fn input(marks: &mut Marks, others: bool) -> ReplaceInput {
    let others = if others {
        vec![marks.told(Edit {
            path: "tides/lamp.py".into(),
            before: "lamp = None".into(),
            after: "lamp = unlit".into(),
        })]
    } else {
        Vec::new()
    };
    ReplaceInput {
        language: marks.told("harbour-script".to_owned()),
        pattern: marks.told("Kiln gauge (kiln.gauge)".to_owned()),
        definition: marks.told("A gauge that is read twice.".to_owned()),
        files: vec![CodeFile {
            path: marks.told("tides/almanac.py".to_owned()),
            text: marks.told("def high(tide): return tide > 4".to_owned()),
        }],
        suite: marks.told("assert high(5) is True".to_owned()),
        others,
        earlier: marks.told("a change the suite rejected".to_owned()),
        output: marks.told("AssertionError: tide table".to_owned()),
        room: marks.internal(123_456usize),
    }
}

#[test]
fn the_new_edit_tells_what_it_holds_and_shows_nothing_internal() {
    for others in [false, true] {
        let mut marks = Marks::new();
        let input = input(&mut marks, others);
        holds::<NewEdit>(&input, &marks);
    }
}
