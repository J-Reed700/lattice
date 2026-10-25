//! A scope written into a query is lifted out of it, and a scope is followed
//! only where its site moved permanently, keeping its breadth.

use super::*;

fn origin(site: &str) -> Origin {
    Origin::parse(site).unwrap()
}

fn url(raw: &str) -> Url {
    Url::parse(raw).unwrap()
}

fn hop(status: u16, to: &str) -> Hop {
    Hop {
        status,
        to: url(to),
    }
}

#[test]
fn a_lone_operator_becomes_the_scope_and_leaves_the_terms() {
    assert_eq!(
        lift_sites("site:docs.anthropic.com tool use input schema"),
        (
            "tool use input schema".to_owned(),
            vec![origin("docs.anthropic.com")]
        )
    );
    assert_eq!(
        lift_sites("Span<T> async SITE:learn.microsoft.com/en-us/dotnet"),
        (
            "Span<T> async".to_owned(),
            vec![origin("learn.microsoft.com/en-us/dotnet")]
        )
    );
}

#[test]
fn operators_joined_by_or_are_one_scope_each_and_take_their_connectors() {
    for query in [
        "site:platform.openai.com OR site:docs.anthropic.com function calling error handling",
        "(site:platform.openai.com OR site:docs.anthropic.com) function calling error handling",
        "function calling error handling site:platform.openai.com | site:docs.anthropic.com",
    ] {
        assert_eq!(
            lift_sites(query),
            (
                "function calling error handling".to_owned(),
                vec![origin("platform.openai.com"), origin("docs.anthropic.com")]
            ),
            "{query}"
        );
    }
}

#[test]
fn a_connector_between_terms_is_kept() {
    assert_eq!(
        lift_sites("site:postgresql.org HNSW OR IVFFlat index"),
        (
            "HNSW OR IVFFlat index".to_owned(),
            vec![origin("postgresql.org")]
        )
    );
}

#[test]
fn exclusions_and_plain_queries_are_terms() {
    assert_eq!(
        lift_sites("tool calling -site:pinterest.com"),
        ("tool calling -site:pinterest.com".to_owned(), Vec::new())
    );
    assert_eq!(
        lift_sites("  reciprocal   rank fusion "),
        ("reciprocal rank fusion".to_owned(), Vec::new())
    );
}

#[test]
fn a_site_named_twice_is_one_scope_and_an_empty_operator_is_dropped() {
    assert_eq!(
        lift_sites("site:openai.com structured outputs site:OpenAI.com site:"),
        ("structured outputs".to_owned(), vec![origin("openai.com")])
    );
}

#[test]
fn a_permanent_redirect_off_the_site_moves_it_whole() {
    // docs.anthropic.com, as it answered on 2026-10-25.
    let site = SiteHome::after(
        origin("docs.anthropic.com"),
        &[
            hop(301, "https://platform.claude.com/docs/"),
            hop(308, "https://platform.claude.com/docs"),
            hop(307, "https://platform.claude.com/docs/en/home"),
        ],
    );
    assert!(site.moved());
    assert_eq!(site.home(), &origin("platform.claude.com"));
    assert_eq!(site.asked(), &origin("docs.anthropic.com"));
    for still_the_site in [
        "https://platform.claude.com/docs/en/agents-and-tools/tool-use/overview",
        "https://docs.anthropic.com/en/docs/build-with-claude/tool-use",
    ] {
        assert!(site.covers(&url(still_the_site)), "{still_the_site}");
    }
    assert!(!site.covers(&url("https://ai-tldr.dev/learn/what-is-mcp/")));
}

#[test]
fn a_move_that_then_settles_on_its_new_host_keeps_that_host() {
    // docs.microsoft.com: a permanent move, then a locale redirect at home.
    let site = SiteHome::after(
        origin("docs.microsoft.com"),
        &[
            hop(301, "https://learn.microsoft.com/"),
            hop(302, "https://learn.microsoft.com/en-us/"),
        ],
    );
    assert_eq!(site.home(), &origin("learn.microsoft.com"));
}

#[test]
fn chained_permanent_moves_end_at_the_last_home() {
    let site = SiteHome::after(
        origin("old.example"),
        &[
            hop(308, "https://interim.example/"),
            hop(301, "https://current.example/start"),
        ],
    );
    assert_eq!(site.home(), &origin("current.example"));
}

#[test]
fn temporary_redirects_off_the_site_do_not_move_it() {
    for status in [302, 303, 307] {
        let site = SiteHome::after(
            origin("console.example.com"),
            &[hop(status, "https://signin.example.net/login")],
        );
        assert!(!site.moved(), "{status}");
        assert_eq!(site.home(), &origin("console.example.com"));
    }
}

#[test]
fn redirects_within_the_site_are_its_own_arrangement() {
    // docs.python.org/ → /3/, http → https, a bare host → its www.
    for (asked, to) in [
        ("docs.python.org", "https://docs.python.org/3/"),
        ("example.org", "https://www.example.org/"),
        ("react.dev", "https://react.dev/learn"),
    ] {
        let site = SiteHome::after(origin(asked), &[hop(301, to)]);
        assert!(!site.moved(), "{asked} → {to}");
    }
}

#[test]
fn a_part_of_a_site_moves_to_the_part_it_became() {
    let account = SiteHome::after(
        origin("github.com/dotnet"),
        &[hop(301, "https://github.com/dotnet-foundation/runtime")],
    );
    assert_eq!(account.home(), &origin("github.com/dotnet-foundation"));
    assert!(!account.covers(&url("https://github.com/someone-else/runtime")));

    let docs = SiteHome::after(
        origin("docs.anthropic.com/en/docs"),
        &[
            hop(301, "https://platform.claude.com/docs/en/docs"),
            hop(307, "https://platform.claude.com/docs/en/intro"),
        ],
    );
    assert_eq!(docs.home(), &origin("platform.claude.com/docs/en"));
}

#[test]
fn a_part_redirected_to_something_shallower_has_no_home_to_keep() {
    let site = SiteHome::after(
        origin("github.com/gone-org"),
        &[hop(301, "https://github.com/")],
    );
    assert!(!site.moved());
}

#[test]
fn no_redirects_is_no_move() {
    let site = SiteHome::after(origin("json-schema.org"), &[]);
    assert_eq!(site, SiteHome::unmoved(origin("json-schema.org")));
}
