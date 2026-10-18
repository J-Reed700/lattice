//! An override change answers with the versions it covers: what the ledger
//! reopens and the gate re-decides.

use super::overrides::{clear, preview, set, CoveredVersion};
use super::test_support::{add_page, article, course, course_on, id, page_signals, Course};
use lattice_core::application::ports::source_trust::SourceTrustStore;
use lattice_core::domain::provenance::{
    OverrideScope, SiteStanding, SubjectClass, TrustLevel, TrustOverride,
};
use lattice_core::shared::error::Result;
use serde_json::json;

fn covered(program_id: &str, version_id: &str) -> CoveredVersion {
    CoveredVersion {
        program_id: program_id.into(),
        source_version_id: version_id.into(),
    }
}

async fn page(course: &Course, url: &str) -> Result<super::test_support::Saved> {
    add_page(
        course,
        url,
        &article(&id(), 8),
        page_signals(url, json!({})),
    )
    .await
}

#[tokio::test]
async fn a_source_override_covers_that_source_and_nothing_else() -> Result<()> {
    let course = course().await?;
    let chosen = page(&course, "https://docs.example.org/a").await?;
    page(&course, "https://docs.example.org/b").await?;
    let entry = TrustOverride::Source {
        program_id: course.program_id.clone(),
        source_id: chosen.source_id.clone(),
        level: TrustLevel::Primary,
        note: None,
        set_at: 1,
    };
    let change = set(&course.store, &entry).await?;
    assert_eq!(change.scope, entry.scope());
    assert_eq!(
        change.versions,
        vec![covered(&course.program_id, &chosen.version_id)]
    );
    let cleared = clear(&course.store, &entry.scope())
        .await?
        .expect("was set");
    assert_eq!(cleared.versions, change.versions);
    assert_eq!(clear(&course.store, &entry.scope()).await?, None);
    Ok(())
}

#[tokio::test]
async fn a_site_override_covers_the_site_in_every_course_with_or_without_a_class() -> Result<()> {
    let course = course().await?;
    let here = page(&course, "https://developer.example.org/api").await?;
    page(&course, "https://elsewhere.example.net/api").await?;
    let other = Course {
        program_id: course_on(&course.pool).await?,
        pool: course.pool.clone(),
        library: course.library.clone(),
        store: course.store.clone(),
    };
    let there = page(&other, "https://www.example.org/api").await?;
    let mut expected = vec![
        covered(&course.program_id, &here.version_id),
        covered(&other.program_id, &there.version_id),
    ];
    expected.sort();

    let site = TrustOverride::Site {
        registrable_domain: "example.org".into(),
        standing: SiteStanding::Institutional,
        note: None,
        set_at: 1,
    };
    assert_eq!(preview(&course.store, &site.scope()).await?, expected);
    assert_eq!(set(&course.store, &site).await?.versions, expected);

    let for_class = TrustOverride::SiteSubject {
        registrable_domain: "example.org".into(),
        class: SubjectClass::new("Example APIs", vec![]),
        level: TrustLevel::Primary,
        note: None,
        set_at: 2,
    };
    assert_eq!(set(&course.store, &for_class).await?.versions, expected);
    // Both stand: the class override does not replace the site's standing.
    assert_eq!(course.store.site_overrides().await?.len(), 2);
    Ok(())
}

#[tokio::test]
async fn an_override_without_its_source_or_site_is_refused() -> Result<()> {
    let course = course().await?;
    let blank = TrustOverride::Site {
        registrable_domain: " ".into(),
        standing: SiteStanding::Community,
        note: None,
        set_at: 1,
    };
    assert!(set(&course.store, &blank).await.is_err());
    let scope = OverrideScope::Site {
        registrable_domain: "never-set.example".into(),
    };
    assert_eq!(clear(&course.store, &scope).await?, None);
    Ok(())
}
