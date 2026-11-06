//! Severity and its weights, and the wire form of the stored types.

use super::*;

#[test]
fn the_weights_are_the_designs() {
    assert_eq!(weight(Severity::Critical), 4);
    assert_eq!(weight(Severity::Major), 3);
    assert_eq!(weight(Severity::Minor), 2);
    assert_eq!(weight(Severity::Nit), 1);
}

#[test]
fn critical_is_the_greatest_severity_and_a_nit_the_least() {
    assert!(Severity::Critical > Severity::Major);
    assert!(Severity::Major > Severity::Minor);
    assert!(Severity::Minor > Severity::Nit);
    assert_eq!(
        [Severity::Minor, Severity::Critical, Severity::Nit]
            .into_iter()
            .max(),
        Some(Severity::Critical)
    );
}

#[test]
fn severities_order_as_their_weights_do() {
    for a in Severity::ALL {
        for b in Severity::ALL {
            assert_eq!(a.cmp(&b), weight(a).cmp(&weight(b)), "{a:?} {b:?}");
        }
    }
}

#[test]
fn all_severities_are_listed_most_severe_first() {
    assert!(Severity::ALL.windows(2).all(|pair| pair[0] > pair[1]));
}

#[test]
fn a_severity_is_written_in_snake_case() {
    assert_eq!(
        serde_json::to_string(&Severity::Critical).unwrap(),
        "\"critical\""
    );
    assert_eq!(
        serde_json::from_str::<Severity>("\"nit\"").unwrap(),
        Severity::Nit
    );
}

#[test]
fn a_proof_and_an_origin_are_written_in_snake_case() {
    let killed = Proof::Killed {
        test: "test_total".to_owned(),
    };
    assert_eq!(
        serde_json::to_string(&killed).unwrap(),
        r#"{"killed":{"test":"test_total"}}"#
    );
    assert_eq!(
        serde_json::to_string(&Origin::FoundInReview).unwrap(),
        "\"found_in_review\""
    );
}

#[test]
fn a_drill_survives_being_stored_and_read_back() {
    let drill = Drill {
        id: "drill-1".to_owned(),
        language: "python".to_owned(),
        title: "A shopping cart".to_owned(),
        files: vec![file("main.py", "total = 0\n")],
        key: vec![issue(Severity::Major, 1, 1)],
    };
    let stored = serde_json::to_string(&drill).unwrap();
    assert_eq!(serde_json::from_str::<Drill>(&stored).unwrap(), drill);
}
