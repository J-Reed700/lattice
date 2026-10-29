//! British English text to Kokoro's phonemes: misaki's `G2P` pipeline over
//! its lexicon, with the BART fallback for words neither dictionary has.
//! spaCy's tokenizer is approximated by splitting punctuation off the ends of
//! each whitespace-separated chunk; misaki's subtokenizer runs on the rest.

mod fallback;
mod lexicon;
mod numbers;
mod subtokens;

#[cfg(test)]
mod tests;

use std::path::Path;

use unicode_normalization::UnicodeNormalization;

use fallback::Fallback;
use lexicon::{
    apply_stress, currency_units, stress_weight, Context, Lexicon, NON_QUOTE_PUNCTS, PRIMARY,
    PUNCTS,
};

/// Spoken for a word nothing could read; not in Kokoro's vocabulary, so it
/// is silent.
const UNKNOWN: &str = "❓";
const JUNK: &str = "',-._‘’/";

#[derive(Debug, Default)]
struct Token {
    text: String,
    whitespace: bool,
    phonemes: Option<String>,
    is_head: bool,
    prespace: bool,
    currency: Option<String>,
}

impl Token {
    fn new(text: &str) -> Self {
        Self {
            text: text.to_string(),
            is_head: true,
            ..Default::default()
        }
    }

    fn punctuation(text: &str) -> Self {
        let phonemes = match text {
            "(" | "[" | "{" => "(".to_string(),
            ")" | "]" | "}" => ")".to_string(),
            "-" | "–" => "—".to_string(),
            _ => text.chars().filter(|c| PUNCTS.contains(*c)).collect(),
        };
        Self {
            phonemes: Some(phonemes),
            ..Self::new(text)
        }
    }
}

enum Word {
    Single(Token),
    Group(Vec<Token>),
}

impl Word {
    fn last_mut(&mut self) -> Option<&mut Token> {
        match self {
            Word::Single(token) => Some(token),
            Word::Group(tokens) => tokens.last_mut(),
        }
    }
}

fn is_junk(text: &str) -> bool {
    text.chars().all(|c| JUNK.contains(c))
}

const OPENERS: &str = "\"“([{«";
const CLOSERS: &str = ".,;:!?…\"”)]}»";
const BREAKS: [char; 3] = ['—', '–', '…'];

/// Words and punctuation of `text`, each knowing whether a space follows.
fn words(text: &str) -> Vec<Word> {
    let mut words = Vec::new();
    for chunk in text.split_whitespace() {
        let mut pieces: Vec<&str> = Vec::new();
        let mut start = 0;
        for (at, c) in chunk.char_indices() {
            if BREAKS.contains(&c) {
                pieces.extend(chunk.get(start..at).filter(|p| !p.is_empty()));
                pieces.extend(chunk.get(at..at + c.len_utf8()));
                start = at + c.len_utf8();
            }
        }
        pieces.extend(chunk.get(start..).filter(|p| !p.is_empty()));

        for piece in pieces {
            if piece.chars().all(|c| BREAKS.contains(&c)) {
                words.push(Word::Single(Token::punctuation(piece)));
                continue;
            }
            let core = piece.trim_start_matches(|c| OPENERS.contains(c));
            for opener in piece.get(..piece.len() - core.len()).unwrap_or("").chars() {
                words.push(Word::Single(Token::punctuation(&opener.to_string())));
            }
            let body = core.trim_end_matches(|c| CLOSERS.contains(c));
            let closers = core.get(body.len()..).unwrap_or("");
            let subtokens = subtokens::split(body);
            match subtokens.len() {
                0 => {}
                1 => words.extend(subtokens.first().map(|t| Word::Single(Token::new(t)))),
                _ => words.push(Word::Group(group(&subtokens))),
            }
            for closer in closers.chars() {
                words.push(Word::Single(Token::punctuation(&closer.to_string())));
            }
        }
        if let Some(last) = words.last_mut().and_then(Word::last_mut) {
            last.whitespace = true;
        }
    }
    words
}

