//! A repair changes only the failed sentences (claim sources design §10).
//! [`confine`] compares the repaired section sentence by sentence: a change
//! outside the failed sentences is reverted, one that rewrites a failed and a
//! protected sentence together is put back too and the failed sentences it
//! touched are listed with why, so that only their correction is lost, and
//! every protected sentence must come out byte-identical. Every rewrite path is
//! confined so.

use super::super::claim_ledger::ClaimLedger;
use super::super::*;
use crate::lessons::repeats;
use lattice_core::domain::learning::claim_review::{ClaimId, ClaimState};
use lattice_core::shared::text::same_text::overlaps;
use lattice_core::shared::text::segmentation::common_subsequence;
use lattice_core::shared::text::sentences;
use std::collections::HashSet;

/// The repair system prompt's confinement sentence.
pub(super) const CONFINEMENT: &str = "Edit only each failed claim's `sentence`; every other sentence is returned byte-identical and any change to it is discarded.";

/// A claim's sentence by its section, the field it is in (a path inside
/// the section, such as `body` or `options/1`) and its quotation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Protected {
    pub(crate) unit: usize,
    pub(crate) field: String,
    pub(crate) quote: String,
    /// The sentence of a machine's claim the teaching review may repair
    /// after a claim repair or a user edit (claim sources design §10.3).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) released: bool,
}

impl Protected {
    /// The sentences of every live claim that is not one of `targets`: what
    /// a repair of `targets` must leave as it is (§10.1).
    pub(in crate::lessons::content_verification) fn of_ledger(
        ledger: &ClaimLedger,
        targets: &[ClaimId],
    ) -> Vec<Self> {
        Self::releasing(ledger, targets, &[])
    }

    /// `of_ledger`, the sentences of the `released` claims marked as ones the
    /// teaching review may repair.
    pub(in crate::lessons::content_verification) fn releasing(
        ledger: &ClaimLedger,
        targets: &[ClaimId],
        released: &[ClaimId],
    ) -> Vec<Self> {
        ledger
            .entries()
            .iter()
            .filter(|entry| !targets.contains(entry.claim_id()))
            .filter(|entry| {
                !matches!(
                    entry.state(),
                    ClaimState::Removed { .. } | ClaimState::Edited { .. }
                )
            })
            .map(|entry| Self {
                released: released.contains(entry.claim_id()),
                ..Self::of_location(entry.location())
            })
            .filter(|protected| !protected.quote.trim().is_empty())
            .collect()
    }

    /// `ids`' sentences, as the ledger locates them.
    pub(in crate::lessons::content_verification) fn of_claims(
        ledger: &ClaimLedger,
        ids: &[ClaimId],
    ) -> Vec<Self> {
        ids.iter()
            .filter_map(|id| ledger.entry(id))
            .map(|entry| Self::of_location(entry.location()))
            .collect()
    }

    fn of_location(location: &lattice_core::domain::learning::claim_review::ClaimLocation) -> Self {
        Self {
            unit: location.unit_index,
            field: location.field.clone(),
            quote: location.quote.clone(),
            released: false,
        }
    }
}

/// A failed sentence whose correction shared a change with a sentence it was
/// not asked to change, which was put back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PutBack {
    pub(crate) target: Protected,
    /// What the correction is told when it is asked again.
    pub(crate) reason: String,
}

/// A change `confine` reverted, with the text it concerns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Undone {
    /// It changed a protected sentence (its quotation).
    Protected(String),
    /// It changed or added text outside the targets.
    Outside(String),
    /// It added a sentence the lesson already has.
    Repeated(String),
}

impl Undone {
    fn reason(&self) -> String {
        match self {
            Self::Protected(quote) => format!(
                "it changes \"{quote}\", which states a checked claim and must stay as it is"
            ),
            Self::Outside(text) => format!(
                "it changes \"{text}\", which is outside what the correction was asked to change"
            ),
            Self::Repeated(text) => format!("it adds \"{text}\", which the lesson already says"),
        }
    }
}

/// A repaired section confined to its failed sentences.
#[derive(Debug)]
pub(crate) struct Confined {
    pub(crate) section: Value,
    /// The targets whose correction was put back, each once, with the reason
    /// of the first change put back that touched it.
    pub(crate) put_back: Vec<PutBack>,
    /// Every change that was reverted.
    pub(crate) undone: Vec<Undone>,
}

