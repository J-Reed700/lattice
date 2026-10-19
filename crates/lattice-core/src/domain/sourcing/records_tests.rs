//! A registry record speaks for the subject only when the need named its
//! package, under that registry's naming rules.

#![cfg(test)]

use url::Url;

use super::*;
use crate::domain::sourcing::descriptor::{FinderId, Reach};
use crate::domain::sourcing::lead::LeadKind;
use crate::domain::sourcing::need::{
    Ecosystem, Freshness, Identifier, NeedKind, SubjectHint, SubjectKind,
};

fn need(kind: NeedKind, subject: Option<&str>, identifiers: Vec<Identifier>) -> SourceNeed {
    SourceNeed {
        kind,
        query: "how to send a request with a timeout".into(),
        subject: subject.map(|name| SubjectHint {
            name: name.into(),
            kind: SubjectKind::Software,
            identifiers,
        }),
        scope: None,
        freshness: Freshness::Any,
        want: 10,
        alternatives: Vec::new(),
        also: Vec::new(),
    }
}

fn record(url: &str, owner: &str) -> Lead {
    Lead {
        kind: LeadKind::OfficialSite,
        owner: Origin::parse(owner),
        ..Lead::page(Url::parse(url).unwrap(), FinderId::from_static("pypi"), 1)
    }
}

#[test]
fn registry_names_round_trip_in_any_case() {
    for ecosystem in Ecosystem::ALL {
        assert_eq!(Ecosystem::parse(ecosystem.as_str()), Some(ecosystem));
        assert_eq!(
            Ecosystem::parse(&ecosystem.as_str().to_ascii_uppercase()),
            Some(ecosystem)
        );
    }
    assert_eq!(Ecosystem::parse("ruby"), None);
}

#[test]
fn each_registry_compares_names_by_its_own_rules() {
    assert!(Ecosystem::PyPi.same_package("Zope.Interface", "zope-interface"));
    assert!(Ecosystem::PyPi.same_package("typing__extensions", "typing-extensions"));
    assert!(!Ecosystem::PyPi.same_package("requests", "requests-oauthlib"));
    assert!(Ecosystem::Crates.same_package("serde_json", "serde-json"));
    assert!(!Ecosystem::Crates.same_package("serde.json", "serde-json"));
    assert!(Ecosystem::Npm.same_package("React", "react"));
    assert!(!Ecosystem::Npm.same_package("react", "react-dom"));
    assert!(Ecosystem::Maven.same_package(
        "jackson-databind",
        "com.fasterxml.jackson.core:jackson-databind"
    ));
    assert!(!Ecosystem::Maven.same_package(
        "org.other:jackson-databind",
        "com.fasterxml.jackson.core:jackson-databind"
    ));
    assert!(Ecosystem::Go.same_package("mux", "github.com/gorilla/mux"));
    assert!(Ecosystem::Go.same_package("github.com/gorilla/mux", "github.com/gorilla/mux"));
    assert!(!Ecosystem::Go.same_package("gorilla/mux", "github.com/gorilla/mux"));
}

#[test]
fn a_need_names_a_package_by_identifier_or_by_a_package_subject() {
    let by_id = need(
        NeedKind::OfficialDocs,
        Some("Requests"),
        vec![Identifier::PackageName {
            ecosystem: Ecosystem::PyPi,
            name: "requests".into(),
        }],
    );
    let package = by_id.package().unwrap();
    assert_eq!(package.ecosystem, Some(Ecosystem::PyPi));
    assert_eq!(package.name, "requests");

    let by_subject = need(NeedKind::Package, Some(" requests "), Vec::new());
    let package = by_subject.package().unwrap();
    assert_eq!(package.ecosystem, None);
    assert_eq!(package.name, "requests");

    // A docs need about a product names no package, and a package need
    // with no subject asks which packages exist.
    assert_eq!(
        need(NeedKind::OfficialDocs, Some("Requests"), Vec::new()).package(),
        None
    );
    assert_eq!(need(NeedKind::Package, None, Vec::new()).package(), None);
}

#[test]
fn a_record_is_the_package_only_in_the_registry_named() {
    let package = need(
        NeedKind::Package,
        Some("requests"),
        vec![Identifier::PackageName {
            ecosystem: Ecosystem::PyPi,
            name: "requests".into(),
        }],
    );
    let package = package.package().unwrap();
    assert!(package.is_record(Ecosystem::PyPi, "Requests"));
    assert!(!package.is_record(Ecosystem::Npm, "requests"));

    let unnamed_registry = need(NeedKind::Package, Some("requests"), Vec::new());
    let unnamed_registry = unnamed_registry.package().unwrap();
    assert!(unnamed_registry.is_record(Ecosystem::Npm, "requests"));
    assert!(!unnamed_registry.is_record(Ecosystem::Npm, "requests-mock"));
}