/// The subtokens of one word; a currency sign before an amount is silent and
/// lends the amount its unit.
fn group(subtokens: &[String]) -> Vec<Token> {
    let mut tokens: Vec<Token> = subtokens
        .iter()
        .enumerate()
        .map(|(i, text)| Token {
            is_head: i == 0,
            ..Token::new(text)
        })
        .collect();
    for i in 0..tokens.len().saturating_sub(1) {
        let currency = tokens
            .get(i)
            .filter(|t| currency_units(&t.text).is_some())
            .map(|t| t.text.clone());
        let amount_follows = tokens
            .get(i + 1)
            .is_some_and(|t| t.text.chars().next().is_some_and(|c| c.is_ascii_digit()));
        if let (Some(currency), true) = (currency, amount_follows) {
            if let Some(sign) = tokens.get_mut(i) {
                sign.phonemes = Some(String::new());
            }
            if let Some(amount) = tokens.get_mut(i + 1) {
                amount.currency = Some(currency);
                amount.is_head = true;
            }
        }
    }
    tokens
}

/// Joins subtokens' readings, with a space where the written word had one.
fn merge(tokens: &[Token]) -> String {
    let mut phonemes = String::new();
    for token in tokens {
        let reading = token.phonemes.as_deref().unwrap_or(UNKNOWN);
        if token.prespace
            && !reading.is_empty()
            && phonemes.chars().last().is_some_and(|c| !c.is_whitespace())
        {
            phonemes.push(' ');
        }
        phonemes.push_str(reading);
    }
    phonemes
}

/// misaki's `resolve_tokens`: silent junk, spaces between parts that were
/// written apart, and only the heavier half of a compound keeping its stress.
fn resolve(tokens: &mut [Token]) {
    let text: String = tokens.iter().map(|t| t.text.as_str()).collect();
    let classes: std::collections::HashSet<u8> = text
        .chars()
        .filter(|c| !JUNK.contains(*c))
        .map(|c| {
            if c.is_alphabetic() {
                0
            } else if c.is_ascii_digit() {
                1
            } else {
                2
            }
        })
        .collect();
    let prespace = text.contains(' ') || text.contains('/') || classes.len() > 1;
    let last = tokens.len().saturating_sub(1);
    for (i, token) in tokens.iter_mut().enumerate() {
        if token.phonemes.is_none() {
            if i == last
                && token.text.chars().all(|c| NON_QUOTE_PUNCTS.contains(c))
                && !token.text.is_empty()
            {
                token.phonemes = Some(token.text.clone());
            } else if is_junk(&token.text) {
                token.phonemes = Some(String::new());
            }
        } else if i > 0 {
            token.prespace = prespace;
        }
    }
    if prespace {
        return;
    }
    let mut stressed: Vec<(bool, usize, usize)> = tokens
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            let ps = t.phonemes.as_deref().filter(|ps| !ps.is_empty())?;
            Some((ps.contains(PRIMARY), stress_weight(ps), i))
        })
        .collect();
    let demote = |token: Option<&mut Token>| {
        if let Some(token) = token {
            if let Some(ps) = token.phonemes.as_mut() {
                *ps = apply_stress(ps, Some(-0.5));
            }
        }
    };
    if let [(_, _, first), (_, _, second)] = stressed.as_slice() {
        if tokens
            .get(*first)
            .is_some_and(|t| t.text.chars().count() == 1)
        {
            demote(tokens.get_mut(*second));
            return;
        }
    }
    let primaries = stressed.iter().filter(|(p, _, _)| *p).count();
    if stressed.len() < 2 || primaries <= stressed.len().div_ceil(2) {
        return;
    }
    stressed.sort();
    let half = stressed.len() / 2;
    for (_, _, i) in stressed.into_iter().take(half) {
        demote(tokens.get_mut(i));
    }
}

pub struct G2p {
    lexicon: Lexicon,
    fallback: Option<Fallback>,
}