impl Confined {
    /// One reason for each distinct change that was reverted.
    pub(crate) fn reasons(&self) -> Vec<String> {
        let mut reasons: Vec<String> = Vec::new();
        for reason in self.undone.iter().map(Undone::reason) {
            if !reasons.contains(&reason) {
                reasons.push(reason);
            }
        }
        reasons
    }
}

/// The text of a value that was reverted.
fn said(value: &Value) -> String {
    value
        .as_str()
        .map_or_else(|| value.to_string(), str::to_owned)
}

/// `repaired`, a section, with every change outside the sentences of
/// `targets` reverted to `previous`'s bytes (§10.2); a word-for-word repeat
/// of a target's sentence outside every protected one counts as that
/// target. Both sets name this section's sentences (their `unit` is not
/// read). A change that rewrites a target together with a protected sentence
/// is reverted as well, and the targets it touched are listed as put back: only
/// their correction is lost. Errs, as a rejected repair, when a protected
/// sentence does not come out byte-identical.
pub(crate) fn confine(
    previous: &Value,
    repaired: &Value,
    targets: &[Protected],
    protected: &[Protected],
) -> Result<Confined> {
    let (mut put_back, mut undone) = (Vec::new(), Vec::new());
    let held = repeats::fields_of(previous);
    let section = confine_at(
        previous,
        repaired,
        "",
        (targets, protected),
        (&held, &mut put_back, &mut undone),
    )?;
    Ok(Confined {
        section,
        put_back,
        undone,
    })
}

/// The sentences the section's fields hold, by field path.
type Held = [(String, Vec<String>)];

fn confine_at(
    previous: &Value,
    repaired: &Value,
    path: &str,
    (targets, protected): (&[Protected], &[Protected]),
    (held, put_back, undone): (&Held, &mut Vec<PutBack>, &mut Vec<Undone>),
) -> Result<Value> {
    let targeted = |path: &str| {
        targets
            .iter()
            .any(|target| target.field == path || target.field.starts_with(&format!("{path}/")))
    };
    let child = |key: &str| {
        if path.is_empty() {
            key.to_owned()
        } else {
            format!("{path}/{key}")
        }
    };
    match (previous, repaired) {
        (Value::String(before), Value::String(after)) => {
            let in_field = |set: &[Protected]| -> Vec<Protected> {
                set.iter()
                    .filter(|item| item.field == path)
                    .cloned()
                    .collect()
            };
            let quotes = |set: &[Protected]| -> Vec<String> {
                set.iter().map(|item| item.quote.clone()).collect()
            };
            let (mine, passed) = (in_field(targets), in_field(protected));
            let elsewhere: Option<HashSet<&str>> = repeats::teaches_path(path).then(|| {
                held.iter()
                    .filter(|(field, _)| field != path)
                    .flat_map(|(_, keys)| keys.iter().map(String::as_str))
                    .collect()
            });
            let (text, reverted, gone) = confine_text(
                before,
                after,
                (&quotes(&mine), &quotes(&passed)),
                elsewhere.as_ref(),
            )?;
            undone.extend(gone);
            for back in reverted {
                let mut said: Vec<&str> = Vec::new();
                for item in back.protected.iter().filter_map(|&at| passed.get(at)) {
                    if !said.contains(&item.quote.as_str()) {
                        said.push(&item.quote);
                    }
                }
                let reason = format!(
                    "The correction also changed a sentence it was not asked to change (\"{}\"), so it was put back; change only this sentence.",
                    said.join("\", \"")
                );
                for target in back.targets.iter().filter_map(|&at| mine.get(at)) {
                    if !put_back.iter().any(|done| done.target == *target) {
                        put_back.push(PutBack {
                            target: target.clone(),
                            reason: reason.clone(),
                        });
                    }
                }
            }
            Ok(Value::String(text))
        }
        (Value::Object(before), Value::Object(after)) => {
            let mut out = serde_json::Map::new();
            for (key, old) in before {
                let value = match after.get(key) {
                    Some(new) => confine_at(
                        old,
                        new,
                        &child(key),
                        (targets, protected),
                        (held, &mut *put_back, &mut *undone),
                    )?,
                    None if targeted(&child(key)) => continue,
                    None => {
                        undone.push(Undone::Outside(said(old)));
                        old.clone()
                    }
                };
                out.insert(key.clone(), value);
            }
            for (key, new) in after {
                if before.contains_key(key) {
                    continue;
                }
                if targeted(&child(key)) {
                    out.insert(key.clone(), new.clone());
                } else {
                    undone.push(Undone::Outside(said(new)));
                }
            }
            Ok(Value::Object(out))
        }
        (Value::Array(before), Value::Array(after)) if before.len() == after.len() => before
            .iter()
            .zip(after)
            .enumerate()
            .map(|(index, (old, new))| {
                confine_at(
                    old,
                    new,
                    &child(&index.to_string()),
                    (targets, protected),
                    (held, &mut *put_back, &mut *undone),
                )
            })
            .collect::<Result<Vec<_>>>()
            .map(Value::Array),
        _ if previous == repaired || targeted(path) => Ok(repaired.clone()),
        _ => {
            undone.push(Undone::Outside(said(previous)));
            Ok(previous.clone())
        }
    }
}

