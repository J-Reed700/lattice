use super::*;

fn brief(difficulty: Difficulty) -> Brief {
    Brief {
        title: "Any".to_owned(),
        difficulty,
        spec: serde_json::Value::Null,
    }
}

#[test]
fn a_program_gets_twenty_thirty_five_or_forty_five_minutes_by_difficulty() {
    let seconds = |difficulty| ProgramBudget.seconds(&brief(difficulty));
    assert_eq!(seconds(Difficulty::Easy), Some(1200));
    assert_eq!(seconds(Difficulty::Medium), Some(2100));
    assert_eq!(seconds(Difficulty::Hard), Some(2700));
}
