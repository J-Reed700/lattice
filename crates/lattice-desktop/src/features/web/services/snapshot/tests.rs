//! Sanitizer and fetch-policy tests (§12). No network: pass 2 runs on a
//! hand-built `ResolvedResources`, and the fetch-policy cases stop before any
//! request is made.

use std::cell::{Cell, RefCell};
use std::sync::Arc;

use base64::Engine;
use chrono::{DateTime, TimeZone, Utc};
use url::Url;

use super::capture::{check_status, decode_document};
use super::clutter::is_clutter;
use super::resources::{
    accept_body, fetch_resource, resolve_data_url, resolve_reference, Fetcher, ResolvedResource,
    ResolvedResources, ResourceKind, ResourceRef,
};
use super::sanitize::{
    choose_srcset, collect, noscript_images, sanitize_with_resources, synthesize_archival_html,
};
use crate::features::web::services::identity;
use crate::features::web::services::web::public_web_client;
use crate::features::web::traits::page_snapshot::{
    CaptureError, CaptureProgress, CaptureRequest, DataUri, ProgressSink,
};

const PAGE: &str = "https://example.com/articles/page.html";

fn doc_url() -> Url {
    Url::parse(PAGE).unwrap()
}

fn captured_at() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 13, 21, 4, 11).unwrap()
}

fn sanitize(html: &str, resources: &ResolvedResources) -> String {
    let request = CaptureRequest {
        url: PAGE.to_string(),
        snapshot_id: "snap-1".to_string(),
    };
    sanitize_with_resources(html, &request, &doc_url(), resources, captured_at()).html
}

fn png() -> DataUri {
    DataUri {
        mime: "image/png".to_string(),
        bytes: b"\x89PNG\r\n\x1a\nfake".to_vec(),
    }
}

fn png_data_url() -> String {
    png().to_data_url()
}

fn sheet(text: &str) -> ResolvedResource {
    ResolvedResource::Inlined(DataUri {
        mime: "text/css".to_string(),
        bytes: text.as_bytes().to_vec(),
    })
}

fn resources(entries: &[(&str, ResolvedResource)]) -> ResolvedResources {
    let mut map = ResolvedResources::default();
    for (url, resolved) in entries {
        map.insert(*url, resolved.clone());
    }
    map
}

fn none() -> ResolvedResources {
    ResolvedResources::default()
}

// --- sanitizer -------------------------------------------------------------

