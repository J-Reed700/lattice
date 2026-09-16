//! Regression guard: selecting a mock credential store silently loses secrets.

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
#[test]
fn default_keyring_backend_is_persistent() {
    use keyring::credential::CredentialPersistence;
    let persistence = keyring::default::default_credential_builder().persistence();
    assert!(matches!(persistence, CredentialPersistence::UntilDelete));
}
