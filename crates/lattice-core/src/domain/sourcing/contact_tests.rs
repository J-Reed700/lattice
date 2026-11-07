use super::is_email;

#[test]
fn an_email_has_one_at_a_local_part_and_a_dotted_domain() {
    for good in ["me@example.org", "first.last+tag@mail.example.org", "a@b.c"] {
        assert!(is_email(good), "{good:?}");
    }
}

#[test]
fn what_is_not_an_email_is_refused() {
    for bad in [
        "",
        "me",
        "me@",
        "@example.org",
        "me@example",
        "me@.org",
        "me@example.",
        "me@exa..mple.org",
        "me@@example.org",
        "me@a@example.org",
        "me @example.org",
        "me@example.org ",
        "me@example.org\n",
        "me@example.org/x",
        "mailto:me@example.org",
    ] {
        assert!(!is_email(bad), "{bad:?}");
    }
}

#[test]
fn a_url_with_a_user_part_is_a_url_and_not_an_email() {
    assert!(!is_email("https://user@host.example/x"));
    assert!(!is_email("https://user@host.example"));
}
