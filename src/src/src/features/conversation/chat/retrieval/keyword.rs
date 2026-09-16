pub(super) fn tokenize_keyword_terms(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            for lower in ch.to_lowercase() {
                current.push(lower);
            }
        } else if !current.is_empty() {
            if let Some(token) = normalize_keyword_token(&current) {
                tokens.push(token);
            }
            current = String::new();
        }
    }
    if !current.is_empty() {
        if let Some(token) = normalize_keyword_token(&current) {
            tokens.push(token);
        }
    }
    tokens
}

pub(super) fn normalize_keyword_token(token: &str) -> Option<String> {
    let trimmed = token.trim();
    if trimmed.len() < 3 {
        return None;
    }
    if trimmed.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    if !trimmed.chars().any(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    Some(trimmed.to_string())
}

pub(super) fn extract_phrase_terms(
    tokens: &[String],
    min_n: usize,
    max_n: usize,
    max_terms: usize,
) -> Vec<String> {
    if tokens.len() < min_n || max_terms == 0 {
        return Vec::new();
    }

    let mut phrases = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for n in min_n..=max_n {
        if tokens.len() < n {
            continue;
        }
        for window in tokens.windows(n) {
            let phrase = window.join(" ");
            if seen.insert(phrase.clone()) {
                phrases.push(phrase);
                if phrases.len() >= max_terms {
                    return phrases;
                }
            }
        }
    }
    phrases
}
