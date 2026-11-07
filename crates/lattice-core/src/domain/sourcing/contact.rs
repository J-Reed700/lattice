//! The one definition of an email address a contact may be.

/// An email address: one `@`, a non-empty local part, a dotted domain with
/// non-empty labels, and no whitespace, `/` or `:`. A URL with a user part
/// (`https://user@host.example/x`) is a URL contact, not an email. The check
/// is shape only and takes the value already trimmed.
pub fn is_email(value: &str) -> bool {
    if value
        .chars()
        .any(|c| c.is_whitespace() || c == '/' || c == ':')
    {
        return false;
    }
    let mut parts = value.split('@');
    let (Some(local), Some(domain), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    !local.is_empty() && domain.contains('.') && domain.split('.').all(|label| !label.is_empty())
}

#[cfg(test)]
#[path = "contact_tests.rs"]
mod tests;
