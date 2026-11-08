//! A repair changes only the failed sentences (claim sources design §10).
//! [`confine`] compares the repaired section sentence by sentence: a change
//! outside the failed sentences is reverted, one that rewrites a failed and a
//! protected sentence together is put back too and the failed sentences it
//! touched are listed with why, so that only their correction is lost, and
//! every protected sentence must come out byte-identical. Every rewrite path is
//! confined so.

use super::super::claim_ledger::ClaimLedger;
use super::super::*;
use lattice_core::domain::learning::claim_review::{ClaimId, ClaimState};
use lattice_core::shared::text::segmentation::common_subsequence;
use lattice_core::shared::text::sentences;
use std::collections::HashSet;

/// The repair system prompt's confinement sentence.
pub(super) const CONFINEMENT: &str = "Edit only each failed claim's `sentence`; every other sentence is returned byte-identical and any change to it is discarded.";

/// A claim's sentence by its section, the field it is in (a path inside
/// the section, such as `body` or `options/1`) and its quotation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::features::learning) struct Protected {
    pub(in crate::features::learning) unit: usize,
    pub(in crate::features::learning) field: String,
    pub(in crate::features::learning) quote: String,
}

impl Protected {
    /// The sentences of every live claim that is not one of `targets`: what
    /// a repair of `targets` must leave as it is (§10.1).
    pub(in crate::features::learning::lessons::content_verification) fn of_ledger(
        ledger: &ClaimLedger,
        targets: &[ClaimId],
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
            .map(|entry| Self::of_location(entry.location()))
            .filter(|protected| !protected.quote.trim().is_empty())
            .collect()
    }

    /// `ids`' sentences, as the ledger locates them.
    pub(in crate::features::learning::lessons::content_verification) fn of_claims(
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
        }
    }
}

/// A failed sentence whose correction shared a change with a sentence that
/// passed its check, which was put back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::features::learning) struct PutBack {
    pub(in crate::features::learning) target: Protected,
    /// What the correction is told when it is asked again.
    pub(in crate::features::learning) reason: String,
}

/// A repaired section confined to its failed sentences.
#[derive(Debug)]
pub(in crate::features::learning) struct Confined {
    pub(in crate::features::learning) section: Value,
    /// The targets whose correction was put back, each once, with the reason
    /// of the first change put back that touched it.
    pub(in crate::features::learning) put_back: Vec<PutBack>,
}

/// `repaired`, a section, with every change outside the sentences of
/// `targets` reverted to `previous`'s bytes (§10.2); a word-for-word repeat
/// of a target's sentence outside every protected one counts as that
/// target. Both sets name this section's sentences (their `unit` is not
/// read). A change that rewrites a target together with a protected sentence
/// is reverted as well, and the targets it touched are listed as put back: only
/// their correction is lost. Errs, as a rejected repair, when a protected
/// sentence does not come out byte-identical.
pub(in crate::features::learning) fn confine(
    previous: &Value,
    repaired: &Value,
    targets: &[Protected],
    protected: &[Protected],
) -> Result<Confined> {
    let mut put_back = Vec::new();
    let section = confine_at(previous, repaired, "", (targets, protected), &mut put_back)?;
    Ok(Confined { section, put_back })
}

fn confine_at(
    previous: &Value,
    repaired: &Value,
    path: &str,
    (targets, protected): (&[Protected], &[Protected]),
    put_back: &mut Vec<PutBack>,
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
            let (text, reverted) = confine_text(before, after, &quotes(&mine), &quotes(&passed))?;
            for back in reverted {
                let mut said: Vec<&str> = Vec::new();
                for item in back.protected.iter().filter_map(|&at| passed.get(at)) {
                    if !said.contains(&item.quote.as_str()) {
                        said.push(&item.quote);
                    }
                }
                let reason = format!(
                    "The correction also changed a sentence that passed its check (\"{}\"), so it was put back; change only this sentence.",
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
                    Some(new) => confine_at(old, new, &child(key), (targets, protected), put_back)?,
                    None if targeted(&child(key)) => continue,
                    None => old.clone(),
                };
                out.insert(key.clone(), value);
            }
            for (key, new) in after {
                if !before.contains_key(key) && targeted(&child(key)) {
                    out.insert(key.clone(), new.clone());
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
                    put_back,
                )
            })
            .collect::<Result<Vec<_>>>()
            .map(Value::Array),
        _ if previous == repaired || targeted(path) => Ok(repaired.clone()),
        _ => Ok(previous.clone()),
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
                .any(|(at, found)| at < end && at + found.len() > start)
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
fn confine_text(
    before: &str,
    after: &str,
    targets: &[String],
    protected: &[String],
) -> Result<(String, Vec<Reverted>)> {
    if before == after {
        return Ok((after.to_owned(), Vec::new()));
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
    // What comes before the first sentence is whitespace no repair needs.
    let mut out = String::from(old.lead);
    let mut reverted = Vec::new();
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
            if on_target && on_protected {
                reverted.push(Reverted {
                    targets: touching(targets, |quote| {
                        reach().any(|k| overlaps(k, std::slice::from_ref(quote)))
                    }),
                    protected: touching(protected, |quote| {
                        replaced
                            .iter()
                            .any(|&k| overlaps(k, std::slice::from_ref(quote)))
                    }),
                });
            }
            let failed = !on_protected && (on_target || reach().any(repeats_failed));
            if failed {
                for k in written {
                    out.push_str(new.piece(k));
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
            "The repair changed a sentence that already passed (\"{lost}\"); the draft is kept."
        )));
    }
    Ok((out, reverted))
}

#[cfg(test)]
#[path = "confine_tests.rs"]
mod tests;
