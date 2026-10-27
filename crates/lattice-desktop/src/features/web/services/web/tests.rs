//! Tests for page reading, search-result parsing and the private-address guard.
use super::page::{decode_response_text, read_bounded_body};
use super::*;
use crate::features::web::services::provenance::PageProvenance;
use std::time::Instant;
use url::Url;

#[test]
fn bounded_text_decode_honors_declared_charset_and_bom() {
    assert_eq!(
        decode_response_text(
            vec![0x93, b'h', b'i', 0x94],
            Some("text/html; charset=windows-1252")
        ),
        "“hi”"
    );
    assert_eq!(
        decode_response_text(
            b"\xef\xbb\xbfhello".to_vec(),
            Some("text/html; charset=windows-1252")
        ),
        "hello"
    );
}

#[tokio::test]
async fn bounded_body_rejects_oversized_chunked_response_without_length() {
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0u8; 1024];
        let _ = tokio::io::AsyncReadExt::read(&mut socket, &mut request).await;
        socket
            .write_all(
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
            )
            .await
            .unwrap();
        socket
            .write_all(b"a\r\n0123456789\r\n0\r\n\r\n")
            .await
            .unwrap();
    });
    let response = reqwest::Client::new()
        .get(format!("http://{address}/"))
        .send()
        .await
        .unwrap();
    let error = read_bounded_body(response, 8).await.unwrap_err();
    assert!(error.contains("exceeded 8 decoded bytes"), "{error}");
}

/// A service for the tests that never leave the process. The cache folder
/// is named but never created: nothing here stores a page.
fn test_service() -> WebService {
    WebService::new(&std::env::temp_dir().join("lattice-web-service-tests")).unwrap()
}

#[test]
fn test_validate_url() {
    let service = test_service();

    // Public IP literals exercise URL validation without making this unit
    // test depend on external DNS availability.
    assert!(service.validate_url("https://1.1.1.1").is_ok());
    assert!(service.validate_url("http://8.8.8.8").is_ok());

    // Invalid: localhost
    assert!(service.validate_url("http://localhost:8000").is_err());
    assert!(service.validate_url("http://127.0.0.1").is_err());

    // Invalid: private IP
    assert!(service.validate_url("http://192.168.1.1").is_err());
    assert!(service.validate_url("http://10.0.0.1").is_err());

    // Invalid: metadata endpoint
    assert!(service.validate_url("http://169.254.169.254").is_err());

    // Invalid: unsupported scheme
    assert!(service.validate_url("ftp://example.com").is_err());
    assert!(service.validate_url("file:///etc/passwd").is_err());
}

#[test]
fn blocked_ranges_cover_every_private_and_reserved_block() {
    let blocked = [
        "192.168.1.1",
        "10.0.0.1",
        "172.16.0.1",
        "127.0.0.1",
        "169.254.1.1",
        "169.254.169.254",
        "100.64.0.1",      // CGNAT low end
        "100.127.255.254", // CGNAT high end
        "0.0.0.0",
        "0.1.2.3", // 0.0.0.0/8
        "224.0.0.1",
        "255.255.255.255",
        "::1",
        "::",
        "::ffff:127.0.0.1",
        "::ffff:192.168.1.1",
        "::ffff:169.254.169.254",
        "64:ff9b::a00:1", // NAT64 of 10.0.0.1
        "fc00::1",
        "fd00:ec2::254",
        "fe80::1",
        "febf::1",
        "ff02::1",
    ];
    for ip in blocked {
        assert!(is_blocked_ip(ip.parse().unwrap()), "{ip} must be blocked");
    }

    let public = [
        "8.8.8.8",
        "1.1.1.1",
        "100.63.255.255", // just below CGNAT
        "100.128.0.1",    // just above CGNAT
        "::ffff:8.8.8.8",
        "2606:4700:4700::1111",
        "fec0::1", // just past fe80::/10
    ];
    for ip in public {
        assert!(!is_blocked_ip(ip.parse().unwrap()), "{ip} must be allowed");
    }
}

