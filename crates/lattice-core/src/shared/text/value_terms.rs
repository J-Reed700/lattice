//! Value terms: the words of a sentence that carry one particular value of
//! what it is about (a number, a month, a name) and the words that negate it.
//!
//! A sentence can keep every word of its subject and still say something else
//! by changing one of these: "retries three times" and "retries five times",
//! "released in March" and "released in May", "does" and "does not". Two
//! features read them: the chat's lexical pre-filter, which never lets a copy
//! settle a claim when the copy changed one, and lesson claim checking, which
//! searches for passages that give the claim's subject another value.

/// Tokens that flip a sentence's meaning without changing its vocabulary,
/// each padded as [`padded_lowercase`] pads text.
pub const NEGATION_MARKERS: &[&str] = &[
    " not ",
    "n't ",
    " no ",
    " never ",
    " without ",
    " cannot ",
    " none ",
    " neither ",
    " nor ",
    " unlike ",
    " instead of ",
    " rather than ",
    " fails to ",
    " failed to ",
    " unable to ",
    " only ",
    " except ",
];

/// Month names, padded as [`padded_lowercase`] pads text. "May" is left out:
/// it is far more often the verb than the month.
pub const MONTH_MARKERS: &[&str] = &[
    " january ",
    " february ",
    " march ",
    " april ",
    " june ",
    " july ",
    " august ",
    " september ",
    " october ",
    " november ",
    " december ",
];

/// `text` lowercased between single spaces, with punctuation flattened to
/// spaces so " not " matches "did not." as well.
pub fn padded_lowercase(text: &str) -> String {
    let mut padded = String::with_capacity(text.len() + 2);
    padded.push(' ');
    for ch in text.chars() {
        if ch.is_alphanumeric() || ch == '\'' || ch == '\u{2019}' {
            padded.extend(ch.to_lowercase());
        } else {
            padded.push(' ');
        }
    }
    padded.push(' ');
    padded
}

/// Words after the first that start with a capital letter, lowercased: names
/// of people, teams, places and days. The first is skipped because every
/// sentence starts with one.
pub fn capitalised_words(sentence: &str) -> impl Iterator<Item = String> + '_ {
    sentence
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .skip(1)
        .filter(|word| {
            word.chars().count() > 1 && word.chars().next().is_some_and(char::is_uppercase)
        })
        .map(str::to_lowercase)
}

/// Whether `word` carries a number written in digits.
pub fn has_digit(word: &str) -> bool {
    word.chars().any(|ch| ch.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn padding_lets_a_marker_match_at_punctuation() {
        assert!(padded_lowercase("It did not.").contains(" not "));
        assert!(padded_lowercase("It doesn't.").contains("n't "));
        assert!(MONTH_MARKERS
            .iter()
            .any(|month| padded_lowercase("Released in March, 2024").contains(month)));
    }

    #[test]
    fn the_first_word_is_never_a_name() {
        let names: Vec<_> = capitalised_words("Teams in Lisbon meet Kim on Friday").collect();
        assert_eq!(names, ["lisbon", "kim", "friday"]);
        assert!(has_digit("v2") && !has_digit("two"));
    }
}
