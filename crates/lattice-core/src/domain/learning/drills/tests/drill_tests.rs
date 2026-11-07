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
        material: Material::code("python"),
        title: "A shopping cart".to_owned(),
        files: vec![file("main.py", "total = 0\n")],
        key: vec![issue(Severity::Major, 1, 1)],
    };
    let stored = serde_json::to_string(&drill).unwrap();
    assert_eq!(serde_json::from_str::<Drill>(&stored).unwrap(), drill);
}

fn drill_json(material: &str) -> String {
    format!(
        r#"{{"id":"drill-1",{material},"title":"A shopping cart","files":[{{"path":"main.py","text":"total = 0\n"}}],"key":[]}}"#
    )
}

#[test]
fn a_drill_stored_with_a_language_loads_as_code() {
    let stored = drill_json(r#""language":"python""#);
    let drill = serde_json::from_str::<Drill>(&stored).unwrap();
    assert_eq!(drill.material, Material::code("python"));
    assert_eq!(drill.title, "A shopping cart");
}

#[test]
fn a_drill_with_a_material_loads_as_that_material() {
    let code = drill_json(r#""material":{"kind":"code","language":"rust"}"#);
    assert_eq!(
        serde_json::from_str::<Drill>(&code).unwrap().material,
        Material::code("rust")
    );
    let passage = drill_json(r#""material":{"kind":"passage"}"#);
    assert_eq!(
        serde_json::from_str::<Drill>(&passage).unwrap().material,
        Material::Passage
    );
}

#[test]
fn a_drill_with_neither_a_material_nor_a_language_does_not_load() {
    let stored = r#"{"id":"d","title":"t","files":[],"key":[]}"#;
    assert!(serde_json::from_str::<Drill>(stored).is_err());
}

#[test]
fn a_drill_is_written_with_a_material_and_no_language() {
    let drill = Drill {
        id: "drill-1".to_owned(),
        material: Material::Passage,
        title: "A study note".to_owned(),
        files: vec![],
        key: vec![],
    };
    let written = serde_json::to_value(&drill).unwrap();
    assert_eq!(
        written["material"],
        serde_json::json!({ "kind": "passage" })
    );
    assert!(written.get("language").is_none());
    let code = Drill {
        material: Material::code("go"),
        ..drill
    };
    assert_eq!(
        serde_json::to_value(&code).unwrap()["material"],
        serde_json::json!({ "kind": "code", "language": "go" })
    );
}

#[test]
fn a_material_knows_its_language() {
    assert_eq!(Material::code("go").language(), Some("go"));
    assert_eq!(Material::Passage.language(), None);
}

#[test]
fn a_sourced_proof_survives_serde() {
    for planted in [SourceVerdict::Contradicted, SourceVerdict::Unsupported] {
        let proof = Proof::Sourced {
            quote: "relapses fell by 14 percent".to_owned(),
            planted,
            reason: "The source states 14.".to_owned(),
        };
        let stored = serde_json::to_string(&proof).unwrap();
        assert_eq!(serde_json::from_str::<Proof>(&stored).unwrap(), proof);
    }
    let written = serde_json::to_value(Proof::Sourced {
        quote: "q".to_owned(),
        planted: SourceVerdict::Contradicted,
        reason: "r".to_owned(),
    })
    .unwrap();
    assert_eq!(
        written,
        serde_json::json!({ "sourced": { "quote": "q", "planted": "contradicted", "reason": "r" } })
    );
}

#[test]
fn a_drill_with_a_sourced_issue_survives_being_stored() {
    let mut issue = issue(Severity::Major, 1, 1);
    issue.proof = Proof::Sourced {
        quote: "q".to_owned(),
        planted: SourceVerdict::Unsupported,
        reason: "r".to_owned(),
    };
    let drill = Drill {
        id: "d".to_owned(),
        material: Material::Passage,
        title: "t".to_owned(),
        files: vec![file("explanation.md", "One.\n")],
        key: vec![issue],
    };
    let stored = serde_json::to_string(&drill).unwrap();
    assert_eq!(serde_json::from_str::<Drill>(&stored).unwrap(), drill);
}