#[test]
fn literal_hosts_in_blocked_ranges_are_refused_from_the_url() {
    let service = test_service();
    for url in [
        "http://[::ffff:127.0.0.1]/",
        "http://[::ffff:c0a8:101]/",
        "http://100.64.1.1/",
        "http://0.0.0.0:8080/",
        "http://[fe80::1]/",
        "http://api.localhost/",
        "http://LOCALHOST./",
    ] {
        assert!(service.validate_url(url).is_err(), "{url} must be refused");
    }
}

#[tokio::test]
async fn resolver_refuses_names_that_resolve_privately() {
    use reqwest::dns::Resolve;
    let name: reqwest::dns::Name = "localhost".parse().unwrap();
    assert!(PublicOnlyResolver.resolve(name).await.is_err());
}

#[tokio::test]
async fn redirect_to_a_lan_address_is_refused_before_it_is_requested() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    // The redirect target: a "LAN device" that records whether it was hit.
    let target = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_port = target.local_addr().unwrap().port();
    let hit = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let hit_flag = hit.clone();
    tokio::spawn(async move {
        if target.accept().await.is_ok() {
            hit_flag.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    });

    // The "public" page that answers with a 302 to it. Reached directly by
    // a client with the policy alone, since the resolver would refuse the
    // loopback test server itself.
    let origin = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin_port = origin.local_addr().unwrap().port();
    tokio::spawn(async move {
        if let Ok((mut socket, _)) = origin.accept().await {
            let mut buf = [0u8; 1024];
            let _ = socket.read(&mut buf).await;
            let reply = format!(
                "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{target_port}/admin\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            let _ = socket.write_all(reply.as_bytes()).await;
        }
    });

    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(redirect_policy())
        .build()
        .unwrap();
    let result = client
        .get(format!("http://127.0.0.1:{origin_port}/"))
        .send()
        .await;

    let err = result.expect_err("the redirect must be refused");
    assert!(err.is_redirect(), "{err}");
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !hit.load(std::sync::atomic::Ordering::SeqCst),
        "the LAN address was contacted"
    );

    // Same refusal for the other literal forms a redirect can use.
    for location in [
        "http://192.168.1.1/",
        "http://[::ffff:10.0.0.1]/",
        "file:///etc/passwd",
    ] {
        let url = Url::parse(location).unwrap();
        assert!(
            parse_fetchable_url(url.as_str())
                .and_then(|u| check_url_host(&u))
                .is_err(),
            "{location}"
        );
    }
}

#[test]
fn test_extract_title() {
    let service = test_service();

    let html = r#"
            <html>
                <head><title>Test Page</title></head>
                <body></body>
            </html>
        "#;

    let title = service.extract_title(html);
    assert_eq!(title, Some("Test Page".to_string()));
}

#[test]
fn test_extract_article_text() {
    let service = test_service();

    let html = r#"
            <html>
                <body>
                    <article>
                        <h1>Main Title</h1>
                        <p>First paragraph.</p>
                        <p>Second paragraph.</p>
                    </article>
                </body>
            </html>
        "#;

    let text = service.extract_article_text(html);
    assert!(text.contains("Main Title"));
    assert!(text.contains("First paragraph"));
    assert!(text.contains("Second paragraph"));
}

/// A changelog's versions, an answer's code and a support table were dropped
/// by a reader that took only paragraphs and headings, and the read was still
/// called whole. A paragraph inside a list item is read once.
#[test]
fn lists_tables_code_and_quotes_are_read_once_each() {
    let service = test_service();
    let html = r#"
            <html><body><article>
                <h1>Release notes</h1>
                <ul><li>3.12 drops distutils</li><li><p>3.11 adds tomllib</p></li></ul>
                <pre>pip install --upgrade pip
python -m venv .venv</pre>
                <table><tr><th>Version</th><th>Support</th></tr><tr><td>3.8</td><td>ended</td></tr></table>
                <blockquote>Upgrade before October.</blockquote>
                <dl><dt>EOL</dt><dd>End of life</dd></dl>
            </article></body></html>
        "#;

    let text = service.extract_article_text(html);
    for fact in [
        "3.12 drops distutils",
        "python -m venv .venv",
        "Version | Support",
        "3.8 | ended",
        "Upgrade before October.",
        "End of life",
    ] {
        assert!(text.contains(fact), "missing {fact:?} in {text:?}");
    }
    assert_eq!(text.matches("3.11 adds tomllib").count(), 1, "{text}");
    let menu = service.extract_article_text(
        "<html><body><nav><ul><li>Home</li><li>Pricing</li></ul></nav>         <ul><li>Install with pip</li></ul><p>Read the guide.</p></body></html>",
    );
    assert!(!menu.contains("Pricing"), "a menu is not page text: {menu}");
    assert!(menu.contains("Install with pip"), "{menu}");
    assert!(
        text.contains("--upgrade pip\npython -m venv"),
        "code keeps its lines: {text:?}"
    );
}