/// One text's sentences with the text after each up to the next (its
/// separator), and what comes before the first.
struct Pieces<'t> {
    lead: &'t str,
    units: Vec<(usize, usize, &'t str)>,
}

impl<'t> Pieces<'t> {
    fn of(text: &'t str) -> Self {
        let spans = sentences(text);
        let lead = text
            .get(..spans.first().map_or(text.len(), |span| span.0))
            .unwrap_or_default();
        let units = spans
            .iter()
            .enumerate()
            .map(|(index, &(start, end))| {
                let next = spans.get(index + 1).map_or(text.len(), |span| span.0);
                (start, end, text.get(start..next).unwrap_or_default())
            })
            .collect();
        Self { lead, units }
    }

    /// The sentences themselves, without their separators.
    fn sentences(&self, text: &'t str) -> Vec<&'t str> {
        self.units
            .iter()
            .map(|&(start, end, _)| text.get(start..end).unwrap_or_default())
            .collect()
    }

    /// Sentence `index` with the separator after it.
    fn piece(&self, index: usize) -> &'t str {
        self.units.get(index).map_or("", |unit| unit.2)
    }
}

/// Whether the byte range `(start, end)` of `text` overlaps any occurrence
/// of one of `quotes`.
fn overlaps_any(text: &str, (start, end): (usize, usize), quotes: &[String]) -> bool {
    quotes
        .iter()
        .filter(|quote| !quote.is_empty())
        .any(|quote| {
            text.match_indices(quote.as_str())
                .any(|(at, found)| overlaps((at, at + found.len()), (start, end)))
        })
}

/// A change put back: the targets and the passed sentences it touched, by
/// their place in the sets `confine_text` was given.
struct Reverted {
    targets: Vec<usize>,
    protected: Vec<usize>,
}

/// The places of the `quotes` that `touched` holds.
fn touching(quotes: &[String], touched: impl Fn(&String) -> bool) -> Vec<usize> {
    quotes
        .iter()
        .enumerate()
        .filter(|(_, quote)| touched(quote))
        .map(|(at, _)| at)
        .collect()
}

