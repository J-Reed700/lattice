//! The claim sources design's level rules (§5): the owner's own account,
//! the user's own sources, and the ladder for a source the course chose
//! whose publisher nobody could identify. Every fixture is a non-software
//! subject: the rules name no vendor, domain or kind of software.

use super::tests::{codes, Case};
use super::*;
use crate::domain::provenance::claim_profile::SubjectKind;
use crate::domain::provenance::signals::{DocumentSignals, SourceDates};

fn subject(name: &str, kind: SubjectKind, class: &str) -> ClaimSubject {
    ClaimSubject::new(name, kind, vec![], vec![class.into()])
}

/// A source the course's research chose, publisher unidentified.
fn unidentified(url: &str, nature: ContentNature, work: WorkEvidence) -> Case {
    let mut case = Case::web(url, Standing::Unknown, EditorialProcess::Unknown, nature)
        .related(SubjectRelation::Independent);
    case.standing.publisher = None;
    case.standing.work = work;
    case.subject = Some(subject("Quercus robur", SubjectKind::Other, "trees"));
    case
}

fn identified(
    url: &str,
    standing: Standing,
    editorial: EditorialProcess,
    nature: ContentNature,
) -> Case {
    let mut case =
        Case::web(url, standing, editorial, nature).related(SubjectRelation::Independent);
    case.subject = Some(subject("Quercus robur", SubjectKind::Other, "trees"));
    case
}

/// A sectioned work citing independent work: herbarium records, journals.
fn cited_structured() -> WorkEvidence {
    WorkEvidence {
        external_domains: 4,
        scholarly_identifiers: 2,
        headings: 14,
        bylines: 0,
        ..WorkEvidence::default()
    }
}

fn uncited_structured() -> WorkEvidence {
    WorkEvidence {
        headings: 9,
        ..WorkEvidence::default()
    }
}

/// An unsectioned article citing two independent origins.
fn cited_article() -> WorkEvidence {
    WorkEvidence {
        external_domains: 2,
        ..WorkEvidence::default()
    }
}

fn level_and_code(case: &Case) -> (TrustLevel, ReasonCode) {
    let trust = case.level();
    (trust.level, *codes(&trust).first().unwrap())
}

#[test]
fn the_owners_own_guide_or_untyped_page_is_primary_for_its_subject() {
    let owner = |nature, editorial| {
        let mut case = Case::web(
            "https://www.example-herbarium.ac.uk/collections/quercus",
            Standing::Institutional,
            editorial,
            nature,
        )
        .related(SubjectRelation::Owner);
        case.subject = Some(subject(
            "the herbarium's oak collection",
            SubjectKind::Work,
            "collections",
        ));
        case.level()
    };
    let guide = owner(ContentNature::Tutorial, EditorialProcess::Editorial);
    assert_eq!(guide.level, TrustLevel::Primary);
    assert!(guide.reasons[0].text.ends_with("its own guide"));
    let page = owner(ContentNature::Unknown, EditorialProcess::Unknown);
    assert_eq!(page.level, TrustLevel::Primary);
    assert!(page.reasons[0].text.ends_with("its own page"));
    assert_eq!(
        owner(ContentNature::Marketing, EditorialProcess::Editorial).level,
        TrustLevel::Established
    );
    let discussion = owner(ContentNature::Discussion, EditorialProcess::None);
    assert_eq!(discussion.level, TrustLevel::Established);
    assert_eq!(codes(&discussion), vec![ReasonCode::OwnerHostedCommunity]);
    assert_eq!(
        owner(ContentNature::Reference, EditorialProcess::CommunityEdited).level,
        TrustLevel::Established
    );
}

#[test]
fn a_source_the_user_added_is_at_least_established() {
    let mut unknown = unidentified(
        "https://www.example.org/notes",
        ContentNature::Reporting,
        WorkEvidence::default(),
    );
    unknown.added_by = AddedBy::User;
    let trust = unknown.level();
    assert_eq!(trust.level, TrustLevel::Established);
    assert!(codes(&trust).contains(&ReasonCode::AddedByYou));

    let mut press = identified(
        "https://www.example-press.ac.uk/chapter",
        Standing::Institutional,
        EditorialProcess::Editorial,
        ContentNature::Reference,
    );
    press.added_by = AddedBy::User;
    let trust = press.level();
    assert_eq!(trust.level, TrustLevel::Authoritative, "never lowered");
    assert!(!codes(&trust).contains(&ReasonCode::AddedByYou));
}