#[test]
fn search_hits_never_assert_an_owner() {
    let searching = need(NeedKind::Package, None, Vec::new());
    let hits = [record("https://some-lib.dev/", "some-lib.dev")];
    assert_eq!(registry_owner(&searching, &hits), None);
}

#[test]
fn the_named_registry_record_asserts_its_owner() {
    let named = need(
        NeedKind::Package,
        Some("requests"),
        vec![Identifier::PackageName {
            ecosystem: Ecosystem::PyPi,
            name: "requests".into(),
        }],
    );
    let records = [record(
        "https://requests.readthedocs.io/",
        "requests.readthedocs.io",
    )];
    assert_eq!(
        registry_owner(&named, &records),
        Origin::parse("requests.readthedocs.io")
    );
}

#[test]
fn a_name_several_registries_hold_under_different_owners_names_no_owner() {
    let unnamed_registry = need(NeedKind::Package, Some("requests"), Vec::new());
    let collision = [
        record(
            "https://requests.readthedocs.io/",
            "requests.readthedocs.io",
        ),
        record(
            "https://github.com/request/request",
            "github.com/request/request",
        ),
    ];
    assert_eq!(registry_owner(&unnamed_registry, &collision), None);

    let agreeing = [
        record("https://protobuf.dev/", "protobuf.dev"),
        record("https://protobuf.dev/reference/", "protobuf.dev"),
    ];
    assert_eq!(
        registry_owner(&unnamed_registry, &agreeing),
        Origin::parse("protobuf.dev")
    );
}

#[test]
fn only_records_on_the_owners_origins_are_kept_whole() {
    let pypi_owner = "requests.readthedocs.io";
    let npm_owner = "github.com/request/request";
    let leads = vec![
        record("https://requests.readthedocs.io/", pypi_owner),
        record("https://github.com/psf/requests", pypi_owner),
        record("https://pypi.org/project/requests/", pypi_owner),
        record("https://github.com/request/request", npm_owner),
        record("https://www.npmjs.com/package/request", npm_owner),
    ];
    // Wikidata names the source repository, not the docs site: the PyPI
    // record lists that repository, so all three of its leads stay.
    let origins = [Origin::parse("github.com/psf/requests").unwrap()];
    let kept: Vec<String> = records_on_owner(leads, &origins)
        .into_iter()
        .map(|lead| lead.url.to_string())
        .collect();
    assert_eq!(
        kept,
        [
            "https://requests.readthedocs.io/",
            "https://github.com/psf/requests",
            "https://pypi.org/project/requests/",
        ]
    );
}

#[test]
fn a_search_hit_on_a_code_host_or_registry_asserts_no_owner() {
    let searching = need(NeedKind::Package, None, Vec::new());
    let repo = [record(
        "https://github.com/someone/requests-clone",
        "github.com/someone/requests-clone",
    )];
    let registry = [record("https://some-lib.dev/", "some-lib.dev")];
    let standard = [record(
        "https://www.rfc-editor.org/rfc/rfc9110",
        "rfc-editor.org",
    )];
    let steps = [
        (Reach::CodeHost, &repo[..]),
        (Reach::Registry(Ecosystem::Npm), &registry[..]),
        (Reach::Standards, &standard[..]),
    ];
    assert_eq!(
        asserted_owner(&searching, steps),
        Origin::parse("rfc-editor.org"),
        "only the standards body's lead asserts an owner"
    );
}

#[test]
fn the_named_repository_and_package_assert_their_owners_in_step_order() {
    let named = need(
        NeedKind::Package,
        Some("requests"),
        vec![
            Identifier::Repo {
                host: "github.com".into(),
                owner: "psf".into(),
                name: "requests".into(),
            },
            Identifier::PackageName {
                ecosystem: Ecosystem::PyPi,
                name: "requests".into(),
            },
        ],
    );
    let repo = [record(
        "https://github.com/psf/requests",
        "github.com/psf/requests",
    )];
    let registry = [record(
        "https://requests.readthedocs.io/",
        "requests.readthedocs.io",
    )];
    assert_eq!(
        asserted_owner(
            &named,
            [
                (Reach::CodeHost, &repo[..]),
                (Reach::Registry(Ecosystem::PyPi), &registry[..])
            ]
        ),
        Origin::parse("github.com/psf/requests")
    );
    assert_eq!(
        asserted_owner(
            &named,
            [
                (Reach::CodeHost, &[][..]),
                (Reach::Registry(Ecosystem::PyPi), &registry[..])
            ]
        ),
        Origin::parse("requests.readthedocs.io")
    );
}
