//! What a client, or a service reading through one, must do when a server on
//! this machine stands behind the address: the checks every path that reads
//! the public web shares.

use lattice_core::shared::error::{AppError, Result};
use reqwest::Client;
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

const PAGE: &str = "<html><body><p>Served by this machine.</p></body></html>";

/// A server on this machine that answers every request with a page.
pub(crate) async fn local_page() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200).set_body_raw(PAGE, "text/html; charset=utf-8"))
        .mount(&server)
        .await;
    server
}

/// Loopback written as an IPv4-mapped IPv6 literal.
pub(crate) fn mapped_loopback(port: u16) -> String {
    format!("http://[::ffff:127.0.0.1]:{port}/")
}

/// `localhost` written with the trailing dot of a fully qualified name.
pub(crate) fn dotted_localhost(port: u16) -> String {
    format!("http://localhost.:{port}/")
}

/// How many requests have reached `server`.
pub(crate) async fn requests_received(server: &MockServer) -> usize {
    server.received_requests().await.unwrap().len()
}

/// Asserts that `result`, the read of `url`, was refused as an invalid URL and
/// that `server`, which `url` reaches, was not contacted.
pub(crate) async fn assert_refused_without_contact<T: std::fmt::Debug>(
    server: &MockServer,
    url: &str,
    result: &Result<T>,
) {
    assert_eq!(
        requests_received(server).await,
        0,
        "{url} reached the local server"
    );
    assert!(
        matches!(result, Err(AppError::InvalidUrl(_))),
        "{url} must be refused as an invalid URL, got {result:?}"
    );
}

/// Makes `server` answer every request with a redirect to `location`.
pub(crate) async fn redirect_every_request_to(server: &MockServer, location: String) {
    Mock::given(any())
        .respond_with(ResponseTemplate::new(302).insert_header("location", location))
        .mount(server)
        .await;
}

/// Asserts that `client` refuses a redirect to a server on this machine and
/// requests neither that server nor the page it would send the client on to.
pub(crate) async fn assert_refuses_a_redirect_through_a_local_server(client: &Client) {
    let landing = local_page().await;
    let middle = MockServer::start().await;
    redirect_every_request_to(&middle, format!("{}/page", landing.uri())).await;
    let origin = MockServer::start().await;
    redirect_every_request_to(&origin, format!("{}/admin", middle.uri())).await;

    let result = client.get(origin.uri()).send().await;

    assert_eq!(
        requests_received(&middle).await,
        0,
        "the client requested the local server a redirect named"
    );
    assert_eq!(
        requests_received(&landing).await,
        0,
        "the client followed a redirect on past the local server"
    );
    assert!(
        matches!(&result, Err(error) if error.is_redirect()),
        "the redirect must be refused by the client's redirect policy, got {result:?}"
    );
}

/// Asserts that `client` refuses a name that resolves to this machine.
pub(crate) async fn assert_refuses_a_name_that_resolves_to_this_machine(client: &Client) {
    let server = local_page().await;
    let url = dotted_localhost(server.address().port());

    let result = client.get(&url).send().await;

    assert_eq!(
        requests_received(&server).await,
        0,
        "{url} reached the local server"
    );
    assert!(
        result.is_err(),
        "{url} must be refused by the client's resolver, got {result:?}"
    );
}
