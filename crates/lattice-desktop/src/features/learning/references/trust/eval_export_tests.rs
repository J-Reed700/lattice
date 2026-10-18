//! The eval export writes one `sources-v1` line per overridden version,
//! labelled by the user with only what the override states.

use super::eval_export::lines;
use super::test_support::{add_page, article, course, id, page_signals};
use lattice_core::application::ports::source_trust::SourceTrustStore;
use lattice_core::domain::provenance::{
    SiteStanding, SubjectClass, SubjectKey, TrustLevel, TrustOverride,
};
use lattice_core::shared::error::Result;
use serde_json::{json, Value};

#[tokio::test]
async fn each_override_labels_the_versions_it_covers() -> Result<()> {
    let course = course().await?;
    let url = "https://docs.example.org/a";
    let saved = add_page(
        &course,
        url,
        &article(&id(), 8),
        page_signals(url, json!({})),
    )
    .await?;
    let overrides = [
        TrustOverride::Source {
            program_id: course.program_id.clone(),
            source_id: saved.source_id.clone(),
            level: TrustLevel::Established,
            note: None,
            set_at: 1,
        },
        TrustOverride::Site {
            registrable_domain: "example.org".into(),
            standing: SiteStanding::Institutional,
            note: Some("A standards body".into()),
            set_at: 1,
        },
        TrustOverride::SiteSubject {
            registrable_domain: "example.org".into(),
            class: SubjectClass {
                label: "Example APIs".into(),
                key: SubjectKey::of("Example APIs"),
                kinds: Vec::new(),
            },
            level: TrustLevel::Primary,
            note: None,
            set_at: 1,
        },
    ];
    for entry in &overrides {
        course.store.set_override(entry).await?;
    }
    let lines: Vec<Value> = lines(&course.store)
        .await?
        .iter()
        .map(|line| serde_json::from_str(line).expect("a JSON line"))
        .collect();
    assert_eq!(lines.len(), 3);
    for line in &lines {
        assert_eq!(line["labeller"], "user");
        assert_eq!(line["signals"]["source"], "web");
    }
    let expected: Vec<&Value> = lines.iter().map(|line| &line["expected"]).collect();
    assert!(expected.contains(&&json!({ "level": "established" })));
    assert!(expected.contains(&&json!({ "standing": "institutional" })));
    assert!(expected.contains(&&json!({ "level": "primary" })));
    let classed = lines
        .iter()
        .find(|line| line["expected"] == json!({ "level": "primary" }))
        .expect("the site-for-subject line");
    assert_eq!(classed["subject"]["name"], "Example APIs");
    let site = lines
        .iter()
        .find(|line| line["expected"] == json!({ "standing": "institutional" }))
        .expect("the site line");
    assert_eq!(site["rationale"], "A standards body");
    Ok(())
}
