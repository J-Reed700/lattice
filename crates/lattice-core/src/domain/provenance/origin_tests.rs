use super::*;

fn origin(url: &str) -> Origin {
    Origin::of_url(url).unwrap()
}

fn restricted(kind: RegistryKind, suffix: &str) -> TldRegistry {
    TldRegistry::Restricted {
        kind,
        suffix: suffix.into(),
    }
}

#[test]
fn a_host_folds_into_its_registrable_domain() {
    let docs = origin("https://docs.python.org/3/library/asyncio.html");
    assert_eq!(docs.key, OriginKey("web:python.org".into()));
    assert_eq!(docs.host.as_deref(), Some("docs.python.org"));
    assert_eq!(docs.registrable_domain.as_deref(), Some("python.org"));
    assert_eq!(docs.tld_registry, TldRegistry::Open);
    assert_eq!(
        origin("https://discuss.python.org/t/1").key,
        docs.key,
        "the owner's forum shares the owner's origin; ownership tells them apart"
    );
}

#[test]
fn public_suffixes_with_several_labels_and_private_suffixes_are_respected() {
    assert_eq!(
        origin("https://www.gov.uk/guidance")
            .registrable_domain
            .as_deref(),
        Some("www.gov.uk")
    );
    assert_eq!(
        origin("https://ico.org.uk/x").registrable_domain.as_deref(),
        Some("ico.org.uk")
    );
    assert_ne!(
        origin("https://alice.github.io/notes").key,
        origin("https://bob.github.io/notes").key,
        "github.io is a private suffix: each user is an origin"
    );
}

#[test]
fn restricted_registries_are_classed_by_their_longest_suffix() {
    assert_eq!(
        origin("https://ods.od.nih.gov/factsheets/").tld_registry,
        restricted(RegistryKind::Government, "gov")
    );
    assert_eq!(
        origin("https://www.who.int/x").tld_registry,
        restricted(RegistryKind::Intergovernmental, "int")
    );
    assert_eq!(
        origin("https://www.ndl.go.jp/x").tld_registry,
        restricted(RegistryKind::Government, "go.jp")
    );
    assert_eq!(
        origin("https://www.cam.ac.uk/x").tld_registry,
        restricted(RegistryKind::Education, "ac.uk")
    );
    assert_eq!(
        origin("https://www.example.co.uk/x").tld_registry,
        TldRegistry::Open,
        "an open second-level zone is no signal"
    );
    assert_eq!(
        origin("https://egov.example.com/").tld_registry,
        TldRegistry::Open,
        "a suffix match is label-aligned, never a substring"
    );
}

#[test]
fn hosts_without_a_public_suffix_are_their_own_origin() {
    let ip = origin("http://192.0.2.10/notes.html");
    assert_eq!(ip.key, OriginKey("web:192.0.2.10".into()));
    assert_eq!(ip.registrable_domain, None);
    assert_eq!(ip.tld_registry, TldRegistry::Unknown);
    let private = origin("https://www.intranet.example/page");
    assert_eq!(private.key, OriginKey("web:www.intranet.example".into()));
    assert_eq!(private.tld_registry, TldRegistry::Unknown);
}

#[test]
fn wayback_captures_unwrap_to_the_original_with_its_timestamp() {
    let archived = origin(
        "https://web.archive.org/web/20240501120000id_/https://docs.python.org/3/library/asyncio.html",
    );
    assert_eq!(archived.key, OriginKey("web:python.org".into()));
    let from = archived.archived.unwrap();
    assert_eq!(
        from.original_url,
        "https://docs.python.org/3/library/asyncio.html"
    );
    assert_eq!(from.archive_host, "web.archive.org");
    assert_eq!(from.archived_at, Some(1_714_564_800));
}

#[test]
fn collapsed_schemes_and_partial_timestamps_are_restored() {
    let archived = origin("https://web.archive.org/web/2015/http:/www.bl.uk/magna-carta");
    let from = archived.archived.unwrap();
    assert_eq!(from.original_url, "http://www.bl.uk/magna-carta");
    assert_eq!(from.archived_at, Some(1_420_070_400));
    assert_eq!(archived.key, OriginKey("web:bl.uk".into()));
}

#[test]
fn archive_today_full_links_unwrap_and_short_links_do_not() {
    let full = origin(
        "https://archive.ph/2026.09.18-101500/https://www.reuters.com/markets/us/fed-decision/",
    );
    assert_eq!(full.key, OriginKey("web:reuters.com".into()));
    assert_eq!(full.archived.unwrap().archived_at, Some(1_789_726_500));
    let short = origin("https://archive.ph/Xk3Lp");
    assert_eq!(short.key, OriginKey("web:archive.ph".into()));
    assert_eq!(short.archived, None);
}

#[test]
fn google_cache_links_unwrap() {
    let cached = origin(
        "https://webcache.googleusercontent.com/search?q=cache:AbCd123:https://www.rfc-editor.org/rfc/rfc9111.html",
    );
    assert_eq!(cached.key, OriginKey("web:rfc-editor.org".into()));
    let from = cached.archived.unwrap();
    assert_eq!(
        from.original_url,
        "https://www.rfc-editor.org/rfc/rfc9111.html"
    );
    assert_eq!(from.archived_at, None);
}

#[test]
fn canonical_links_stay_on_the_origin_or_become_relations() {
    let mut page = origin("https://www.example.com/a?utm=1");
    assert_eq!(
        page.apply_canonical("https://example.com/a"),
        CanonicalLink::SameOrigin
    );
    assert_eq!(page.canonical_url.as_deref(), Some("https://example.com/a"));
    let mut syndicated = origin("https://medium.com/@dan/pods");
    assert_eq!(
        syndicated.apply_canonical("https://dan.example.org/pods"),
        CanonicalLink::CrossOrigin {
            url: "https://dan.example.org/pods".into()
        }
    );
    assert_eq!(syndicated.key, OriginKey("web:medium.com".into()));
    assert_eq!(
        syndicated.apply_canonical("javascript:void(0)"),
        CanonicalLink::Ignored
    );
}

#[test]
fn files_and_pasted_text_have_opaque_origins() {
    let file = Origin::of_file("guidelines.pdf");
    assert!(file.key.0.starts_with("file:"));
    assert_eq!(file.key.0.len(), "file:".len() + 64);
    assert_eq!(Origin::pasted("v-1").key, OriginKey("pasted:v-1".into()));
    assert!(Origin::of_url("mailto:a@b.c").is_err());
    assert!(Origin::of_url("file:///etc/hosts").is_err());
}

#[test]
fn the_registry_table_parses_completely() {
    let rows = REGISTRY_SUFFIXES
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect::<Vec<_>>();
    assert!(rows.len() > 50);
    for row in rows {
        let fields = row.split_whitespace().collect::<Vec<_>>();
        assert_eq!(fields.len(), 2, "{row}");
        assert!(RegistryKind::from_column(fields[1]).is_some(), "{row}");
    }
}