/// Pages that failed in a real deep-research turn: one needs TLS 1.3, one
/// hides its body outside `<article>`. Run by hand; it needs the network.
#[tokio::test]
#[ignore = "requires network access"]
async fn live_pages_that_once_failed_are_read() {
    let dir = std::env::temp_dir().join(format!("lattice-live-read-{}", std::process::id()));
    let service = WebService::new(&dir).unwrap();
    for url in [
        "https://urbansurvivalsite.com/highest-calorie-crops-survival-garden/",
        "https://www.htecfarming.com/indoor-plants-that-dont-need-sun/",
        "https://www.frontiersin.org/journals/sustainable-food-systems/articles/10.3389/fsufs.2020.588988/full",
    ] {
        let page = service.read_page(url).await.unwrap_or_else(|e| panic!("{url}: {e}"));
        assert!(page.output.word_count > 500, "{url}: {} words", page.output.word_count);
    }
}

#[test]
fn related_article_cards_do_not_stand_in_for_the_body() {
    let service = test_service();
    let body = "<p>Perennial staple crops provide carbohydrates, proteins and fats.</p>".repeat(40);
    let html = format!(
        "<html><body><article class=\"Card\"><p>Related: another paper</p></article>\
             <div class=\"JournalFullText\">{body}</div></body></html>"
    );

    let text = service.extract_article_text(&html);
    assert!(text.matches("Perennial staple crops").count() == 40);
}

#[test]
fn test_site_key_aliases_mirrors_without_collapsing_public_suffixes() {
    for url in [
        "https://html.duckduckgo.com/html/?q=a&s=0",
        "https://duckduckgo.com/html/?q=a&s=0",
        "https://lite.duckduckgo.com/lite/?q=a&s=0",
    ] {
        assert_eq!(SitePacer::site_key(url).as_deref(), Some("duckduckgo.com"));
    }
    assert_eq!(
        SitePacer::site_key("https://www.bing.com/search?q=a").as_deref(),
        Some("bing.com")
    );
    assert_eq!(
        SitePacer::site_key("https://en.wikipedia.org/w/api.php?action=query").as_deref(),
        Some("en.wikipedia.org")
    );
    // Unrelated sites sharing a public suffix must not share a queue.
    assert_ne!(
        SitePacer::site_key("https://www.bbc.co.uk/news"),
        SitePacer::site_key("https://www.theguardian.co.uk/news")
    );
    assert_eq!(SitePacer::site_key("not a url"), None);
}

/// The whole page is kept and cached; the prompt fits what it shows to its
/// own budget.
#[tokio::test]
async fn a_chat_read_keeps_a_long_page_whole_and_caches_all_of_it() {
    let directory = tempfile::tempdir().unwrap();
    let service = WebService::new(directory.path()).unwrap();
    let text = format!(
        "{}THE_LAST_PARAGRAPH",
        "An article sentence. ".repeat(10_000)
    );
    assert!(text.chars().count() > 200_000);
    let url = "https://example.org/long-article";
    let page = service
        .keep_page(
            url,
            super::page::PageText {
                final_url: url.into(),
                title: Some("Long article".into()),
                content: text.clone(),
                content_type: Some("text/html".into()),
                via: Default::default(),
            },
            (Instant::now(), PageRead::Chat),
            PageProvenance::NotRead("kept directly by the test"),
        )
        .await;
    assert!(!page.output.content_truncated);
    assert_eq!(page.output.content, text);
    match service.page_cache.get(url).await {
        Some(crate::features::web::services::page_cache::CachedFetch::Page(cached)) => {
            assert!(cached.content.ends_with("THE_LAST_PARAGRAPH"));
        }
        _ => panic!("the chat read should be cached"),
    }
}

