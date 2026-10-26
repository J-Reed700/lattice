//! A site's redirects are read one hop at a time, each status as the site
//! gave it; a hop the address guard refuses is never requested, and a walk
//! that ends on a server error or a 429 says nothing about where the site is.

use std::time::Duration;

use lattice_core::domain::sourcing::{Origin, SiteHome};
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;

/// The reader the app builds, over the real hop client.
fn real_reader() -> HttpHops {
    HttpHops::new(
        super::super::build_hop_client(Duration::from_secs(5)).unwrap(),
        Arc::new(SitePacer::with_interval(Duration::ZERO)),
    )
}

/// The real reader, its guard letting through only the local test server
/// (an address literal, which the client's resolver is never asked about):
/// every other address meets the guard every read meets.
fn reader() -> HttpHops {
    fn clear_but_the_test_server(address: &str) -> Result<Url, String> {
        let url = parse_fetchable_url(address).map_err(|error| error.to_string())?;
        if url.host_str() == Some("127.0.0.1") {
            return Ok(url);
        }
        cleared(address)
    }
    HttpHops {
        clear: clear_but_the_test_server,
        ..real_reader()
    }
}

fn redirect(status: u16, to: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).insert_header("location", to)
}

async fn serve(server: &MockServer, at: &str, reply: ResponseTemplate) {
    Mock::given(path(at))
        .respond_with(reply)
        .mount(server)
        .await;
}

fn at(server: &MockServer, address: &str) -> Url {
    Url::parse(&format!("{}{address}", server.uri())).unwrap()
}

#[tokio::test]
async fn each_hop_is_read_with_its_own_status_up_to_the_page() {
    let server = MockServer::start().await;
    serve(&server, "/", redirect(301, "/docs/")).await;
    serve(&server, "/docs/", redirect(308, "/docs")).await;
    serve(&server, "/docs", redirect(307, "/docs/en/home")).await;
    serve(&server, "/docs/en/home", ResponseTemplate::new(200)).await;

    let hops = reader().hops(&at(&server, "/")).await.unwrap();

    let read: Vec<(u16, Url)> = hops.into_iter().map(|hop| (hop.status, hop.to)).collect();
    assert_eq!(
        read,
        [
            (301, at(&server, "/docs/")),
            (308, at(&server, "/docs")),
            (307, at(&server, "/docs/en/home")),
        ]
    );
}

#[tokio::test]
async fn a_page_or_a_refusal_at_the_address_is_no_hop() {
    let server = MockServer::start().await;
    serve(&server, "/page", ResponseTemplate::new(200)).await;
    serve(&server, "/walled", ResponseTemplate::new(403)).await;
    serve(&server, "/bare", ResponseTemplate::new(301)).await;

    for address in ["/page", "/walled", "/bare"] {
        let hops = reader().hops(&at(&server, address)).await.unwrap();
        assert!(hops.is_empty(), "{address}: {hops:?}");
    }
}

#[tokio::test]
async fn a_server_error_or_a_429_anywhere_in_the_walk_is_no_answer() {
    let server = MockServer::start().await;
    serve(&server, "/down", ResponseTemplate::new(503)).await;
    serve(&server, "/busy", ResponseTemplate::new(429)).await;
    serve(&server, "/moved", redirect(301, "/down")).await;

    for address in ["/down", "/busy", "/moved"] {
        let read = reader().hops(&at(&server, address)).await;
        assert!(read.is_err(), "{address}: {read:?}");
    }
}

#[tokio::test]
async fn a_hop_the_guard_refuses_is_never_requested() {
    let server = MockServer::start().await;
    serve(&server, "/", redirect(301, "/next")).await;
    serve(
        &server,
        "/next",
        redirect(301, "http://169.254.169.254/latest/meta-data/"),
    )
    .await;
    serve(&server, "/local", redirect(301, "https://localhost/admin")).await;
    serve(&server, "/file", redirect(301, "file:///etc/passwd")).await;

    let hops = reader().hops(&at(&server, "/")).await.unwrap();
    assert_eq!(
        hops,
        [Hop {
            status: 301,
            to: at(&server, "/next"),
        }],
        "the walk stops before the refused hop"
    );
    for address in ["/local", "/file"] {
        let hops = reader().hops(&at(&server, address)).await.unwrap();
        assert!(hops.is_empty(), "{address}: {hops:?}");
    }
    let refused = real_reader().hops(&server.uri().parse().unwrap()).await;
    assert!(
        refused.is_err(),
        "the real guard refuses the local server itself"
    );
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        4,
        "/, /next, /local and /file, once each; the real guard sent nothing"
    );
}

#[tokio::test]
async fn a_redirect_loop_ends_at_the_redirect_limit() {
    let server = MockServer::start().await;
    serve(&server, "/a", redirect(302, "/b")).await;
    serve(&server, "/b", redirect(302, "/a")).await;

    let hops = reader().hops(&at(&server, "/a")).await.unwrap();

    assert_eq!(hops.len(), MAX_REDIRECTS);
}

/// Live: sites that moved, as they answer today. Run with
/// `cargo test -p lattice-desktop --lib web::hops -- --ignored`.
#[tokio::test]
#[ignore = "reads live sites"]
async fn moved_sites_are_followed_home_and_others_stay() {
    let reader = HttpHops::new(
        super::super::build_hop_client(Duration::from_secs(20)).unwrap(),
        Arc::new(SitePacer::with_interval(Duration::ZERO)),
    );
    for (asked, home) in [
        ("docs.anthropic.com", "platform.claude.com"),
        ("reactjs.org", "react.dev"),
        ("docs.microsoft.com", "learn.microsoft.com"),
        ("cookbook.openai.com", "developers.openai.com"),
        ("platform.openai.com/docs", "developers.openai.com/api"),
        ("docs.python.org", "docs.python.org"),
        ("json-schema.org", "json-schema.org"),
    ] {
        let asked = Origin::parse(asked).unwrap();
        let address = Url::parse(&format!("https://{asked}")).unwrap();
        let hops = reader.hops(&address).await.unwrap();
        let site = SiteHome::after(asked.clone(), &hops);
        assert_eq!(
            site.home(),
            &Origin::parse(home).unwrap(),
            "{asked}: {hops:?}"
        );
    }
}
