//! Result-page parsing, moved here with the parser from `web/results.rs`.

use super::*;

/// The exact protocol-relative href shape DuckDuckGo's HTML endpoint emits is
/// unwrapped.
#[test]
fn a_protocol_relative_duckduckgo_redirect_resolves_to_its_destination() {
    let raw = "//duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.thereviewgeek.com%2Fsilo%2Ds1e2review%2F&rut=c99505e363ada8be";
    assert_eq!(
        normalize_search_url(raw).as_deref(),
        Some("https://www.thereviewgeek.com/silo-s1e2review/")
    );
}

#[test]
fn an_absolute_duckduckgo_redirect_resolves_to_its_destination() {
    let raw = "https://duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fa%2Fb&rut=deadbeef";
    assert_eq!(
        normalize_search_url(raw).as_deref(),
        Some("https://example.com/a/b")
    );
}

/// A wrapper we cannot read is still a better result than no result: the
/// fetch may follow it server-side. Only a decodable payload is replaced.
#[test]
fn a_redirect_without_a_readable_destination_is_left_alone() {
    for raw in [
        "https://duckduckgo.com/l/?rut=deadbeef",
        "https://duckduckgo.com/l/?uddg=not-a-url",
    ] {
        assert_eq!(normalize_search_url(raw).as_deref(), Some(raw));
    }
}

#[test]
fn an_ordinary_result_url_passes_through_untouched() {
    let raw = "https://example.com/silo/recap?season=1";
    assert_eq!(normalize_search_url(raw).as_deref(), Some(raw));
}

#[test]
fn duckduckgos_challenge_page_is_recognised() {
    let duckduckgo = r#"<html><body>
            <div class="anomaly-modal__title">Unfortunately, bots use DuckDuckGo too.</div>
        </body></html>"#;
    assert!(is_challenge_page(duckduckgo));
    assert!(!is_challenge_page(
        "<html><a class='result__a' href='https://a.example/'>A</a></html>"
    ));
}

#[test]
fn test_parse_search_results_generic_anchor_fallback() {
    let html = r#"
            <html>
                <body>
                    <a href="https://example.com/articles/arugula-health-benefits">
                        Arugula compounds and health effects
                    </a>
                </body>
            </html>
        "#;

    let hits = parse_results(html, 5);
    assert_eq!(hits.len(), 1);
    assert_eq!(
        hits[0].url.as_str(),
        "https://example.com/articles/arugula-health-benefits"
    );
}

#[test]
fn test_parse_search_results_generic_fallback_skips_search_hosts() {
    let html = r#"
            <html>
                <body>
                    <a href="https://duckduckgo.com/?q=arugula">Search navigation</a>
                    <a href="https://example.com/arugula">Result outside the engine</a>
                </body>
            </html>
        "#;

    let hits = parse_results(html, 5);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].url.as_str(), "https://example.com/arugula");
}

#[test]
fn a_repeated_result_and_its_tracking_twin_are_listed_once() {
    let html = r#"<html><body>
        <div class="result"><a class="result__a" href="https://a.example/doc?utm_source=x">A</a>
            <div class="result__snippet">first</div></div>
        <div class="result"><a class="result__a" href="https://a.example/doc">A again</a></div>
        <div class="result"><a class="result__a" href="https://b.example/">B</a></div>
    </body></html>"#;

    let hits = parse_results(html, 10);
    let urls: Vec<_> = hits.iter().map(|hit| hit.url.as_str()).collect();
    assert_eq!(
        urls,
        ["https://a.example/doc?utm_source=x", "https://b.example/"]
    );
    assert_eq!(hits[0].snippet, "first");
}
