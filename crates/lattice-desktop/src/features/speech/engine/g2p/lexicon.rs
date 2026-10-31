//! misaki's British English lexicon: the gold and silver dictionaries, the
//! suffix rules that stretch them to inflected forms, the special cases for
//! function words, and numbers. Ported from misaki's `en.py` (`Lexicon`).
//! There is no part-of-speech tagger here, so a word with several readings
//! ("read", "lead") takes its default one.

use std::collections::HashMap;

use serde_json::Value;

use super::numbers;

pub(super) const PRIMARY: char = 'ˈ';
pub(super) const SECONDARY: char = 'ˌ';
const VOWELS: &str = "AIOQWYaiuæɑɒɔəɛɜɪʊʌᵻ";
const CONSONANTS: &str = "bdfhjklmnpstvwzðŋɡɹɾʃʒʤʧθ";
pub(super) const PUNCTS: &str = ";:,.!?—…\"“”";
pub(super) const NON_QUOTE_PUNCTS: &str = ";:,.!?—…";
const DIPHTHONGS: &str = "AIOQWYʤʧ";
const ORDINALS: [&str; 4] = ["st", "nd", "rd", "th"];

fn symbol_word(word: &str) -> Option<&'static str> {
    match word {
        "%" => Some("percent"),
        "&" => Some("and"),
        "+" => Some("plus"),
        "@" => Some("at"),
        _ => None,
    }
}

pub(super) fn currency_units(symbol: &str) -> Option<(&'static str, &'static str)> {
    match symbol {
        "$" => Some(("dollar", "cent")),
        "£" => Some(("pound", "pence")),
        "€" => Some(("euro", "cent")),
        _ => None,
    }
}

fn is_vowel(c: char) -> bool {
    VOWELS.contains(c)
}

/// How heavy a reading is when deciding which part of a compound keeps its
/// stress: diphthongs count twice.
pub(super) fn stress_weight(phonemes: &str) -> usize {
    phonemes
        .chars()
        .map(|c| if DIPHTHONGS.contains(c) { 2 } else { 1 })
        .sum()
}

/// What follows the word being read, which some readings depend on.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Context {
    /// Whether the next word starts with a vowel sound; `None` before
    /// punctuation or the end.
    pub(super) future_vowel: Option<bool>,
    /// Whether the next word is "to".
    pub(super) future_to: bool,
}

impl Context {
    /// The context for the word before one read as `phonemes`.
    pub(super) fn before(self, phonemes: &str, text: &str) -> Self {
        let future_vowel = if phonemes.is_empty() {
            self.future_vowel
        } else {
            phonemes
                .chars()
                .find(|&c| is_vowel(c) || CONSONANTS.contains(c) || NON_QUOTE_PUNCTS.contains(c))
                .map_or(self.future_vowel, |c| {
                    if NON_QUOTE_PUNCTS.contains(c) {
                        None
                    } else {
                        Some(is_vowel(c))
                    }
                })
        };
        Self {
            future_vowel,
            future_to: matches!(text, "to" | "To" | "TO"),
        }
    }
}

#[derive(Clone)]
enum Entry {
    Plain(String),
    /// Readings by part of speech, always with a `DEFAULT`.
    Tagged(HashMap<String, Option<String>>),
}

impl Entry {
    fn reading(&self, context: Option<Context>) -> Option<&str> {
        match self {
            Entry::Plain(reading) => Some(reading),
            Entry::Tagged(readings) => {
                let at_end = context.is_some_and(|c| c.future_vowel.is_none());
                let tag = if at_end && readings.contains_key("None") {
                    "None"
                } else {
                    "DEFAULT"
                };
                readings.get(tag).and_then(|r| r.as_deref())
            }
        }
    }
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first
            .to_uppercase()
            .chain(chars.flat_map(char::to_lowercase))
            .collect(),
        None => String::new(),
    }
}

fn is_upper(word: &str) -> bool {
    word == word.to_uppercase()
}

fn is_lower(word: &str) -> bool {
    word == word.to_lowercase()
}

/// Every character after the first is lower case.
fn tail_is_lower(word: &str) -> bool {
    word.chars().skip(1).all(|c| !c.is_uppercase())
}

fn tail_is_upper(word: &str) -> bool {
    word.chars().skip(1).all(|c| !c.is_lowercase())
}

fn is_alpha(word: &str) -> bool {
    !word.is_empty() && word.chars().all(char::is_alphabetic)
}

fn is_digits(word: &str) -> bool {
    !word.is_empty() && word.chars().all(|c| c.is_ascii_digit())
}