#[tokio::test]
async fn reference_capture_retains_large_page_tail_without_using_chat_cache() {
    let directory = tempfile::tempdir().unwrap();
    let service = WebService::new(directory.path()).unwrap();
    let text = format!(
        "{}FINAL_REFERENCE_PASSAGE",
        "Document paragraph. ".repeat(110_000)
    );
    let html = format!("<html><head><title>Full reference</title></head><body><article><p>{text}</p></article></body></html>");
    assert!(text.len() > 2_000_000);
    assert!(html.len() < MAX_PAGE_BODY_BYTES);
    let (title, extracted) = super::super::reference_text::extract(&html).unwrap();
    let url = "https://example.org/complete-reference";
    let page = service
        .keep_page(
            url,
            super::page::PageText {
                final_url: url.into(),
                title,
                content: extracted,
                content_type: Some("text/html".into()),
                via: Default::default(),
            },
            (Instant::now(), PageRead::Reference),
            PageProvenance::NotRead("kept directly by the test"),
        )
        .await;
    assert!(!page.output.content_truncated);
    assert!(page.output.content.len() > 2_000_000);
    assert!(page.output.content.ends_with("FINAL_REFERENCE_PASSAGE"));
    assert!(service.page_cache.get(url).await.is_none());
}

/// Two different hosts have nothing to wait on each other for.
#[tokio::test]
async fn first_contact_with_a_host_is_not_delayed() {
    let pacer = SitePacer::with_interval(Duration::from_secs(30));
    let started = Instant::now();
    drop(
        pacer
            .acquire("https://collider.com/silo-season-1-recap/")
            .await,
    );
    drop(
        pacer
            .acquire("https://fugitives.com/silo-season-2-full-recap/")
            .await,
    );
    drop(
        pacer
            .acquire("https://www.theastromech.com/2026/09/silo.html")
            .await,
    );
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "three different hosts waited {:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn test_site_pacer_spaces_same_site_requests() {
    let interval = Duration::from_millis(120);
    let pacer = SitePacer::with_interval(interval);

    // The first request to a site has nothing to pace against.
    let start = Instant::now();
    drop(pacer.acquire("https://html.duckduckgo.com/html/?q=a").await);
    assert!(
        start.elapsed() < interval,
        "first request to a site must not wait"
    );

    // A mirror of that site waits out the rest of the interval.
    let start = Instant::now();
    drop(pacer.acquire("https://lite.duckduckgo.com/lite/?q=b").await);
    let waited = start.elapsed();
    assert!(
        waited >= interval / 2,
        "same site must wait out the interval, waited {waited:?}"
    );

    // Another site is paced independently.
    let start = Instant::now();
    drop(pacer.acquire("https://www.bing.com/search?q=a").await);
    assert!(
        start.elapsed() < interval,
        "another site must not inherit the wait"
    );
}

#[tokio::test]
async fn test_site_pacer_serializes_same_site_only() {
    // A short interval keeps the assertions about the slot itself, not
    // about the pacing wait, which the test above covers.
    let pacer = SitePacer::with_interval(Duration::from_millis(1));
    let wait = Duration::from_millis(50);
    let ddg_slot = pacer.acquire("https://html.duckduckgo.com/html/?q=a").await;

    let bing_slot =
        tokio::time::timeout(wait, pacer.acquire("https://www.bing.com/search?q=a")).await;
    assert!(bing_slot.is_ok(), "another site must not wait");

    let mirror_slot =
        tokio::time::timeout(wait, pacer.acquire("https://lite.duckduckgo.com/lite/?q=b")).await;
    assert!(
        mirror_slot.is_err(),
        "same site must wait for the held slot"
    );

    drop(ddg_slot);
    let mirror_slot =
        tokio::time::timeout(wait, pacer.acquire("https://lite.duckduckgo.com/lite/?q=b")).await;
    assert!(mirror_slot.is_ok(), "released slot must be handed on");
}
