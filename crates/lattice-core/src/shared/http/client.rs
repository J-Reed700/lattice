use std::time::Duration;

use reqwest::ClientBuilder;

/// How long a connection may go silent before it is probed.
const KEEPALIVE: Duration = Duration::from_secs(30);
/// How long an HTTP/2 PING may go unanswered before the connection is dropped.
const KEEPALIVE_ANSWER: Duration = Duration::from_secs(20);

/// Build a reqwest client builder with safe defaults for tests.
///
/// On macOS, reqwest's system proxy lookup can panic in sandboxed environments.
/// We disable system proxies when running tests or when the
/// RECALL_DISABLE_SYSTEM_PROXY env var is set.
pub fn reqwest_client_builder() -> ClientBuilder {
    // Chosen explicitly: other crates switch on reqwest's native-tls, and with
    // both compiled in reqwest picks native-tls — Secure Transport on macOS,
    // which cannot speak TLS 1.3 or HTTP/2. See the note in Cargo.toml.
    let mut builder = reqwest::Client::builder()
        .use_rustls_tls()
        // A connection that dies mid-response (a dropped network, a laptop
        // that slept) otherwise holds its request forever: a non-streamed
        // answer has no read deadline unless the caller set a budget. TCP
        // keepalive notices a dead peer under HTTP/1.1, PINGs under HTTP/2.
        .tcp_keepalive(KEEPALIVE)
        .http2_keep_alive_interval(KEEPALIVE)
        .http2_keep_alive_timeout(KEEPALIVE_ANSWER)
        .http2_keep_alive_while_idle(true);
    if should_disable_system_proxy() {
        builder = builder.no_proxy();
    }
    builder
}

pub fn should_disable_system_proxy() -> bool {
    if cfg!(test) {
        return true;
    }

    std::env::var("RECALL_DISABLE_SYSTEM_PROXY")
        .map(|value| value != "0" && !value.eq_ignore_ascii_case("false"))
        .unwrap_or(false)
}