/// `ps`, with its stress shifted as misaki's `apply_stress` does: below -1
/// removes it, -1 or a non-positive stress demotes it, a positive stress adds
/// or promotes it.
pub(super) fn apply_stress(ps: &str, stress: Option<f32>) -> String {
    let has = |c: char| ps.contains(c);
    let Some(stress) = stress else {
        return ps.to_string();
    };
    if stress < -1.0 {
        return ps.replace([PRIMARY, SECONDARY], "");
    }
    if stress == -1.0 || ((stress == 0.0 || stress == -0.5) && has(PRIMARY)) {
        return ps
            .replace(SECONDARY, "")
            .replace(PRIMARY, &SECONDARY.to_string());
    }
    let unstressed = !has(PRIMARY) && !has(SECONDARY);
    if (stress == 0.0 || stress == 0.5 || stress == 1.0) && unstressed {
        if !ps.chars().any(is_vowel) {
            return ps.to_string();
        }
        return restress(&format!("{SECONDARY}{ps}"));
    }
    if stress >= 1.0 && !has(PRIMARY) && has(SECONDARY) {
        return ps.replace(SECONDARY, &PRIMARY.to_string());
    }
    if stress > 1.0 && unstressed {
        if !ps.chars().any(is_vowel) {
            return ps.to_string();
        }
        return restress(&format!("{PRIMARY}{ps}"));
    }
    ps.to_string()
}

/// Moves each stress mark to just before the vowel it stresses.
fn restress(ps: &str) -> String {
    let chars: Vec<char> = ps.chars().collect();
    let mut keyed: Vec<(f32, char)> = chars
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            let key = if c == PRIMARY || c == SECONDARY {
                chars
                    .iter()
                    .enumerate()
                    .skip(i)
                    .find(|(_, &v)| is_vowel(v))
                    .map_or(i as f32, |(j, _)| j as f32 - 0.5)
            } else {
                i as f32
            };
            (key, c)
        })
        .collect();
    keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
    keyed.into_iter().map(|(_, c)| c).collect()
}

fn add_s(stem: &str) -> Option<String> {
    let last = stem.chars().last()?;
    Some(if "ptkfθ".contains(last) {
        format!("{stem}s")
    } else if "szʃʒʧʤ".contains(last) {
        format!("{stem}ɪz")
    } else {
        format!("{stem}z")
    })
}

fn add_ed(stem: &str) -> Option<String> {
    let last = stem.chars().last()?;
    Some(if "pkfθʃsʧ".contains(last) {
        format!("{stem}t")
    } else if last == 'd' || last == 't' {
        format!("{stem}ɪd")
    } else {
        format!("{stem}d")
    })
}

fn add_ing(stem: &str) -> Option<String> {
    let last = stem.chars().last()?;
    if last == 'ə' || last == 'ː' {
        return None;
    }
    Some(format!("{stem}ɪŋ"))
}

pub(super) struct Lexicon {
    golds: HashMap<String, Entry>,
    silvers: HashMap<String, Entry>,
}

fn parse(json: &[u8]) -> Result<HashMap<String, Entry>, serde_json::Error> {
    let raw: HashMap<String, Value> = serde_json::from_slice(json)?;
    let entries = raw.into_iter().filter_map(|(word, value)| {
        let entry = match value {
            Value::String(reading) => Entry::Plain(reading),
            Value::Object(tags) => Entry::Tagged(
                tags.into_iter()
                    .map(|(tag, reading)| (tag, reading.as_str().map(str::to_string)))
                    .collect(),
            ),
            _ => return None,
        };
        Some((word, entry))
    });
    // Each entry also answers for its capitalised or lower-case twin.
    let mut grown = HashMap::new();
    let mut original = HashMap::new();
    for (word, entry) in entries {
        if word.chars().count() >= 2 {
            let lower = word.to_lowercase();
            if word == lower {
                let capital = capitalize(&word);
                if capital != word {
                    grown.insert(capital, entry.clone());
                }
            } else if word == capitalize(&lower) {
                grown.insert(lower, entry.clone());
            }
        }
        original.insert(word, entry);
    }
    grown.extend(original);
    Ok(grown)
}

impl Lexicon {
    pub(super) fn from_json(gold: &[u8], silver: &[u8]) -> Result<Self, serde_json::Error> {
        Ok(Self {
            golds: parse(gold)?,
            silvers: parse(silver)?,
        })
    }

    fn gold(&self, word: &str) -> Option<&str> {
        self.golds.get(word).and_then(|e| e.reading(None))
    }