#[test]
fn a_cited_structured_reference_work_from_an_unidentified_publisher_is_established() {
    // An online regional flora with species accounts citing herbarium
    // records and journals; a local history society's book on its town with
    // chapters and footnotes to archives.
    let flora = unidentified(
        "https://flora.example.net/fagaceae/quercus-robur",
        ContentNature::Reference,
        cited_structured(),
    );
    let town = unidentified(
        "https://town-history.example.org/book/03-the-mills",
        ContentNature::PrimaryDocument,
        WorkEvidence {
            restricted_registry_links: 1,
            numbered_path: true,
            ..WorkEvidence::default()
        },
    );
    for case in [&flora, &town] {
        assert_eq!(
            level_and_code(case),
            (TrustLevel::Established, ReasonCode::WorkStandsOnSources)
        );
    }
    let mut added = flora;
    added.added_by = AddedBy::User;
    let trust = added.level();
    assert_eq!(trust.level, TrustLevel::Established);
    assert!(codes(&trust).contains(&ReasonCode::UnknownPublisher));
    assert!(codes(&trust).contains(&ReasonCode::AddedByYou));
}

#[test]
fn an_uncited_reference_work_from_an_unidentified_publisher_is_community() {
    // A chaptered bird guide; a family-history site with chapters.
    for nature in [ContentNature::Reference, ContentNature::Documentation] {
        let case = unidentified(
            "https://birds.example.net/guide",
            nature,
            uncited_structured(),
        );
        assert_eq!(
            level_and_code(&case),
            (TrustLevel::Community, ReasonCode::WorkUnsourced)
        );
    }
}

#[test]
fn a_cited_article_from_an_unidentified_publisher_is_community() {
    // A blog explainer on CRISPR citing two papers; a history magazine
    // article citing two archives.
    let crispr = unidentified(
        "https://genes.example.net/crispr-explained",
        ContentNature::Tutorial,
        WorkEvidence {
            scholarly_identifiers: 2,
            external_domains: 1,
            ..WorkEvidence::default()
        },
    );
    let magazine = unidentified(
        "https://past.example.com/articles/the-siege",
        ContentNature::Reporting,
        cited_article(),
    );
    for case in [&crispr, &magazine] {
        assert_eq!(
            level_and_code(case),
            (TrustLevel::Community, ReasonCode::ArticleCitesSources)
        );
    }
    let one_origin = unidentified(
        "https://past.example.com/articles/the-siege",
        ContentNature::Reporting,
        WorkEvidence {
            external_domains: 1,
            ..WorkEvidence::default()
        },
    );
    assert_eq!(
        level_and_code(&one_origin),
        (TrustLevel::Unknown, ReasonCode::UnknownPublisher)
    );
}

#[test]
fn an_uncited_article_from_an_unidentified_publisher_stays_unknown() {
    // An uncited explainer on mitosis; an uncited article on the
    // Peloponnesian War.
    for nature in [ContentNature::Tutorial, ContentNature::Reporting] {
        let case = unidentified(
            "https://cells.example.net/mitosis",
            nature,
            uncited_structured(),
        );
        assert_eq!(
            level_and_code(&case),
            (TrustLevel::Unknown, ReasonCode::UnknownPublisher)
        );
    }
}

#[test]
fn a_structured_text_of_unclassified_kind_that_cites_sources_is_community() {
    // A plant-toxicity compendium citing journals; a dynasty timeline
    // citing chronicles.
    let compendium = unidentified(
        "https://toxic-plants.example.org/",
        ContentNature::Unknown,
        cited_structured(),
    );
    assert_eq!(
        level_and_code(&compendium),
        (TrustLevel::Community, ReasonCode::UnsortedWorkCitesSources)
    );
    let unstructured = unidentified(
        "https://dynasty.example.org/timeline",
        ContentNature::Unknown,
        cited_article(),
    );
    assert_eq!(unstructured.level().level, TrustLevel::Unknown);
}

#[test]
fn discussion_aggregation_marketing_and_opinion_from_an_unidentified_publisher_stay_unknown() {
    // A gardening Q&A thread; a forum debate on the causes of a war.
    for nature in [
        ContentNature::Discussion,
        ContentNature::Opinion,
        ContentNature::Aggregation,
        ContentNature::Marketing,
    ] {
        let case = unidentified(
            "https://forum.example.net/t/oaks",
            nature,
            cited_structured(),
        );
        assert_eq!(case.level().level, TrustLevel::Unknown, "{nature:?}");
    }
}

