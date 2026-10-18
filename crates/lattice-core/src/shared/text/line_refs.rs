//! The line-reference grammar shared by the prompt and the frontend parser.
//!
//! An answer points at lines in the folder with inline code whose whole
//! content is `path:line` or `path:start-end` (an en dash works too), the path
//! relative to the root with at least one `/` or `.` in it. Anything else in
//! backticks is ordinary code. The frontend parses the same pattern, so the two
//! must change together.

use once_cell::sync::Lazy;
use regex::Regex;

// Both patterns are constants and compile; `None` would only mean nothing is
// ever read as a reference, which is the safe direction.
static INLINE_CODE: Lazy<Option<Regex>> = Lazy::new(|| Regex::new(r"`([^`\n]+)`").ok());

static LINE_REFERENCE: Lazy<Option<Regex>> =
    Lazy::new(|| Regex::new(r"^([A-Za-z0-9_@.+\-/]+):(\d+)(?:[-–](\d+))?$").ok());

/// Whether `code` (the inside of one inline-code span) is a line reference.
pub fn is_line_reference(code: &str) -> bool {
    LINE_REFERENCE
        .as_ref()
        .and_then(|pattern| pattern.captures(code))
        .and_then(|captures| captures.get(1))
        .is_some_and(|path| path.as_str().contains(['/', '.']))
}

/// Whether `text` carries at least one line reference.
pub fn contains_line_reference(text: &str) -> bool {
    INLINE_CODE.as_ref().is_some_and(|pattern| {
        pattern
            .captures_iter(text)
            .filter_map(|captures| captures.get(1))
            .any(|code| is_line_reference(code.as_str()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_follow_the_grammar() {
        assert!(contains_line_reference(
            "See `src/main.rs:10-24` for the loop."
        ));
        assert!(contains_line_reference("It is set in `Cargo.toml:12`."));
        assert!(contains_line_reference("Range with a dash `lib/a.ts:3–9`."));
        assert!(contains_line_reference("`@scope/pkg/index.js:1`"));
        // A path needs a `/` or a `.`, so `Option:2` is ordinary code.
        assert!(!contains_line_reference("Use `Option:2` here."));
        assert!(!contains_line_reference("Call `foo(bar)` first."));
        assert!(!contains_line_reference(
            "Plain src/main.rs:10 outside backticks."
        ));
        assert!(!contains_line_reference(
            "`src/main.rs:10-` is not finished."
        ));
    }
}
