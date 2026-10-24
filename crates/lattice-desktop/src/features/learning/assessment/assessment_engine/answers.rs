//! Typed answers: normalisation, numbers with their units, and matching a
//! typed answer against a text key.
use super::{LearningNumericKey, LearningTextKey};
use unicode_normalization::UnicodeNormalization;

/// Whitespace-collapsed, lowercased text with diacritics removed.
fn folded(value: &str) -> String {
    value
        .nfkd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .collect::<String>()
        .to_lowercase()
}

/// The normalised form of a typed answer: case, whitespace, punctuation,
/// diacritics, a leading article and thousands separators do not matter.
pub fn normalize_answer(value: &str) -> String {
    let folded = folded(value);
    let chars: Vec<char> = folded.chars().collect();
    let mut kept = String::with_capacity(chars.len());
    for (index, c) in chars.iter().enumerate() {
        let before = index.checked_sub(1).and_then(|i| chars.get(i));
        let after = chars.get(index + 1);
        let between_digits =
            before.is_some_and(char::is_ascii_digit) && after.is_some_and(char::is_ascii_digit);
        if (*c == ',' || *c == '\u{2009}' || *c == '_') && between_digits {
            continue; // thousands separator
        }
        if *c == '.' && between_digits {
            kept.push('.');
        } else if c.is_alphanumeric() {
            kept.push(*c);
        } else {
            kept.push(' ');
        }
    }
    let words: Vec<&str> = kept.split_whitespace().collect();
    let words = match words.split_first() {
        Some((first, rest)) if matches!(*first, "the" | "a" | "an") && !rest.is_empty() => rest,
        _ => &words[..],
    };
    words.join(" ")
}

/// The number a typed answer states, with an accepted unit removed. None when
/// the answer is not a number (or carries a unit the key does not accept).
pub fn parse_number(value: &str, unit_variants: &[String]) -> Option<f64> {
    let mut text = folded(value).trim().replace(['\u{2212}', '\u{2013}'], "-");
    for unit in unit_variants
        .iter()
        .map(|u| folded(u).trim().to_string())
        .filter(|u| !u.is_empty())
    {
        if let Some(stripped) = text.strip_suffix(unit.as_str()) {
            text = stripped.trim().to_string();
            break;
        }
    }
    let cleaned: String = {
        let chars: Vec<char> = text.chars().collect();
        chars
            .iter()
            .enumerate()
            .filter(|(index, c)| {
                let before = index.checked_sub(1).and_then(|i| chars.get(i));
                let after = chars.get(index + 1);
                !((**c == ',' || **c == ' ' || **c == '\u{2009}' || **c == '_')
                    && before.is_some_and(char::is_ascii_digit)
                    && after.is_some_and(char::is_ascii_digit))
            })
            .map(|(_, c)| *c)
            .collect()
    };
    let cleaned = cleaned.trim().trim_start_matches('+');
    if cleaned.is_empty() {
        return None;
    }
    if let Some((numerator, denominator)) = cleaned.split_once('/') {
        let n: f64 = numerator.trim().parse().ok()?;
        let d: f64 = denominator.trim().parse().ok()?;
        return (d != 0.0 && (n / d).is_finite()).then_some(n / d);
    }
    if !cleaned
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | 'e'))
    {
        return None;
    }
    cleaned.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// A number written for people: no trailing zeros, no exponent for ordinary
/// magnitudes.
pub fn show_number(value: f64) -> String {
    let text = format!("{value:.6}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".into()
    } else {
        text.into()
    }
}

pub(super) fn quoted(value: &str) -> String {
    format!("“{}”", value.trim())
}

pub(super) fn whole_words(haystack: &str, needle: &str) -> bool {
    !needle.is_empty() && format!(" {haystack} ").contains(&format!(" {needle} "))
}

pub(super) fn first_variant(key: &LearningTextKey) -> String {
    match &key.numeric {
        Some(n) => numeric_answer(n),
        None => key.variants.first().cloned().unwrap_or_default(),
    }
}

pub(super) fn numeric_answer(n: &LearningNumericKey) -> String {
    let unit = n
        .unit_variants
        .first()
        .map(|u| format!(" {u}"))
        .unwrap_or_default();
    if n.tolerance > 0.0 {
        format!(
            "{} ± {}{unit}",
            show_number(n.value),
            show_number(n.tolerance)
        )
    } else {
        format!("{}{unit}", show_number(n.value))
    }
}

/// Text against one text key: Some(true/false) when code decides, None when
/// only the model's equivalence judge can.
pub fn text_matches(answer: &str, key: &LearningTextKey) -> Option<bool> {
    let units = key
        .numeric
        .as_ref()
        .map(|n| n.unit_variants.as_slice())
        .unwrap_or(&[]);
    if let Some(given) = parse_number(answer, units) {
        if let Some(n) = &key.numeric {
            return Some((given - n.value).abs() <= n.tolerance + 1e-9 * n.value.abs().max(1.0));
        }
        return Some(key.variants.iter().any(|v| {
            parse_number(v, &[]).is_some_and(|expected| (expected - given).abs() <= 1e-9)
                || normalize_answer(v) == normalize_answer(answer)
        }));
    }
    let normalized = normalize_answer(answer);
    if key
        .variants
        .iter()
        .any(|v| normalize_answer(v) == normalized)
    {
        return Some(true);
    }
    None
}