#[test]
fn flagged_works_are_excluded_from_the_ladder_and_the_ladder_reads_as_one_order() {
    for flag in [
        RiskFlag::ContentFarmPattern,
        RiskFlag::GeneratedContentPattern,
    ] {
        let mut farm = unidentified(
            "https://flora.example.net/oak",
            ContentNature::Reference,
            cited_structured(),
        );
        farm.standing.flags = vec![flag];
        assert_eq!(farm.level().level, TrustLevel::Unknown, "{flag:?}");
    }
    let mut official = unidentified(
        "https://flora.example.net/oak",
        ContentNature::Reference,
        cited_structured(),
    );
    official.standing.flags = vec![RiskFlag::SelfDeclaredOfficial];
    let trust = official.level();
    assert_eq!(trust.level, TrustLevel::Community);
    assert!(codes(&trust).contains(&ReasonCode::SelfDeclaredOfficialCap));

    let herbarium = identified(
        "https://herbarium.example.ac.uk/specimens/oak",
        Standing::Institutional,
        EditorialProcess::Editorial,
        ContentNature::Reference,
    )
    .level()
    .level;
    let field_guide_author = identified(
        "https://janedoe-fieldguides.example.com/oak",
        Standing::Identified,
        EditorialProcess::SelfPublished,
        ContentNature::Reference,
    )
    .level()
    .level;
    let cited_flora = unidentified(
        "https://flora.example.net/oak",
        ContentNature::Reference,
        cited_structured(),
    )
    .level()
    .level;
    let bird_guide = unidentified(
        "https://birds.example.net/guide",
        ContentNature::Reference,
        uncited_structured(),
    )
    .level()
    .level;
    let cited_blog = unidentified(
        "https://genes.example.net/oak-genome",
        ContentNature::Tutorial,
        cited_article(),
    )
    .level()
    .level;
    let uncited_explainer = unidentified(
        "https://cells.example.net/mitosis",
        ContentNature::Tutorial,
        WorkEvidence::default(),
    )
    .level()
    .level;
    assert_eq!(herbarium, TrustLevel::Authoritative);
    assert_eq!(
        [field_guide_author, cited_flora],
        [TrustLevel::Established; 2]
    );
    assert_eq!([bird_guide, cited_blog], [TrustLevel::Community; 2]);
    assert_eq!(uncited_explainer, TrustLevel::Unknown);
}

#[test]
fn a_history_course_levels_without_any_vendor() {
    let treaty = subject("the Treaty of Westphalia", SubjectKind::Event, "treaties");
    let at = |url: &str, standing, editorial, nature, relation| {
        let mut case = Case::web(url, standing, editorial, nature).related(relation);
        case.subject = Some(treaty.clone());
        case.level().level
    };
    use EditorialProcess as E;
    use SubjectRelation as R;
    assert_eq!(
        at(
            "https://press.example.ac.uk/chapter/westphalia",
            Standing::Institutional,
            E::Editorial,
            ContentNature::Reference,
            R::Independent
        ),
        TrustLevel::Authoritative
    );
    assert_eq!(
        at(
            "https://journal.example.org/article/peace-of-1648",
            Standing::Institutional,
            E::PeerReviewed,
            ContentNature::PrimaryDocument,
            R::Independent
        ),
        TrustLevel::Authoritative
    );
    assert_eq!(
        at(
            "https://legislature.example.gov/statutes/1648",
            Standing::Institutional,
            E::Editorial,
            ContentNature::PrimaryDocument,
            R::Owner
        ),
        TrustLevel::Primary
    );
    assert_eq!(
        at(
            "https://museum.example.org/collection/treaty-copy",
            Standing::Institutional,
            E::Editorial,
            ContentNature::Reference,
            R::Owner
        ),
        TrustLevel::Primary
    );
    assert_eq!(
        at(
            "https://historian.example.com/blog/westphalia",
            Standing::Identified,
            E::SelfPublished,
            ContentNature::Reporting,
            R::Independent
        ),
        TrustLevel::Established
    );
    assert_eq!(
        at(
            "https://en.wikipedia.org/wiki/Peace_of_Westphalia",
            Standing::Institutional,
            E::CommunityEdited,
            ContentNature::Reference,
            R::Independent
        ),
        TrustLevel::Community
    );

    let mut scan = unidentified(
        "https://example.org/unused",
        ContentNature::PrimaryDocument,
        WorkEvidence::default(),
    );
    scan.signals = ProvenanceSignals::Document(DocumentSignals {
        file_name: "treaty-scan.pdf".into(),
        mime_type: "application/pdf".into(),
        page_count: Some(12),
        title: None,
        heading_lines: vec![],
        declared_origin: None,
        dates: SourceDates::default(),
    });
    scan.origin = Origin::of_file("treaty-scan.pdf");
    scan.added_by = AddedBy::User;
    assert_eq!(scan.level().level, TrustLevel::Established);
}
