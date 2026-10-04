use reqwest::ClientBuilder;

/// Build a reqwest client builder with safe defaults for tests.
///
/// On macOS, reqwest's system proxy lookup can panic in sandboxed environments.
/// We disable system proxies when running tests or when the
/// RECALL_DISABLE_SYSTEM_PROXY env var is set.
pub fn reqwest_client_builder() -> ClientBuilder {
    // Chosen explicitly: other crates switch on reqwest's native-tls, and with
    // both compiled in reqwest picks native-tls — Secure Transport on macOS,
    // which cannot speak TLS 1.3 or HTTP/2. See the note in Cargo.toml.
    let mut builder = reqwest::Client::builder().use_rustls_tls();
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