    /// An initialism, letter by letter, stressed on the last letter.
    fn spell(&self, word: &str) -> Option<String> {
        let letters = word
            .chars()
            .filter(|c| c.is_alphabetic())
            .map(|c| self.gold(&c.to_uppercase().to_string()))
            .collect::<Option<Vec<_>>>()?;
        let joined = apply_stress(&letters.concat(), Some(0.0));
        Some(match joined.rfind(SECONDARY) {
            Some(at) => format!(
                "{}{PRIMARY}{}",
                joined.get(..at)?,
                joined.get(at + SECONDARY.len_utf8()..)?
            ),
            None => joined,
        })
    }

    fn special_case(&self, word: &str, stress: Option<f32>, ctx: Context) -> Option<String> {
        if let Some(name) = symbol_word(word) {
            return self.lookup(name, None, Some(ctx));
        }
        let dotted = word.trim_matches('.');
        if dotted.contains('.')
            && is_alpha(&word.replace('.', ""))
            && word
                .split('.')
                .map(|part| part.chars().count())
                .max()
                .unwrap_or(0)
                < 3
        {
            return self.spell(word);
        }
        let reading = match word {
            "a" => "ɐ",
            "A" if ctx.future_vowel.is_some() => "ɐ",
            "A" => "ˈA",
            "am" | "Am" => {
                if ctx.future_vowel.is_none() || word != "am" || stress.is_some_and(|s| s > 0.0) {
                    return self.gold("am").map(str::to_string);
                }
                "ɐm"
            }
            "AM" => return self.spell(word),
            "an" | "An" | "AN" => "ɐn",
            "I" => "ˌI",
            "to" | "To" | "TO" => match ctx.future_vowel {
                None => return self.gold("to").map(str::to_string),
                Some(false) => "tə",
                Some(true) => "tʊ",
            },
            "in" | "In" | "IN" if ctx.future_vowel.is_none() => "ˈɪn",
            "in" | "In" | "IN" => "ɪn",
            "the" | "The" | "THE" if ctx.future_vowel == Some(true) => "ði",
            "the" | "The" | "THE" => "ðə",
            "used" | "Used" | "USED" => {
                let tag = if ctx.future_to { "VBD" } else { "DEFAULT" };
                return match self.golds.get("used") {
                    Some(Entry::Tagged(readings)) => readings.get(tag).cloned().flatten(),
                    _ => None,
                };
            }
            _ if word.eq_ignore_ascii_case("vs") || word.eq_ignore_ascii_case("vs.") => {
                return self.lookup("versus", None, Some(ctx));
            }
            _ => return None,
        };
        Some(reading.to_string())
    }

    fn is_known(&self, word: &str) -> bool {
        if self.golds.contains_key(word)
            || symbol_word(word).is_some()
            || self.silvers.contains_key(word)
        {
            return true;
        }
        if !is_alpha(word) || !word.chars().all(|c| c.is_ascii_alphabetic()) {
            return false;
        }
        if word.chars().count() == 1 {
            return true;
        }
        if is_upper(word) && self.golds.contains_key(&word.to_lowercase()) {
            return true;
        }
        tail_is_upper(word)
    }

    pub(super) fn lookup(
        &self,
        word: &str,
        stress: Option<f32>,
        ctx: Option<Context>,
    ) -> Option<String> {
        let lowered;
        let word = if is_upper(word) && !self.golds.contains_key(word) {
            lowered = word.to_lowercase();
            lowered.as_str()
        } else {
            word
        };
        let reading = self
            .golds
            .get(word)
            .or_else(|| self.silvers.get(word))
            .and_then(|entry| entry.reading(ctx));
        match reading {
            Some(ps) => Some(apply_stress(ps, stress)),
            None => self.spell(word),
        }
    }

    fn stem_s(&self, word: &str, stress: Option<f32>, ctx: Context) -> Option<String> {
        let len = word.chars().count();
        let base = word.strip_suffix('s')?;
        if len < 3 {
            return None;
        }
        let stem = if !word.ends_with("ss") && self.is_known(base) {
            base.to_string()
        } else if let Some(stem) = word
            .strip_suffix("'s")
            .or_else(|| {
                (len > 4 && !word.ends_with("ies"))
                    .then(|| word.strip_suffix("es"))
                    .flatten()
            })
            .filter(|stem| self.is_known(stem))
        {
            stem.to_string()
        } else {
            (len > 4)
                .then(|| word.strip_suffix("ies"))
                .flatten()
                .map(|stem| format!("{stem}y"))
                .filter(|stem| self.is_known(stem))?
        };
        add_s(&self.lookup(&stem, stress, Some(ctx))?)
    }

