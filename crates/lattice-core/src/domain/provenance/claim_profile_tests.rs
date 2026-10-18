use super::*;

fn fetch() -> ClaimSubject {
    ClaimSubject::new(
        "Fetch API",
        SubjectKind::Standard,
        vec!["fetch()".into()],
        vec!["Web platform APIs".into(), "web development".into()],
    )
}

#[test]
fn keys_normalise_case_width_and_punctuation_but_keep_language_names_apart() {
    assert_eq!(
        SubjectKey::of("  Web   Platform-APIs "),
        SubjectKey("web platform apis".into())
    );
    assert_eq!(SubjectKey::of("ＰＹＴＨＯＮ"), SubjectKey("python".into()));
    assert_ne!(SubjectKey::of("C#"), SubjectKey::of("C"));
    assert_ne!(SubjectKey::of("C++"), SubjectKey::of("C#"));
}

#[test]
fn a_class_matches_the_subject_its_aliases_or_its_broader_classes() {
    let subject = fetch();
    assert_eq!(
        SubjectClass::new("Fetch API", vec![]).match_rank(&subject),
        Some(0)
    );
    assert_eq!(
        SubjectClass::new("fetch()", vec![]).match_rank(&subject),
        Some(1)
    );
    assert_eq!(
        SubjectClass::new("web platform APIs", vec![]).match_rank(&subject),
        Some(2)
    );
    assert_eq!(
        SubjectClass::new("Web development", vec![]).match_rank(&subject),
        Some(3)
    );
}

#[test]
fn a_class_never_matches_by_substring() {
    let python = ClaimSubject::new("Python packaging", SubjectKind::Concept, vec![], vec![]);
    assert_eq!(
        SubjectClass::new("Python", vec![]).match_rank(&python),
        None
    );
    assert_eq!(
        SubjectClass::new("web platform", vec![]).match_rank(&fetch()),
        None
    );
}

#[test]
fn a_class_restricted_to_kinds_ignores_other_kinds() {
    let class = SubjectClass::new("web platform APIs", vec![SubjectKind::Library]);
    assert_eq!(class.match_rank(&fetch()), None);
    let class = SubjectClass::new(
        "web platform APIs",
        vec![SubjectKind::Standard, SubjectKind::Library],
    );
    assert_eq!(class.match_rank(&fetch()), Some(2));
}

#[test]
fn profiles_round_trip_through_their_storage_form() {
    let profile = ClaimProfile {
        kind: ClaimKind::VersionBound,
        subject: Some(fetch()),
        temporal: TemporalScope::Since {
            version: Some("7.2".into()),
            date: None,
        },
        measurement_origin: None,
        attributed_to: None,
        reasons: vec![],
        policy: "claim-profile-v1".into(),
        model: "m".into(),
    };
    let json = serde_json::to_value(&profile).unwrap();
    assert_eq!(json["temporal"]["scope"], "since");
    assert_eq!(
        serde_json::from_value::<ClaimProfile>(json).unwrap(),
        profile
    );
}
