use super::loopback_client_builder;

#[test]
fn a_loopback_client_builds() {
    assert!(loopback_client_builder().build().is_ok());
}