impl G2p {
    pub fn load(
        gold: &Path,
        silver: &Path,
        fallback_config: &Path,
        fallback_weights: &Path,
    ) -> Result<Self, String> {
        let read =
            |path: &Path| std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()));
        Ok(Self {
            lexicon: Lexicon::from_json(&read(gold)?, &read(silver)?)
                .map_err(|e| format!("lexicon: {e}"))?,
            fallback: Some(
                Fallback::load(fallback_config, fallback_weights)
                    .map_err(|e| format!("G2P fallback: {e}"))?,
            ),
        })
    }

    fn fallback(&self, text: &str) -> Option<String> {
        let plain: String = text.nfd().filter(char::is_ascii).collect();
        self.fallback
            .as_ref()?
            .read(&plain)
            .ok()
            .filter(|ps| !ps.is_empty())
    }

    /// Kokoro phonemes for `text`, punctuation kept where it shapes prosody.
    pub fn phonemize(&self, text: &str) -> String {
        let text: String = text.nfkc().collect::<String>().replace(['‘', '’'], "'");
        let mut words = words(&text);
        let mut ctx = Context::default();
        for word in words.iter_mut().rev() {
            match word {
                Word::Single(token) => {
                    if token.phonemes.is_none() {
                        token.phonemes = self
                            .lexicon
                            .read(&token.text, ctx, token.is_head, None)
                            .or_else(|| self.fallback(&token.text));
                    }
                    ctx = ctx.before(token.phonemes.as_deref().unwrap_or(""), &token.text);
                }
                Word::Group(tokens) => ctx = self.read_group(tokens, ctx),
            }
        }
        let mut out = String::new();
        for word in &words {
            let (phonemes, whitespace) = match word {
                Word::Single(token) => (
                    token
                        .phonemes
                        .clone()
                        .unwrap_or_else(|| UNKNOWN.to_string()),
                    token.whitespace,
                ),
                Word::Group(tokens) => (merge(tokens), tokens.last().is_some_and(|t| t.whitespace)),
            };
            out.push_str(&phonemes);
            if whitespace {
                out.push(' ');
            }
        }
        out.trim().to_string()
    }

    /// Reads a written word made of several subtokens, longest known span
    /// from the right first, as misaki does; any unreadable part sends the
    /// whole word to the fallback.
    fn read_group(&self, tokens: &mut [Token], mut ctx: Context) -> Context {
        let (mut left, mut right) = (0, tokens.len());
        let mut needs_fallback = false;
        while left < right {
            let span = tokens.get(left..right).unwrap_or(&[]);
            let reading = if span.iter().any(|t| t.phonemes.is_some()) {
                None
            } else {
                let text: String = span.iter().map(|t| t.text.as_str()).collect();
                let currency = span.iter().find_map(|t| t.currency.as_deref());
                let is_head = span.first().is_some_and(|t| t.is_head);
                self.lexicon
                    .read(&text, ctx, is_head, currency)
                    .map(|ps| (ps, text))
            };
            if let Some((ps, text)) = reading {
                ctx = ctx.before(&ps, &text);
                for (i, token) in tokens.iter_mut().enumerate().take(right).skip(left) {
                    token.phonemes = Some(if i == left { ps.clone() } else { String::new() });
                }
                right = left;
                left = 0;
            } else if left + 1 < right {
                left += 1;
            } else {
                right -= 1;
                if let Some(token) = tokens.get_mut(right) {
                    if token.phonemes.is_none() {
                        if is_junk(&token.text) {
                            token.phonemes = Some(String::new());
                        } else {
                            needs_fallback = true;
                            break;
                        }
                    }
                }
                left = 0;
            }
        }
        if needs_fallback {
            let text: String = tokens.iter().map(|t| t.text.as_str()).collect();
            let reading = self.fallback(&text);
            for (i, token) in tokens.iter_mut().enumerate() {
                token.phonemes = if i == 0 {
                    reading.clone()
                } else {
                    Some(String::new())
                };
                token.prespace = false;
            }
            return ctx.before(reading.as_deref().unwrap_or(""), &text);
        }
        resolve(tokens);
        ctx
    }
}
