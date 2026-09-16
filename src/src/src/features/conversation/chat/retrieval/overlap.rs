use std::collections::HashSet;

/// Split free text into the lowercase alphanumeric tokens used for lexical
/// overlap comparisons between a query and a candidate passage.
pub(super) fn tokenize_overlap_terms(text: &str) -> HashSet<String> {
    let mut terms: HashSet<String> = HashSet::new();
    let mut current = String::new();

    for ch in text.chars() {
        if ch.is_alphanumeric() {
            for lower in ch.to_lowercase() {
                current.push(lower);
            }
        } else if !current.is_empty() {
            terms.insert(current);
            current = String::new();
        }
    }
    if !current.is_empty() {
        terms.insert(current);
    }

    terms
}