    fn stem_ed(&self, word: &str, stress: Option<f32>, ctx: Context) -> Option<String> {
        let len = word.chars().count();
        let base = word.strip_suffix('d')?;
        if len < 4 {
            return None;
        }
        let stem = if !word.ends_with("dd") && self.is_known(base) {
            base
        } else {
            (len > 4 && !word.ends_with("eed"))
                .then(|| word.strip_suffix("ed"))
                .flatten()
                .filter(|stem| self.is_known(stem))?
        };
        add_ed(&self.lookup(stem, stress, Some(ctx))?)
    }

    fn stem_ing(&self, word: &str, stress: Option<f32>, ctx: Context) -> Option<String> {
        let len = word.chars().count();
        let base = word.strip_suffix("ing")?;
        if len < 5 {
            return None;
        }
        let doubled = {
            let mut tail = base.chars().rev();
            match (tail.next(), tail.next()) {
                (Some(a), Some(b)) => {
                    (a == b && "bcdgklmnprstvxz".contains(a)) || (a == 'k' && b == 'c')
                }
                _ => false,
            }
        };
        let stem = if len > 5 && self.is_known(base) {
            base.to_string()
        } else if self.is_known(&format!("{base}e")) {
            format!("{base}e")
        } else if len > 5 && doubled {
            let mut stem = base.to_string();
            stem.pop();
            if !self.is_known(&stem) {
                return None;
            }
            stem
        } else {
            return None;
        };
        add_ing(&self.lookup(&stem, stress, Some(ctx))?)
    }

    fn word(&self, word: &str, stress: Option<f32>, ctx: Context) -> Option<String> {
        if let Some(ps) = self.special_case(word, stress, ctx) {
            return Some(ps);
        }
        let lower = word.to_lowercase();
        let word = if word.chars().count() > 1
            && is_alpha(&word.replace('\'', ""))
            && word != lower
            && !self.golds.contains_key(word)
            && !self.silvers.contains_key(word)
            && (is_upper(word) || tail_is_lower(word))
            && (self.golds.contains_key(&lower)
                || self.silvers.contains_key(&lower)
                || self.stem_s(&lower, stress, ctx).is_some()
                || self.stem_ed(&lower, stress, ctx).is_some()
                || self.stem_ing(&lower, stress, ctx).is_some())
        {
            lower.as_str()
        } else {
            word
        };
        if self.is_known(word) {
            return self.lookup(word, stress, Some(ctx));
        }
        if let Some(stem) = word.strip_suffix("s'") {
            let possessive = format!("{stem}'s");
            if self.is_known(&possessive) {
                return self.lookup(&possessive, stress, Some(ctx));
            }
        }
        if let Some(stem) = word.strip_suffix('\'').filter(|stem| self.is_known(stem)) {
            return self.lookup(stem, stress, Some(ctx));
        }
        self.stem_s(word, stress, ctx)
            .or_else(|| self.stem_ed(word, stress, ctx))
            .or_else(|| self.stem_ing(word, Some(stress.unwrap_or(0.5)), ctx))
    }

    /// One word or number, as misaki's `Lexicon.__call__` reads it; `None`
    /// when the lexicon does not know it.
    pub(super) fn read(
        &self,
        word: &str,
        ctx: Context,
        is_head: bool,
        currency: Option<&str>,
    ) -> Option<String> {
        let stress = if is_lower(word) {
            None
        } else if is_upper(word) {
            Some(2.0)
        } else {
            Some(0.5)
        };
        if let Some(ps) = self.word(word, stress, ctx) {
            return Some(ps);
        }
        if is_number(word, is_head) {
            return self.number(word, currency, is_head);
        }
        None
    }

    /// Number words read through the lexicon, "and" left out as misaki
    /// leaves it out.
    fn say<S: AsRef<str>>(&self, spoken: &[S]) -> Vec<String> {
        spoken
            .iter()
            .map(AsRef::as_ref)
            .filter(|w| *w != "and")
            .filter_map(|w| self.lookup(w, (w == "point").then_some(-2.0), None))
            .collect()
    }

    fn say_digits(&self, digits: &str) -> Vec<String> {
        digits
            .chars()
            .filter_map(|c| c.to_digit(10))
            .flat_map(|d| self.say(&numbers::cardinal(u64::from(d))))
            .collect()
    }

