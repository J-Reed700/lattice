//! Cache freshness, the `After` bypass, and keys that fold equivalent needs.

#![cfg(test)]

use chrono::{TimeDelta, TimeZone};

use super::*;
use crate::domain::sourcing::{NeedKind, SubjectKind};

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 18, 14, 0, 0)
        .single()
        .unwrap()
}

fn minutes(n: i64) -> TimeDelta {
    TimeDelta::try_minutes(n).unwrap()
}

fn need(kind: NeedKind, freshness: Freshness) -> SourceNeed {
    SourceNeed {
        kind,
        query: "ignored: the step's query is keyed".into(),
        subject: None,
        scope: None,
        freshness,
        want: 10,
        alternatives: Vec::new(),
        also: Vec::new(),
    }
}

#[test]
fn ttls_come_from_the_policy() {
    let policy = SourcingPolicy::default();
    assert_eq!(
        ttl(SourcingCache::ProviderPage, &policy),
        policy.search_page_ttl
    );
    assert_eq!(
        ttl(SourcingCache::OwnerResolution, &policy),
        policy.owner_ttl
    );
    assert_eq!(
        ttl(SourcingCache::ArchiveLookup, &policy),
        policy.archive_lookup_ttl
    );
    assert_eq!(ttl(SourcingCache::SiteHome, &policy), policy.site_home_ttl);
}

#[test]
fn freshness_table() {
    let ttl = Duration::from_secs(15 * 60);
    let cases = [
        ("just stored", t0(), true),
        ("one minute short of the ttl", t0() - minutes(14), true),
        ("exactly the ttl old", t0() - minutes(15), false),
        ("older than the ttl", t0() - minutes(16), false),
        ("stamped in the future", t0() + minutes(1), false),
    ];
    for (case, stored_at, expected) in cases {
        assert_eq!(is_fresh(stored_at, t0(), ttl), expected, "{case}");
    }
}

#[test]
fn an_after_date_bypasses_pages_stored_before_it() {
    let policy = SourcingPolicy::default();
    let today = t0().date_naive();
    let stored_this_morning = t0() - minutes(5);
    let stored_yesterday_late = start_of(today) - minutes(1);
    let cases = [
        ("any freshness", stored_this_morning, Freshness::Any, true),
        (
            "after today, stored today",
            stored_this_morning,
            Freshness::After(today),
            true,
        ),
        (
            "after today, stored before midnight",
            stored_yesterday_late,
            Freshness::After(today),
            false,
        ),
    ];
    for (case, stored_at, freshness, expected) in cases {
        let now = if stored_at < start_of(today) {
            stored_at + minutes(2)
        } else {
            t0()
        };
        assert_eq!(
            provider_page_usable(stored_at, now, freshness, &policy),
            expected,
            "{case}"
        );
    }
}

#[test]
fn provider_page_keys_fold_spelling_and_split_what_matters() {
    let ddg = FinderId::from_static("duckduckgo");
    let bing = FinderId::from_static("bing_html");
    let web = need(NeedKind::Web, Freshness::Any);
    let scope = Origin::parse("docs.python.org/3").unwrap();
    let base = provider_page_key(&ddg, &web, "asyncio  Task Groups", None, 1);

    assert_eq!(
        base,
        provider_page_key(&ddg, &web, " ASYNCIO task   groups ", None, 1),
        "case and whitespace fold"
    );
    let differs = [
        (
            "finder",
            provider_page_key(&bing, &web, "asyncio task groups", None, 1),
        ),
        (
            "page",
            provider_page_key(&ddg, &web, "asyncio task groups", None, 2),
        ),
        (
            "scope",
            provider_page_key(&ddg, &web, "asyncio task groups", Some(&scope), 1),
        ),
        (
            "kind",
            provider_page_key(
                &ddg,
                &need(NeedKind::OfficialDocs, Freshness::Any),
                "asyncio task groups",
                None,
                1,
            ),
        ),
        (
            "freshness",
            provider_page_key(
                &ddg,
                &need(
                    NeedKind::Web,
                    Freshness::After(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()),
                ),
                "asyncio task groups",
                None,
                1,
            ),
        ),
    ];
    for (case, key) in differs {
        assert_ne!(base, key, "{case}");
    }
}

#[test]
fn owner_and_archive_keys_fold_equivalents() {
    let subject = |name: &str| SubjectHint {
        name: name.into(),
        kind: SubjectKind::Software,
        identifiers: Vec::new(),
    };
    assert_eq!(owner_key(&subject(".NET")), owner_key(&subject("  .net ")));

    let day = NaiveDate::from_ymd_opt(2024, 1, 15);
    let live = Url::parse("https://example.com/a?utm_source=x").unwrap();
    let archived = Url::parse("https://web.archive.org/web/2024/https://example.com/a").unwrap();
    assert_eq!(
        archive_lookup_key(&live, day),
        archive_lookup_key(&archived, day)
    );
    assert_ne!(
        archive_lookup_key(&live, day),
        archive_lookup_key(&live, None)
    );
}
