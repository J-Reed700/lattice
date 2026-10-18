//! Canonical keys fold the archive, tracking and `rel=canonical` cases, and
//! normalise per RFC 3986 without folding distinct resources.

#![cfg(test)]

use chrono::{TimeZone, Utc};
use url::Url;

use super::*;
use crate::domain::sourcing::descriptor::FinderId;

fn url(raw: &str) -> Url {
    Url::parse(raw).unwrap()
}

fn key_of(raw: &str) -> String {
    key(&url(raw)).as_str().to_string()
}

#[test]
fn equivalent_spellings_share_a_key() {
    let cases: &[(&str, &str, &str)] = &[
        (
            "utm parameters",
            "https://example.com/a?utm_source=x&utm_medium=y",
            "https://example.com/a",
        ),
        (
            "click ids",
            "https://example.com/a?gclid=1&fbclid=2&msclkid=3",
            "https://example.com/a",
        ),
        (
            "tracking mixed with real parameters, any order",
            "https://example.com/a?b=2&utm_campaign=z&a=1",
            "https://example.com/a?a=1&b=2",
        ),
        (
            "case of scheme and host",
            "HTTPS://Example.COM/a",
            "https://example.com/a",
        ),
        (
            "default port",
            "https://example.com:443/a",
            "https://example.com/a",
        ),
        (
            "fragment",
            "https://example.com/a#section-2",
            "https://example.com/a",
        ),
        (
            "trailing slash",
            "https://example.com/a/",
            "https://example.com/a",
        ),
        (
            "dot segments",
            "https://example.com/x/../a",
            "https://example.com/a",
        ),
        (
            "userinfo",
            "https://user:pw@example.com/a",
            "https://example.com/a",
        ),
        (
            "percent-encoded unreserved",
            "https://example.com/%7Euser/%61",
            "https://example.com/~user/a",
        ),
        (
            "lowercase percent hex",
            "https://example.com/a%2fb",
            "https://example.com/a%2Fb",
        ),
        (
            "http and https",
            "http://example.com/a",
            "https://example.com/a",
        ),
        ("empty path", "https://example.com", "https://example.com/"),
    ];
    for (case, left, right) in cases {
        assert_eq!(key_of(left), key_of(right), "{case}");
    }
}

#[test]
fn distinct_resources_keep_distinct_keys() {
    let cases: &[(&str, &str, &str)] = &[
        (
            "different path",
            "https://example.com/a",
            "https://example.com/b",
        ),
        (
            "different real parameter",
            "https://example.com/a?id=1",
            "https://example.com/a?id=2",
        ),
        (
            "non-default port",
            "https://example.com:8443/a",
            "https://example.com/a",
        ),
        (
            "www is a different host",
            "https://www.example.com/a",
            "https://example.com/a",
        ),
        (
            "encoded slash is not a slash",
            "https://example.com/a%2Fb",
            "https://example.com/a/b",
        ),
    ];
    for (case, left, right) in cases {
        assert_ne!(key_of(left), key_of(right), "{case}");
    }
}

#[test]
fn wayback_urls_key_as_the_page_they_archive() {
    let live = key_of("https://learn.microsoft.com/en-us/dotnet/api/x?view=net-8.0");
    for archived in [
        "https://web.archive.org/web/20240115093000/https://learn.microsoft.com/en-us/dotnet/api/x?view=net-8.0",
        "https://web.archive.org/web/20240115093000id_/https://learn.microsoft.com/en-us/dotnet/api/x?view=net-8.0",
        "https://web.archive.org/web/2024/http://learn.microsoft.com/en-us/dotnet/api/x?view=net-8.0",
        "https://wayback.archive.org/web/20240115/learn.microsoft.com/en-us/dotnet/api/x?view=net-8.0",
        "https://web.archive.org/web/20240115093000/https:/learn.microsoft.com/en-us/dotnet/api/x?view=net-8.0&utm_source=a",
    ] {
        assert_eq!(key_of(archived), live, "{archived}");
    }
}

/// A capture time as (year, month, day, hour, minute, second).
type Ymdhms = (i32, u32, u32, u32, u32, u32);

#[test]
fn unwrap_archive_reads_the_capture_time() {
    let cases: &[(&str, Option<Ymdhms>)] = &[
        (
            "https://web.archive.org/web/20240115093007id_/https://example.com/",
            Some((2024, 1, 15, 9, 30, 7)),
        ),
        (
            "https://web.archive.org/web/202401/https://example.com/",
            Some((2024, 1, 1, 0, 0, 0)),
        ),
        ("https://web.archive.org/web/*/https://example.com/", None),
    ];
    for (raw, expected) in cases {
        let copy = unwrap_archive(&url(raw)).unwrap();
        assert_eq!(copy.original.as_str(), "https://example.com/", "{raw}");
        let expected = expected
            .map(|(y, mo, d, h, mi, s)| Utc.with_ymd_and_hms(y, mo, d, h, mi, s).single().unwrap());
        assert_eq!(copy.captured_at, expected, "{raw}");
    }
}

#[test]
fn unwrap_archive_ignores_what_is_not_a_wayback_copy() {
    for raw in [
        "https://example.com/web/2024/https://other.example/",
        "https://web.archive.org/details/somebook",
        "https://web.archive.org/web/2024/ftp://example.com/file",
    ] {
        assert_eq!(unwrap_archive(&url(raw)), None, "{raw}");
    }
}

#[test]
fn lead_keys_fold_rel_canonical_and_archived_copies() {
    let finder = FinderId::from_static("test");
    let target = url("https://docs.example.com/guide/install");

    let mut mirror = Lead::page(
        url("https://mirror.example.net/install.html"),
        finder.clone(),
        1,
    );
    mirror.rel_canonical = Some(target.clone());

    let mut archived = Lead::page(
        url("https://web.archive.org/web/2023/https://docs.example.com/guide/install"),
        finder.clone(),
        2,
    );
    archived.kind = LeadKind::ArchivedCopy {
        original: target.clone(),
        captured_at: None,
    };

    let tagged = Lead::page(
        url("https://docs.example.com/guide/install/?utm_source=newsletter"),
        finder,
        3,
    );

    let expected = key(&target);
    assert_eq!(lead_key(&mirror), expected, "rel=canonical");
    assert_eq!(lead_key(&archived), expected, "archived copy");
    assert_eq!(lead_key(&tagged), expected, "utm-tagged");
}
