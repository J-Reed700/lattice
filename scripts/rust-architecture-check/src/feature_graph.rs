//! The feature graph allowlist, `feature-graph.txt`: feature edges that are
//! one feature built on a lower-level one, where a port would add indirection
//! without decoupling anything.
//!
//! Each line is `allow features::<a> features::<b>  # <reason>`; the reason is
//! required. References along an allowed edge leave `feature-edge` but still
//! feed the cycle check, so an allowed edge that closes a cycle fails like any
//! other. The allowed edges themselves must form no cycle, and an allowed edge
//! with no references left is STALE: delete its line.
use std::{fs, path::Path};

use crate::ratchet::{cycle_counts, cycle_route, Counts, Key, FEATURE_EDGE};

const FEATURE: &str = "features::";

/// One `allow` line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Allowed {
    pub subject: String,
    pub target: String,
    pub reason: String,
}

impl Allowed {
    /// The `feature-edge` key whose references this line allows.
    pub fn key(&self) -> Key {
        Key::new(FEATURE_EDGE, &self.subject, &self.target)
    }
}

pub fn parse(text: &str) -> Result<Vec<Allowed>, String> {
    let mut allowed: Vec<Allowed> = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let at = |problem: &str| format!("line {}: {problem}", index + 1);
        let (fields, reason) = line.split_once('#').unwrap_or((line, ""));
        let reason = reason.trim();
        if reason.is_empty() {
            return Err(at("an allowed edge needs a `# <reason>` comment"));
        }
        let fields: Vec<&str> = fields.split_whitespace().collect();
        let ["allow", subject, target] = fields[..] else {
            return Err(at(
                "expected `allow features::<a> features::<b>  # <reason>`",
            ));
        };
        let named = |feature: &str| {
            feature
                .strip_prefix(FEATURE)
                .is_some_and(|name| !name.is_empty() && !name.contains(':'))
        };
        if !named(subject) || !named(target) {
            return Err(at("both ends must be `features::<name>`"));
        }
        if subject == target {
            return Err(at("a feature cannot be allowed to reference itself"));
        }
        if allowed
            .iter()
            .any(|edge| edge.subject == subject && edge.target == target)
        {
            return Err(at("duplicate edge"));
        }
        allowed.push(Allowed {
            subject: subject.to_string(),
            target: target.to_string(),
            reason: reason.to_string(),
        });
    }
    Ok(allowed)
}

/// Reads and parses the allowlist, naming the file in any error.
pub fn load(path: &Path) -> Result<Vec<Allowed>, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// The cycles the allowed edges form on their own, one route per edge on a
/// cycle. An allowlist that permits a cycle would allow the tangle the
/// `feature-cycle` rule exists to prevent.
pub fn cycles(allowed: &[Allowed]) -> Vec<String> {
    let edges: Counts = allowed.iter().map(|edge| (edge.key(), 1)).collect();
    cycle_counts(&edges)
        .keys()
        .filter_map(|key| cycle_route(&edges, &key.subject, &key.target))
        .collect()
}

/// Moves the allowed `feature-edge` counts out of `counts`, returning them.
pub fn take_allowed(counts: &mut Counts, allowed: &[Allowed]) -> Counts {
    allowed
        .iter()
        .filter_map(|edge| {
            let key = edge.key();
            counts.remove(&key).map(|count| (key, count))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(subject: &str, target: &str) -> Allowed {
        Allowed {
            subject: format!("features::{subject}"),
            target: format!("features::{target}"),
            reason: "reason".into(),
        }
    }

    #[test]
    fn parses_allowed_edges_with_their_reasons() {
        let text = "\
# header

allow features::batch features::indexing  # batch runs indexing use cases
allow features::compare features::search # compare is a thin layer over search
";
        assert_eq!(
            parse(text).unwrap(),
            vec![
                Allowed {
                    subject: "features::batch".into(),
                    target: "features::indexing".into(),
                    reason: "batch runs indexing use cases".into(),
                },
                Allowed {
                    subject: "features::compare".into(),
                    target: "features::search".into(),
                    reason: "compare is a thin layer over search".into(),
                },
            ]
        );
    }

    #[test]
    fn every_allowed_edge_needs_a_reason() {
        let missing = parse("allow features::a features::b").unwrap_err();
        assert!(
            missing.contains("line 1") && missing.contains("reason"),
            "{missing}"
        );
        assert!(parse("allow features::a features::b  #   ").is_err());
    }

    #[test]
    fn rejects_malformed_self_and_duplicate_edges() {
        for text in [
            "permit features::a features::b # r",
            "allow features::a # r",
            "allow features::a features::b features::c # r",
            "allow a features::b # r",
            "allow features:: features::b # r",
            "allow features::a::dto features::b # r",
            "allow features::a features::a # r",
            "allow features::a features::b # r\nallow features::a features::b # again",
        ] {
            assert!(parse(text).is_err(), "{text}");
        }
    }

    #[test]
    fn the_allowed_edges_must_not_form_a_cycle() {
        assert!(cycles(&[edge("a", "b"), edge("b", "c"), edge("a", "c")]).is_empty());
        assert_eq!(
            cycles(&[
                edge("a", "b"),
                edge("b", "c"),
                edge("c", "a"),
                edge("d", "a")
            ]),
            vec![
                "features::a -> features::b -> features::c -> features::a",
                "features::b -> features::c -> features::a -> features::b",
                "features::c -> features::a -> features::b -> features::c",
            ]
        );
    }

    #[test]
    fn allowed_counts_move_out_and_missing_ones_are_absent() {
        let mut counts = Counts::from([(edge("a", "b").key(), 3), (edge("a", "c").key(), 1)]);
        let taken = take_allowed(&mut counts, &[edge("a", "b"), edge("x", "y")]);
        assert_eq!(taken, Counts::from([(edge("a", "b").key(), 3)]));
        assert_eq!(counts, Counts::from([(edge("a", "c").key(), 1)]));
    }
}
