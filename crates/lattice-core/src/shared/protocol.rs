//! The name and version of every rule whose results are saved.
use serde::{Serialize, Serializer};
use std::fmt;

/// A rule whose results are saved, named with its version (`name-vN`): the
/// text goes into every key and fingerprint of what it saved, so a new
/// version reads nothing an older one wrote. Made only by [`crate::protocol!`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Protocol(&'static str);

impl Protocol {
    #[doc(hidden)]
    pub const fn from_versioned(text: &'static str) -> Self {
        Self(text)
    }

    pub const fn as_str(self) -> &'static str {
        self.0
    }

    /// The key of what this protocol saved for `scope`: `name-vN:scope`.
    pub fn key(self, scope: impl fmt::Display) -> String {
        format!("{}:{scope}", self.0)
    }

    /// Whether `saved` names this protocol.
    pub fn names(self, saved: &str) -> bool {
        self.0 == saved
    }
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl Serialize for Protocol {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.0)
    }
}

#[macro_export]
macro_rules! protocol {
    ($name:literal, $version:literal) => {{
        let _: u16 = $version;
        $crate::shared::protocol::Protocol::from_versioned(concat!($name, "-v", $version))
    }};
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn a_protocol_is_its_name_and_version() {
        let claim = crate::protocol!("claim", 1);
        assert_eq!(claim.to_string(), "claim-v1");
        assert_eq!(claim.key("abc"), "claim-v1:abc");
        assert_eq!(serde_json::to_value(claim).unwrap(), json!("claim-v1"));
        assert!(claim.names("claim-v1"));
        assert!(!claim.names("claim-v2"));
    }

    #[test]
    fn a_protocol_renders_the_text_saved_results_were_keyed_under() {
        assert_eq!(
            crate::protocol!("lesson-evidence", 22).as_str(),
            "lesson-evidence-v22"
        );
        assert_eq!(
            crate::protocol!("passage-coverage", 10).as_str(),
            "passage-coverage-v10"
        );
    }
}
