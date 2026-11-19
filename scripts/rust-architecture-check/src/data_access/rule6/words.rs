//! Whether SQL text writes: one of the writing words stands at parenthesis depth 0 once
//! comments, strings and quoted identifiers are removed.

const WRITE_WORDS: [&str; 9] = [
    "insert", "update", "delete", "create", "drop", "alter", "vacuum", "analyze", "reindex",
];
const PRAGMA_WRITES: [&str; 3] = ["optimize", "wal_checkpoint", "incremental_vacuum"];

enum Token {
    Word {
        text: String,
        depth: usize,
        /// The next character that is not a space is `(`.
        called: bool,
    },
    Equals {
        depth: usize,
    },
    Semicolon,
}

/// The index after the closing `quote` of the quoted run that starts at `start`; a doubled
/// quote inside the run is part of it.
fn skip_quoted(chars: &[char], start: usize, quote: char) -> usize {
    let mut at = start + 1;
    while let Some(&c) = chars.get(at) {
        if c == quote {
            if chars.get(at + 1) == Some(&quote) {
                at += 2;
            } else {
                return at + 1;
            }
        } else {
            at += 1;
        }
    }
    at
}

/// The text with every comment, string and quoted identifier replaced by one space.
fn stripped(sql: &str) -> Vec<char> {
    let chars: Vec<char> = sql.chars().collect();
    let mut out = Vec::with_capacity(chars.len());
    let mut at = 0;
    while let Some(&c) = chars.get(at) {
        let next = chars.get(at + 1).copied();
        match (c, next) {
            ('-', Some('-')) => {
                while chars.get(at).is_some_and(|&c| c != '\n') {
                    at += 1;
                }
                out.push(' ');
            }
            ('/', Some('*')) => {
                at += 2;
                while let Some(&c) = chars.get(at) {
                    if c == '*' && chars.get(at + 1) == Some(&'/') {
                        at += 2;
                        break;
                    }
                    at += 1;
                }
                out.push(' ');
            }
            ('\'' | '"' | '`', _) => {
                at = skip_quoted(&chars, at, c);
                out.push(' ');
            }
            ('[', _) => {
                at = chars
                    .iter()
                    .skip(at + 1)
                    .position(|&c| c == ']')
                    .map_or(chars.len(), |found| at + found + 2);
                out.push(' ');
            }
            _ => {
                out.push(c);
                at += 1;
            }
        }
    }
    out
}

fn tokens(chars: &[char]) -> Vec<Token> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut at = 0;
    while let Some(&c) = chars.get(at) {
        if c.is_alphanumeric() || c == '_' {
            let start = at;
            while chars
                .get(at)
                .is_some_and(|&c| c.is_alphanumeric() || c == '_')
            {
                at += 1;
            }
            let text: String = chars.get(start..at).unwrap_or_default().iter().collect();
            let called = chars
                .get(at..)
                .and_then(|rest| rest.iter().find(|c| !c.is_whitespace()))
                == Some(&'(');
            out.push(Token::Word {
                text: text.to_lowercase(),
                depth,
                called,
            });
            continue;
        }
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ';' => out.push(Token::Semicolon),
            '=' => {
                let before = at.checked_sub(1).and_then(|i| chars.get(i)).copied();
                let after = chars.get(at + 1).copied();
                if !matches!(before, Some('<' | '>' | '!' | '=')) && after != Some('=') {
                    out.push(Token::Equals { depth });
                }
            }
            _ => {}
        }
        at += 1;
    }
    out
}

/// Whether the SQL writes.
pub(super) fn writes(sql: &str) -> bool {
    // Depth-0 words seen since `PRAGMA` in the current statement.
    let mut pragma: Option<usize> = None;
    for token in tokens(&stripped(sql)) {
        match token {
            Token::Semicolon => pragma = None,
            Token::Equals { depth: 0 } => {
                if pragma.is_some() {
                    return true;
                }
            }
            Token::Equals { .. } => {}
            Token::Word {
                text,
                depth: 0,
                called,
            } => {
                let word = text.as_str();
                if WRITE_WORDS.contains(&word) || (word == "replace" && !called) {
                    return true;
                }
                if word == "pragma" {
                    pragma = Some(0);
                } else if let Some(seen) = pragma {
                    if seen < 2 && PRAGMA_WRITES.contains(&word) {
                        return true;
                    }
                    pragma = Some(seen + 1);
                }
            }
            Token::Word { .. } => {}
        }
    }
    false
}
