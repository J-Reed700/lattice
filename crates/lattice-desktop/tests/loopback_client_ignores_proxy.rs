#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use lattice_core::shared::http::loopback_client_builder;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

/// The only test in this binary, so setting process-wide variables is safe.
#[tokio::test]
async fn a_loopback_request_does_not_go_through_the_environment_proxy() {
    let closed_port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let unreachable_proxy = format!("http://127.0.0.1:{closed_port}");
    for variable in [
        "HTTP_PROXY",
        "http_proxy",
        "HTTPS_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
    ] {
        std::env::set_var(variable, &unreachable_proxy);
    }
    for variable in ["NO_PROXY", "no_proxy"] {
        std::env::remove_var(variable);
    }

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let client = loopback_client_builder().build().unwrap();
    let response = client
        .get(format!("{}/health", server.uri()))
        .send()
        .await
        .expect("a loopback request must not use the proxy");
    assert!(response.status().is_success());
}