/// `after` confined to the sentences of `targets` (see [`confine`]), and the
/// changes it put back because they also rewrote a sentence of `protected`.
/// A new sentence that `before` or the text `elsewhere` already has is not
/// kept; with no `elsewhere`, the repeat rule does not read this text and
/// none is dropped.
fn confine_text(
    before: &str,
    after: &str,
    (targets, protected): (&[String], &[String]),
    elsewhere: Option<&HashSet<&str>>,
) -> Result<(String, Vec<Reverted>, Vec<Undone>)> {
    if before == after {
        return Ok((after.to_owned(), Vec::new(), Vec::new()));
    }
    let (old, new) = (Pieces::of(before), Pieces::of(after));
    let old_sentences = old.sentences(before);
    let new_sentences = new.sentences(after);
    let pairs = common_subsequence(&old_sentences, &new_sentences);
    let overlaps = |k: usize, set: &[String]| {
        old.units
            .get(k)
            .is_some_and(|&(start, end, _)| overlaps_any(before, (start, end), set))
    };
    // A failed sentence the field repeats word for word outside the quote
    // its claim was located by is the same failed sentence: the repair may
    // change those repeats too, unless one lies in a protected sentence.
    let failed_texts: HashSet<&str> = old_sentences
        .iter()
        .enumerate()
        .filter(|&(k, _)| overlaps(k, targets))
        .map(|(_, sentence)| *sentence)
        .collect();
    let repeats_failed = |k: usize| {
        old_sentences
            .get(k)
            .is_some_and(|sentence| failed_texts.contains(sentence))
    };
    let own: HashSet<String> = repeats::keys(before).into_iter().collect();
    let mut dropped = false;
    // What comes before the first sentence is whitespace no repair needs.
    let mut out = String::from(old.lead);
    let mut reverted = Vec::new();
    let mut undone = Vec::new();
    let (mut i, mut j) = (0, 0);
    for (match_i, match_j) in pairs
        .iter()
        .copied()
        .chain(std::iter::once((old.units.len(), new.units.len())))
    {
        // The hunk: old sentences i..match_i replaced by new j..match_j. As
        // many sentences replaced as written are read one for one, so a
        // rewording beside the failed sentence is a hunk of its own.
        let (old_count, new_count) = (match_i - i, match_j - j);
        let hunks: Vec<(Vec<usize>, std::ops::Range<usize>)> = if old_count == new_count {
            (0..old_count)
                .map(|k| (vec![i + k], j + k..j + k + 1))
                .collect()
        } else {
            vec![((i..match_i).collect(), j..match_j)]
        };
        for (replaced, written) in hunks {
            // An insertion touches the failed sentence beside it.
            let beside: Vec<usize> = if replaced.is_empty() {
                (i.saturating_sub(1)..(match_i + 1).min(old.units.len())).collect()
            } else {
                Vec::new()
            };
            let reach = || replaced.iter().chain(&beside).copied();
            let on_target = reach().any(|k| overlaps(k, targets));
            let on_protected = replaced.iter().any(|&k| overlaps(k, protected));
            // A change that rewrites a target together with a sentence that
            // passed is put back, and only the targets it touched lose their
            // correction.
            let touched_protected = touching(protected, |quote| {
                replaced
                    .iter()
                    .any(|&k| overlaps(k, std::slice::from_ref(quote)))
            });
            if on_target && on_protected {
                reverted.push(Reverted {
                    targets: touching(targets, |quote| {
                        reach().any(|k| overlaps(k, std::slice::from_ref(quote)))
                    }),
                    protected: touched_protected.clone(),
                });
            }
            if on_protected {
                undone.extend(
                    touched_protected
                        .iter()
                        .filter_map(|&at| protected.get(at))
                        .map(|quote| Undone::Protected(quote.clone())),
                );
            }
            let failed = !on_protected && (on_target || reach().any(repeats_failed));
            if !failed && !on_protected {
                let texts: Vec<&str> = if replaced.is_empty() {
                    written
                        .clone()
                        .filter_map(|k| new_sentences.get(k).copied())
                        .collect()
                } else {
                    replaced
                        .iter()
                        .filter_map(|&k| old_sentences.get(k).copied())
                        .collect()
                };
                undone.extend(
                    texts
                        .into_iter()
                        .map(|text| Undone::Outside(text.to_owned())),
                );
            }
            if failed {
                let replaced_keys: HashSet<String> = replaced
                    .iter()
                    .filter_map(|&k| old_sentences.get(k))
                    .flat_map(|sentence| repeats::keys(sentence))
                    .collect();
                for k in written {
                    let repeat = elsewhere.is_some_and(|elsewhere| {
                        new_sentences.get(k).is_some_and(|sentence| {
                            repeats::keys(sentence).iter().any(|key| {
                                elsewhere.contains(key.as_str())
                                    || (own.contains(key) && !replaced_keys.contains(key))
                            })
                        })
                    });
                    if repeat {
                        dropped = true;
                        undone.extend(
                            new_sentences
                                .get(k)
                                .map(|sentence| Undone::Repeated((*sentence).to_owned())),
                        );
                    } else {
                        out.push_str(new.piece(k));
                    }
                }
            } else {
                for k in replaced {
                    out.push_str(old.piece(k));
                }
            }
        }
        if match_i < old.units.len() {
            out.push_str(new.piece(match_j));
        }
        (i, j) = (match_i + 1, match_j + 1);
    }
    if let Some(lost) = protected
        .iter()
        .find(|quote| before.contains(quote.as_str()) && !out.contains(quote.as_str()))
    {
        return Err(invalid(format!(
            "The repair changed a sentence it was not asked to change (\"{lost}\"); change only the failed sentences."
        )));
    }
    if dropped && !before.ends_with(char::is_whitespace) {
        out.truncate(out.trim_end().len());
    }
    Ok((out, reverted, undone))
}

#[cfg(test)]
#[path = "confine_tests.rs"]
mod tests;