#[test]
fn scripts_handlers_and_javascript_urls_are_removed_in_html_and_svg() {
    let html = r##"<html><head><script>alert(1)</script></head><body>
        <p onclick="alert(2)" onmouseover="alert(3)">Hi</p>
        <a href="javascript:alert(4)">j</a><a href="java&#x09;script:alert(5)">k</a>
        <img src="javascript:alert(6)" onerror="alert(7)">
        <svg onload="alert(8)"><script>alert(9)</script>
          <a xlink:href="javascript:alert(10)"><text>t</text></a>
          <animate attributeName="href" to="javascript:alert(11)"/><set attributeName="x" to="1"/>
          <foreignObject><div>alert(12)</div></foreignObject>
          <use href="https://evil.example/sprite.svg#a"/><use href="#local"/>
        </svg></body></html>"##;
    let out = sanitize(html, &none());
    assert!(!out.contains("alert"), "{out}");
    assert!(!out.to_ascii_lowercase().contains("javascript"), "{out}");
    assert!(!out.contains("<script") && !out.contains("onload") && !out.contains("onclick"));
    assert!(!out.contains("evil.example") && !out.contains("<animate") && !out.contains("<set"));
    assert!(out.contains("<p>Hi</p>"), "{out}");
    assert!(out.contains(r##"<use href="#local""##), "{out}");
}

#[test]
fn meta_refresh_page_csp_and_page_charset_are_removed() {
    let html = r#"<head><meta charset="iso-8859-1">
        <meta http-equiv="refresh" content="0;url=https://evil.example/">
        <meta http-equiv="Content-Security-Policy" content="default-src *">
        <meta name="referrer" content="unsafe-url"><base href="https://evil.example/">
        <meta name="description" content="kept"></head><body>x</body>"#;
    let out = sanitize(html, &none());
    assert!(
        !out.contains("refresh") && !out.contains("evil.example") && !out.contains("unsafe-url")
    );
    assert_eq!(out.matches("Content-Security-Policy").count(), 1);
    assert_eq!(out.matches("<meta charset").count(), 1);
    assert!(out.contains(r#"<meta name="description" content="kept">"#));
}

#[test]
fn forms_and_their_controls_are_removed_and_the_content_kept() {
    let html = r#"<form action="https://evil.example/post" method="post"><label>Name</label>
        <input name="q"><button formaction="https://evil.example/b">Go</button>
        <select><option>a</option></select><textarea>t</textarea><p>Kept text</p></form>"#;
    let out = sanitize(html, &none());
    for gone in [
        "<form",
        "<input",
        "<button",
        "<select",
        "<option",
        "<textarea",
        "evil.example",
    ] {
        assert!(!out.contains(gone), "{gone} survived: {out}");
    }
    assert!(out.contains("Name") && out.contains("<p>Kept text</p>"));
}

#[test]
fn http_links_are_routed_through_the_link_endpoint() {
    let html = r#"<a href="/next?a=1&amp;b=2" target="_blank" ping="https://t.example/" download>Next</a>
        <svg><a xlink:href="https://other.example/x"><text>s</text></a></svg>"#;
    let out = sanitize(html, &none());
    let target = "https://example.com/next?a=1&b=2";
    let routed = format!(
        r#"href="/l?u={}&amp;s=snap-1""#,
        urlencoding::encode(target)
    );
    assert!(out.contains(&routed), "{out}");
    assert!(
        out.contains(r#"data-lattice-href="https://example.com/next?a=1&amp;b=2""#),
        "{out}"
    );
    let svg_routed = format!(
        r#"href="/l?u={}&amp;s=snap-1""#,
        urlencoding::encode("https://other.example/x")
    );
    assert!(
        out.contains(&svg_routed) && !out.contains("xlink:href"),
        "{out}"
    );
    assert!(!out.contains("target=") && !out.contains("ping=") && !out.contains("download"));
}

#[test]
fn fragment_links_stay_and_mailto_is_unlinked() {
    let out = sanitize(
        r##"<a href="#section-2">Jump</a><a href="mailto:a@example.com">Mail</a><a href="tel:+100">Call</a>"##,
        &none(),
    );
    assert!(out.contains(r##"<a href="#section-2">Jump</a>"##), "{out}");
    assert!(
        out.contains("<a>Mail</a>") && out.contains("<a>Call</a>"),
        "{out}"
    );
    assert!(!out.contains("mailto") && !out.contains("tel:"));
}

#[test]
fn lol_html_delivers_noscript_content_as_text() {
    let elements = Cell::new(0);
    let text_seen = RefCell::new(String::new());
    let settings = lol_html::RewriteStrSettings::new()
        .append_element_content_handler(lol_html::element!("noscript img", |_el| {
            elements.set(elements.get() + 1);
            Ok(())
        }))
        .append_element_content_handler(lol_html::text!("noscript", |chunk| {
            text_seen.borrow_mut().push_str(chunk.as_str());
            Ok(())
        }));
    lol_html::rewrite_str(
        r#"<body><noscript><img src="a.png"></noscript></body>"#,
        settings,
    )
    .unwrap();
    assert_eq!(elements.get(), 0, "noscript children arrived as elements");
    assert!(
        text_seen.borrow().contains("<img"),
        "noscript content was not text"
    );
}

#[test]
fn noscript_keeps_only_its_images() {
    let map = resources(&[(
        "https://example.com/real.jpg",
        ResolvedResource::Inlined(png()),
    )]);
    let out = sanitize(
        r#"<noscript><img src="/real.jpg" alt="Real" onerror="alert(1)"><p>Enable JavaScript</p></noscript>"#,
        &map,
    );
    assert!(
        out.contains(r#"alt="Real""#) && out.contains(&png_data_url()),
        "{out}"
    );
    assert!(
        !out.contains("Enable JavaScript") && !out.contains("noscript") && !out.contains("alert")
    );

    let out = sanitize(
        r#"<noscript><p>Please enable JavaScript</p><style>body{display:none}</style></noscript>"#,
        &none(),
    );
    assert!(
        !out.contains("Please enable")
            && !out.contains("display:none")
            && !out.contains("noscript"),
        "{out}"
    );
    assert_eq!(noscript_images("<p>text only</p>"), None);
}

#[test]
fn lazy_data_src_is_promoted_over_a_placeholder() {
    let map = resources(&[(
        "https://example.com/img/lazy.jpg",
        ResolvedResource::Inlined(png()),
    )]);
    let out = sanitize(
        r#"<img src="data:image/gif;base64,R0lGODlhAQABAAAAACw=" data-src="/img/lazy.jpg" alt="L" loading="lazy">"#,
        &map,
    );
    assert!(
        out.contains(&format!(r#"src="{}""#, png_data_url())),
        "{out}"
    );
    assert!(!out.contains("data-src") && !out.contains("R0lGOD") && !out.contains("loading"));
}

#[test]
fn srcset_choice_prefers_w_up_to_1600_then_x_up_to_2() {
    assert_eq!(
        choose_srcset("a.jpg 400w, b.jpg 1600w, c.jpg 2400w").as_deref(),
        Some("b.jpg")
    );
    assert_eq!(
        choose_srcset("a.jpg 1x, b.jpg 2x, c.jpg 3x").as_deref(),
        Some("b.jpg")
    );
    assert_eq!(choose_srcset("a.jpg, b.jpg 1.5x").as_deref(), Some("b.jpg"));
    assert_eq!(
        choose_srcset("a.jpg 2400w, b.jpg 3200w").as_deref(),
        Some("a.jpg")
    );
    assert_eq!(
        choose_srcset("https://cdn.example/w_400,h_300/x.jpg 400w, https://cdn.example/w_800,h_600/x.jpg 800w").as_deref(),
        Some("https://cdn.example/w_800,h_600/x.jpg")
    );

    let map = resources(&[(
        "https://example.com/articles/big.jpg",
        ResolvedResource::Inlined(png()),
    )]);
    let out = sanitize(
        r#"<img srcset="small.jpg 1x, big.jpg 2x" sizes="100vw" decoding="async" fetchpriority="high">"#,
        &map,
    );
    assert!(out.contains(&png_data_url()), "{out}");
    for gone in ["srcset", "sizes", "decoding", "fetchpriority"] {
        assert!(!out.contains(gone), "{gone} survived: {out}");
    }
}

#[test]
fn picture_sources_are_dropped_and_the_img_kept() {
    let map = resources(&[(
        "https://example.com/a.jpg",
        ResolvedResource::Inlined(png()),
    )]);
    let out = sanitize(
        r#"<picture><source srcset="/a.webp" type="image/webp"><img src="/a.jpg"></picture>"#,
        &map,
    );
    assert!(!out.contains("<source") && !out.contains("a.webp"), "{out}");
    assert!(out.contains("<picture>") && out.contains(&png_data_url()));
}

#[test]
fn style_urls_are_inlined_or_their_declaration_dropped() {
    let map = resources(&[(
        "https://example.com/bg.png",
        ResolvedResource::Inlined(png()),
    )]);
    let html = r#"<style>.a{background:url("/bg.png");color:red}.b{background-image:url(https://tracker.example/beacon.gif);margin:0}</style>
        <div style="background: url(/bg.png); border: 1px solid">x</div>
        <div style="background-image: url('https://tracker.example/t.gif'); padding: 1px">y</div>"#;
    let out = sanitize(html, &map);
    assert!(
        out.contains(&format!(r#"url("{}")"#, png_data_url())),
        "{out}"
    );
    assert!(out.contains("color:red") && out.contains("margin:0") && out.contains("padding: 1px"));
    assert!(!out.contains("tracker.example"), "{out}");
}

#[test]
fn import_is_inlined_up_to_depth_three() {
    let map = resources(&[
        (
            "https://example.com/a.css",
            sheet("@import 'b.css'; .a{color:red}"),
        ),
        (
            "https://example.com/b.css",
            sheet("@import url(c.css); .b{color:green}"),
        ),
        (
            "https://example.com/c.css",
            sheet("@import \"d.css\"; .c{color:blue}"),
        ),
        ("https://example.com/d.css", sheet(".d{color:pink}")),
    ]);
    let out = sanitize("<style>@import url(/a.css);</style>", &map);
    assert!(
        out.contains(".a{color:red}")
            && out.contains(".b{color:green}")
            && out.contains(".c{color:blue}"),
        "{out}"
    );
    assert!(!out.contains(".d{") && !out.contains("@import"), "{out}");
}

#[test]
fn linked_stylesheets_become_style_and_cannot_close_it() {
    let map = resources(&[(
        "https://example.com/s.css",
        sheet(r#".p{color:red} .x::after{content:"</style><script>alert(1)</script>"}"#),
    )]);
    let out = sanitize(
        r#"<head><link rel="stylesheet" href="/s.css" media="print"><link rel="preload" href="/f.woff2"></head>"#,
        &map,
    );
    assert!(
        out.contains("@media print {") && out.contains(".p{color:red}"),
        "{out}"
    );
    assert!(
        out.contains(r#"<\/style><script>alert(1)<\/script>"#),
        "{out}"
    );
    assert_eq!(out.matches("</style>").count(), 2, "{out}");
    assert!(
        !out.contains("rel=\"stylesheet\"") && !out.contains("preload"),
        "{out}"
    );
}

#[test]
fn css_comments_are_stripped_before_scanning() {
    let html = r#"<style>/* url(https://evil.example/x.png) */ .x{color:red} .y{back/**/ground:u/**/rl(https://evil.example/y.png)}</style>"#;
    let out = sanitize(html, &none());
    assert!(
        !out.contains("evil.example") && !out.contains("/*"),
        "{out}"
    );
    assert!(out.contains(".x{color:red}"));
}

#[test]
fn expression_binding_and_behavior_declarations_are_dropped() {
    let html = r#"<style>.z{behavior:url(x.htc);-moz-binding:url(y.xml#b);color:green}</style>
        <div style="width: expression(alert(1)); color: blue">x</div>"#;
    let out = sanitize(html, &none());
    assert!(
        !out.contains("expression") && !out.contains("behavior") && !out.contains("-moz-binding"),
        "{out}"
    );
    assert!(out.contains("color:green") && out.contains("color: blue"));
}

#[test]
fn the_envelope_is_present_once_and_first() {
    let html = r#"<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.0//EN"><html lang="en"><!-- c --><head><meta charset="iso-8859-1"><title>T</title></head><body>b</body></html>"#;
    let out = sanitize(html, &none());
    assert!(out.starts_with("<!doctype html>\n"), "{out}");
    assert_eq!(out.to_ascii_lowercase().matches("<!doctype").count(), 1);
    assert!(
        out.contains(
            "<head><meta charset=\"utf-8\">\n<meta http-equiv=\"Content-Security-Policy\""
        ),
        "{out}"
    );
    assert_eq!(
        out.matches(r#"<meta name="lattice-snapshot" content="1">"#)
            .count(),
        1
    );
    assert!(
        out.contains(r#"<meta name="memento-datetime" content="Tue, 13 Oct 2026 21:04:11 GMT">"#)
    );
    assert!(out.contains(&format!(r#"<link rel="original" href="{PAGE}">"#)));
    assert!(out.contains(r#"<style id="lattice-base">"#) && !out.contains("<!-- c -->"));

    let bare = sanitize("<p>bare</p>", &none());
    assert!(
        bare.starts_with("<!doctype html>\n<head>\n<meta charset=\"utf-8\">"),
        "{bare}"
    );
    assert_eq!(bare.matches("lattice-snapshot").count(), 1);
}

#[test]
fn windows_1252_pages_decode_by_meta_and_by_header() {
    let page = b"<html><head><meta charset=\"windows-1252\"></head><body>caf\xe9</body></html>";
    assert!(decode_document(page, Some("text/html")).contains("caf\u{e9}"));
    assert_eq!(
        decode_document(b"caf\xe9", Some("text/html; charset=ISO-8859-1")),
        "caf\u{e9}"
    );
    assert_eq!(decode_document("caf\u{e9}".as_bytes(), None), "caf\u{e9}");
}

#[test]
fn clutter_is_removed_by_whole_token_only() {
    let html = r#"<div class="ad">AD-ONE</div><div id="sidebar-ads">AD-TWO</div>
        <ins class="adsbygoogle">AD-THREE</ins><div data-ad-slot="1">AD-FOUR</div>
        <div id="div-gpt-ad-123">AD-FIVE</div><div aria-label="Advertisement">AD-SIX</div>
        <address>Addr</address><header class="header">Head</header>
        <div class="loader shadow">Load</div><div id="main_header">Main</div><div class="adx-header">Keep</div>"#;
    let out = sanitize(html, &none());
    for gone in [
        "AD-ONE", "AD-TWO", "AD-THREE", "AD-FOUR", "AD-FIVE", "AD-SIX",
    ] {
        assert!(!out.contains(gone), "{gone} survived");
    }
    for kept in ["Addr", "Head", "Load", "Main", "Keep"] {
        assert!(out.contains(kept), "{kept} was removed");
    }
    let attrs =
        |class: &'static str| move |name: &str| (name == "class").then(|| class.to_string());
    assert!(!is_clutter("div", attrs("address header loader shadow")));
    assert!(!is_clutter("body", attrs("ads")));
}

#[test]
fn an_oversize_image_is_dropped_with_its_box_and_reason() {
    let map = resources(&[(
        "https://example.com/huge.jpg",
        ResolvedResource::Dropped {
            reason: "too_large".to_string(),
        },
    )]);
    let out = sanitize(
        r#"<img src="/huge.jpg" alt="Huge" width="800" height="600">"#,
        &map,
    );
    assert!(out.contains(r#"data-lattice-missing="too_large""#), "{out}");
    assert!(
        out.contains(r#"alt="Huge""#)
            && out.contains(r#"width="800""#)
            && out.contains(r#"height="600""#)
    );
    assert!(!out.contains("src="), "{out}");
}

#[test]
fn base_href_decides_resolution_and_is_removed() {
    let map = resources(&[(
        "https://cdn.example.com/assets/pic.png",
        ResolvedResource::Inlined(png()),
    )]);
    let out = sanitize(
        r#"<head><base href="https://cdn.example.com/assets/"></head><img src="pic.png">"#,
        &map,
    );
    assert!(
        out.contains(&png_data_url()) && !out.contains("<base"),
        "{out}"
    );
}

#[test]
fn pass_one_collects_every_role() {
    let html = r#"<head><title> A &amp; B </title><link rel="icon" href="/fav.ico">
        <link rel="stylesheet" href="/s.css"><style>.a{background:url(bg.png)}</style></head>
        <body><img src="/i.jpg"><noscript><img src="/n.jpg"></noscript><video poster="/p.jpg"></video>
        <svg><image href="/s.png"/><title>svg title</title></svg>
        <div style="background:url(/d.png)"></div><iframe src="/frame.html"></iframe></body>"#;
    let collected = collect(html, &doc_url());
    let urls: Vec<&str> = collected.refs.iter().map(|r| r.url.as_str()).collect();
    for expected in [
        "https://example.com/s.css",
        "https://example.com/articles/bg.png",
        "https://example.com/i.jpg",
        "https://example.com/n.jpg",
        "https://example.com/p.jpg",
        "https://example.com/s.png",
        "https://example.com/d.png",
    ] {
        assert!(urls.contains(&expected), "{expected} missing from {urls:?}");
    }
    assert!(!urls.iter().any(|u| u.contains("frame.html")));
    assert_eq!(
        collected.favicon.as_deref(),
        Some("https://example.com/fav.ico")
    );
    assert_eq!(collected.title.as_deref(), Some("A & B"));
}

#[test]
fn blocked_statuses_are_distinguished() {
    assert!(matches!(
        check_status(403),
        Err(CaptureError::Blocked { status: 403 })
    ));
    assert!(matches!(
        check_status(429),
        Err(CaptureError::Blocked { .. })
    ));
    assert!(matches!(check_status(404), Err(CaptureError::Refused(_))));
    assert!(check_status(200).is_ok());
}

// --- fetch policy ----------------------------------------------------------

#[tokio::test]
async fn a_private_address_is_refused_without_a_request() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let client = public_web_client().unwrap();
    let profile = identity::random_profile();
    for url in [
        format!("http://127.0.0.1:{port}/x.png"),
        "http://10.0.0.1/x.png".to_string(),
        "http://169.254.169.254/latest/meta-data".to_string(),
        "file:///etc/passwd".to_string(),
    ] {
        let resolved =
            fetch_resource(&client, profile, &url, PAGE, ResourceKind::Image, 1024).await;
        assert_eq!(
            resolved,
            ResolvedResource::Dropped {
                reason: "blocked_host".to_string()
            },
            "{url}"
        );
    }
    let accepted = listener.accept();
    assert!(
        matches!(accepted, Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock),
        "a request was made"
    );
    assert_eq!(
        resolve_reference(&doc_url(), "javascript:alert(1)"),
        Err("blocked_host")
    );
}

#[test]
fn a_data_url_over_the_cap_is_dropped() {
    let payload = base64::engine::general_purpose::STANDARD.encode(vec![0u8; 4096]);
    let url = format!("data:image/png;base64,{payload}");
    assert_eq!(
        resolve_data_url(&url, ResourceKind::Image, 1024),
        ResolvedResource::Dropped {
            reason: "too_large".to_string()
        }
    );
    assert!(matches!(
        resolve_data_url(&url, ResourceKind::Image, 8192),
        ResolvedResource::Inlined(_)
    ));
}

#[test]
fn a_wrong_content_type_is_dropped() {
    let bad = ResolvedResource::Dropped {
        reason: "bad_type".to_string(),
    };
    let html_body = b"<!doctype html><html><script>alert(1)</script></html>".to_vec();
    assert_eq!(
        accept_body(ResourceKind::Image, Some("image/png"), html_body.clone()),
        bad
    );
    assert_eq!(
        accept_body(
            ResourceKind::Image,
            Some("text/html; charset=utf-8"),
            png().bytes
        ),
        bad
    );
    assert_eq!(
        accept_body(ResourceKind::Font, Some("font/woff2"), html_body),
        bad
    );
    assert_eq!(
        accept_body(
            ResourceKind::Stylesheet,
            Some("text/plain"),
            b".a{}".to_vec()
        ),
        bad
    );
    assert_eq!(
        resolve_data_url(
            "data:text/html,<script>alert(1)</script>",
            ResourceKind::Image,
            1024
        ),
        bad
    );
    assert!(matches!(
        accept_body(ResourceKind::Font, Some("application/octet-stream"), b"wOF2rest".to_vec()),
        ResolvedResource::Inlined(DataUri { ref mime, .. }) if mime == "font/woff2"
    ));
}

#[tokio::test]
async fn the_resource_budget_sets_partial() {
    let client = public_web_client().unwrap();
    let progress: ProgressSink = Arc::new(|_: CaptureProgress| {});
    let fetcher = Fetcher::new(
        &client,
        identity::random_profile(),
        PAGE.to_string(),
        progress,
    );
    let refs: Vec<ResourceRef> = (0..401)
        .map(|i| ResourceRef {
            url: format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(format!("img{i}"))
            ),
            kind: ResourceKind::Image,
            depth: 0,
        })
        .collect();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    let outcome = fetcher.fetch_all(refs, deadline).await;
    assert!(
        outcome.partial_reasons.contains(&"over_budget".to_string()),
        "{:?}",
        outcome.partial_reasons
    );
    let report = outcome.resources.report();
    assert_eq!(report.inlined, 400);
    assert_eq!(report.dropped.len(), 1);
    assert_eq!(report.dropped[0].reason, "over_budget");
}

#[test]
fn preload_style_with_noscript_fallback_inlines_once() {
    let html = r#"<head>
        <link rel="preload" as="style" onload="this.onload=null;this.rel='stylesheet'" href="/theme.css">
        <noscript><link rel="stylesheet" href="/theme.css"></noscript>
    </head><body><p>Hello</p></body>"#;

    let collected = collect(html, &doc_url());
    let urls: Vec<&str> = collected.refs.iter().map(|r| r.url.as_str()).collect();
    assert!(
        urls.contains(&"https://example.com/theme.css"),
        "theme.css not collected: {urls:?}"
    );

    let map = resources(&[(
        "https://example.com/theme.css",
        sheet(".theme{background:#123;}"),
    )]);
    let out = sanitize(html, &map);
    assert!(
        out.contains(".theme{background:#123;}"),
        "sheet content missing: {out}"
    );
    assert_eq!(
        out.matches(".theme{background:#123;}").count(),
        1,
        "sheet should only be inlined once: {out}"
    );
    assert!(
        !out.contains("rel=\"preload\"")
            && !out.contains("rel=\"stylesheet\"")
            && !out.contains("rel='stylesheet'"),
        "link tags survived: {out}"
    );
    assert!(!out.contains("<noscript>"), "noscript tag survived: {out}");
}

#[test]
fn deferred_print_stylesheet_with_onload_inlines_without_media_wrap() {
    let html = r#"<head>
        <link rel="stylesheet" href="/deferred.css" media="print" onload="this.media='all'">
    </head><body><p>Hello</p></body>"#;

    let map = resources(&[(
        "https://example.com/deferred.css",
        sheet(".deferred{display:block;}"),
    )]);
    let out = sanitize(html, &map);
    assert!(out.contains(".deferred{display:block;}"), "{out}");
    assert!(
        !out.contains("@media print"),
        "deferred sheet was wrapped in @media print: {out}"
    );
}

#[test]
fn color_scheme_meta_is_stripped_and_base_layer_has_light_and_fouc_rules() {
    let html = r#"<head>
        <meta name="color-scheme" content="dark light">
    </head><body><p>Hello</p></body>"#;

    let out = sanitize(html, &none());
    assert!(
        !out.contains("color-scheme\" content=\"dark light\""),
        "color-scheme meta survived: {out}"
    );
    assert!(
        out.contains("color-scheme: light"),
        "base layer missing color-scheme: light: {out}"
    );
    assert!(
        out.contains("visibility: visible !important"),
        "base layer missing visibility: visible: {out}"
    );
    assert!(
        out.contains("#__next > div[style*=\\\"visibility:hidden\\\"]")
            || out.contains("#__next > div[style*=\"visibility:hidden\"]"),
        "base layer missing FOUC visibility rules: {out}"
    );
}

#[test]
fn synthesize_archival_html_produces_valid_enveloped_document() {
    let url = "https://example.com/article/1?foo=bar&baz=qux";
    let title = Some("A \"Special\" <Article> & Tale");
    let text =
        "Paragraph one with <script>alert(1)</script> & special \"chars\".\n\nParagraph two with 'quotes'.";
    let snapshot_id = "snap-test-42";
    let captured = captured_at();

    let out = synthesize_archival_html(url, title, text, snapshot_id, captured).unwrap();
    assert!(
        out.starts_with("<!doctype html>\n"),
        "missing doctype: {out}"
    );
    assert!(
        out.contains("<meta charset=\"utf-8\">"),
        "missing charset: {out}"
    );
    assert!(
        out.contains("<meta http-equiv=\"Content-Security-Policy\""),
        "missing CSP: {out}"
    );
    assert!(
        out.contains(r#"<meta name="lattice-snapshot" content="1">"#),
        "missing lattice-snapshot: {out}"
    );
    assert!(
        out.contains(r#"<meta name="memento-datetime" content="Tue, 13 Oct 2026 21:04:11 GMT">"#),
        "missing memento: {out}"
    );
    assert!(
        out.contains(
            r#"<link rel="original" href="https://example.com/article/1?foo=bar&amp;baz=qux">"#
        ),
        "missing original link: {out}"
    );
    assert!(
        out.contains("<title>A &quot;Special&quot; &lt;Article&gt; &amp; Tale</title>"),
        "title not escaped properly: {out}"
    );
    assert!(
        out.contains("<h1>A &quot;Special&quot; &lt;Article&gt; &amp; Tale</h1>"),
        "h1 not escaped properly: {out}"
    );
    assert!(
        out.contains(
            r#"/l?u=https%3A%2F%2Fexample.com%2Farticle%2F1%3Ffoo%3Dbar%26baz%3Dqux&amp;s=snap-test-42"#
        ),
        "missing routed url: {out}"
    );
    assert!(
        out.contains(
            "<p>Paragraph one with &lt;script&gt;alert(1)&lt;/script&gt; &amp; special &quot;chars&quot;.</p>"
        ),
        "paragraph 1 not escaped: {out}"
    );
    assert!(
        out.contains("<p>Paragraph two with &#39;quotes&#39;.</p>"),
        "paragraph 2 not escaped: {out}"
    );
    assert!(
        !out.contains("<script>alert(1)</script>"),
        "unescaped script found: {out}"
    );
}

#[tokio::test]
#[ignore = "requires live network access"]
async fn live_devblogs_capture_inlines_preload_stylesheets() {
    let engine = super::build_page_capture().unwrap();
    let progress: ProgressSink = Arc::new(|_: CaptureProgress| {});
    let cancel = tokio_util::sync::CancellationToken::new();
    let page = engine
        .capture(
            CaptureRequest {
                url: "https://devblogs.microsoft.com/dotnet/how-async-await-really-works/"
                    .to_string(),
                snapshot_id: "e56f333fd5c64c6b9dc24dc6bee0e788".to_string(),
            },
            progress,
            cancel,
        )
        .await
        .expect("live capture of devblogs.microsoft.com should succeed");
    assert!(
        page.html.contains("--clr-body-bg:#F9F8FE") || page.html.contains("--clr-body-bg: #F9F8FE"),
        "theme.min.css variables should be inlined into the snapshot"
    );
    assert!(
        !page
            .html
            .contains("name=\"color-scheme\" content=\"dark light\""),
        "dark light color-scheme meta should be stripped"
    );
    assert!(
        page.resources.inlined >= 45,
        "expected >= 45 resources (38 images + 11 stylesheets + fonts), got {}",
        page.resources.inlined
    );
}
