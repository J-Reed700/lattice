//! misaki's subtokenizer, which splits a written word into the parts the
//! lexicon reads: camel-case humps, numbers, hyphens, apostrophes and
//! symbols. Its regex needs look-ahead, which the `regex` crate lacks, so the
//! alternatives are tried by hand in the same order.

fn is_quote(c: char) -> bool {
    matches!(c, '\'' | '‘' | '’')
}

/// One step of the number pattern `\d?[,.]?\d`, in the order a backtracking
/// engine tries it.
fn number_step(chars: &[char], at: usize) -> Option<usize> {
    let digit = |i: usize| chars.get(i).is_some_and(char::is_ascii_digit);
    let separator = |i: usize| chars.get(i).is_some_and(|c| *c == ',' || *c == '.');
    if digit(at) && separator(at + 1) && digit(at + 2) {
        Some(at + 3)
    } else if (digit(at) || separator(at)) && digit(at + 1) {
        Some(at + 2)
    } else if digit(at) {
        Some(at + 1)
    } else {
        None
    }
}

/// Where the subtoken starting at `at` ends, or `None` when no alternative
/// matches there.
fn subtoken_end(chars: &[char], at: usize) -> Option<usize> {
    let c = *chars.get(at)?;
    let next = chars.get(at + 1).copied();
    let run = |from: usize, keep: &dyn Fn(char) -> bool| {
        chars
            .iter()
            .skip(from)
            .position(|&c| !keep(c))
            .map_or(chars.len(), |n| from + n)
    };

    // ^['‘’]+
    if at == 0 && is_quote(c) {
        return Some(run(at, &is_quote));
    }
    // \p{Lu}(?=\p{Lu}\p{Ll})
    if c.is_uppercase()
        && next.is_some_and(char::is_uppercase)
        && chars.get(at + 2).is_some_and(|c| c.is_lowercase())
    {
        return Some(at + 1);
    }
    // (?:^-)?(?:\d?[,.]?\d)+
    let start = if at == 0 && c == '-' { at + 1 } else { at };
    if let Some(mut end) = number_step(chars, start) {
        while let Some(further) = number_step(chars, end) {
            end = further;
        }
        return Some(end);
    }
    // [-_]+
    if c == '-' || c == '_' {
        return Some(run(at, &|c| c == '-' || c == '_'));
    }
    // ['‘’]{2,}
    if is_quote(c) && next.is_some_and(is_quote) {
        return Some(run(at, &is_quote));
    }
    // \p{L}*?(?:['‘’]\p{L})*?\p{Ll}(?=\p{Lu}): the shortest run of letters
    // ending in a lower-case letter that an upper-case one follows.
    if c.is_alphabetic() {
        for (i, &letter) in chars.iter().enumerate().skip(at) {
            if !letter.is_alphabetic() {
                break;
            }
            if letter.is_lowercase() && chars.get(i + 1).is_some_and(|c| c.is_uppercase()) {
                return Some(i + 1);
            }
        }
        // \p{L}+(?:['‘’]\p{L})*
        let mut end = run(at, &|c| c.is_alphabetic());
        while chars.get(end).copied().is_some_and(is_quote)
            && chars.get(end + 1).is_some_and(|c| c.is_alphabetic())
        {
            end += 2;
        }
        return Some(end);
    }
    // [^-_\p{L}'‘’\d]
    if !(c == '-' || c == '_' || is_quote(c) || c.is_ascii_digit()) {
        return Some(at + 1);
    }
    // ['‘’]+$
    if is_quote(c) && run(at, &is_quote) == chars.len() {
        return Some(chars.len());
    }
    None
}

pub(super) fn split(word: &str) -> Vec<String> {
    let chars: Vec<char> = word.chars().collect();
    let mut parts = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        match subtoken_end(&chars, at) {
            Some(end) if end > at => {
                parts.push(chars.iter().skip(at).take(end - at).collect());
                at = end;
            }
            _ => at += 1,
        }
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::split;

    #[test]
    fn splits_like_misaki() {
        assert_eq!(split("don't"), ["don't"]);
        assert_eq!(split("well-known"), ["well", "-", "known"]);
        assert_eq!(split("camelCase"), ["camel", "Case"]);
        assert_eq!(split("COVID-19"), ["COVID", "-", "19"]);
        assert_eq!(split("1,000.50"), ["1,000.50"]);
        assert_eq!(split("-5"), ["-5"]);
        assert_eq!(split("$5"), ["$", "5"]);
        assert_eq!(split("50%"), ["50", "%"]);
        assert_eq!(split("e.g"), ["e", ".", "g"]);
        assert_eq!(split("'quoted'"), ["'", "quoted", "'"]);
        assert_eq!(split("HTMLParser"), ["HTMLParser"]);
    }
}