    fn number(&self, word: &str, currency: Option<&str>, is_head: bool) -> Option<String> {
        let suffix_at = word
            .char_indices()
            .rev()
            .take_while(|(_, c)| c.is_ascii_lowercase() || *c == '\'')
            .last()
            .map_or(word.len(), |(i, _)| i);
        let (number, suffix) = word.split_at_checked(suffix_at)?;
        let mut words: Vec<String> = Vec::new();
        let number = match number.strip_prefix('-') {
            Some(rest) => {
                words.push(self.lookup("minus", None, None)?);
                rest
            }
            None => number,
        };
        let plain = number.replace(',', "");
        let units = currency.and_then(currency_units);
        if is_digits(number) && ORDINALS.contains(&suffix) {
            words.extend(self.say(&numbers::ordinal(number.parse().ok()?)));
        } else if words.is_empty() && number.len() == 4 && units.is_none() && is_digits(number) {
            words.extend(self.say(&numbers::year(number.parse().ok()?)));
        } else if !is_head && !number.contains('.') {
            let digits: Vec<char> = plain.chars().collect();
            match digits.as_slice() {
                ['0', ..] => words.extend(self.say_digits(&plain)),
                d if d.len() > 3 => words.extend(self.say_digits(&plain)),
                [first, '0', last] if *last != '0' => {
                    words.extend(self.say_digits(&first.to_string()));
                    words.extend(self.lookup("O", Some(-2.0), None));
                    words.extend(self.say_digits(&last.to_string()));
                }
                [first, rest @ ..] if digits.len() == 3 && !plain.ends_with("00") => {
                    words.extend(self.say_digits(&first.to_string()));
                    let rest: String = rest.iter().collect();
                    words.extend(self.say(&numbers::cardinal(rest.parse().ok()?)));
                }
                _ => words.extend(self.say(&numbers::cardinal(plain.parse().ok()?))),
            }
        } else if number.matches('.').count() > 1 || !is_head {
            for part in plain.split('.').filter(|p| !p.is_empty()) {
                let tail_nonzero = part.chars().skip(1).any(|c| c != '0');
                if part.starts_with('0') || (part.chars().count() != 2 && tail_nonzero) {
                    words.extend(self.say_digits(part));
                } else {
                    words.extend(self.say(&numbers::cardinal(part.parse().ok()?)));
                }
            }
        } else if let Some((unit, subunit)) =
            units.filter(|_| plain.split('.').nth(1).is_none_or(|cents| cents.len() < 3))
        {
            let mut parts = plain.split('.');
            let whole: u64 = match parts.next().filter(|p| !p.is_empty()) {
                Some(p) => p.parse().ok()?,
                None => 0,
            };
            let mut pairs = vec![(whole, unit)];
            if let Some(cents) = parts.next() {
                pairs.push((cents.parse().unwrap_or(0), subunit));
                if pairs.get(1).is_some_and(|(n, _)| *n == 0) {
                    pairs.truncate(1);
                } else if whole == 0 {
                    pairs.remove(0);
                }
            }
            for (i, (amount, unit)) in pairs.into_iter().enumerate() {
                if i > 0 {
                    words.extend(self.lookup("and", None, None));
                }
                words.extend(self.say(&numbers::cardinal(amount)));
                words.extend(if amount != 1 && unit != "pence" {
                    self.stem_s(&format!("{unit}s"), None, Context::default())
                } else {
                    self.lookup(unit, None, None)
                });
            }
        } else if !number.contains('.') {
            let value: u64 = plain.parse().ok()?;
            if ORDINALS.contains(&suffix) {
                words.extend(self.say(&numbers::ordinal(value)));
            } else {
                words.extend(self.say(&numbers::cardinal(value)));
            }
        } else {
            let (integer, fraction) = plain.split_once('.')?;
            if integer.is_empty() {
                words.extend(self.say(&["point"]));
                words.extend(self.say_digits(fraction));
            } else {
                words.extend(self.say(&numbers::decimal(integer, fraction)));
            }
        }
        if words.is_empty() {
            return None;
        }
        let spoken = words.join(" ");
        match suffix {
            "s" | "'s" => add_s(&spoken),
            "ed" | "'d" => add_ed(&spoken),
            "ing" => add_ing(&spoken),
            _ => Some(spoken),
        }
    }
}

/// Digits, with an optional inflection, sign, separators or decimal point.
pub(super) fn is_number(word: &str, is_head: bool) -> bool {
    if !word.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    let base = ["ing", "'d", "ed", "'s", "st", "nd", "rd", "th", "s"]
        .iter()
        .find_map(|suffix| word.strip_suffix(suffix))
        .unwrap_or(word);
    base.chars()
        .enumerate()
        .all(|(i, c)| c.is_ascii_digit() || c == ',' || c == '.' || (is_head && i == 0 && c == '-'))
}

#[cfg(test)]
#[path = "lexicon_tests.rs"]
pub(super) mod tests;
