//! A repair changes only the failed sentences (claim sources design §10).
//!
//! Every sentence of the section under repair that a live claim stands on and
//! that is not a target of this repair is *protected*: verified, confirmed,
//! accepted, queued or simply not sent. [`confine`] compares the repaired
//! section with the section as it was, sentence by sentence, and keeps a
//! change only where it touches a failed sentence: any other change is
//! reverted to the bytes it had, a change that rewrites a failed and a
//! protected sentence together rejects the repair, and every protected
//! sentence must come out byte-identical. The model's edit and the
//! regenerated-section fallback are confined the same way, and so is the
//! teaching review's rewrite after a repair.

use super::super::claim_ledger::ClaimLedger;
use super::super::*;
use lattice_core::domain::learning::claim_review::{ClaimId, ClaimState};
use lattice_core::shared::text::sentences;

/// The repair system prompt's confinement sentence.
pub(super) const CONFINEMENT: &str = "Edit only the failed sentences; every other sentence is returned byte-identical and any change to it is discarded.";

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

    /// The protected set a repair context carries (`"protected"`), so a
    /// resumed repair confines as the first one did.
    pub(in crate::features::learning::lessons::content_verification) fn from_context(
        context: &Value,
    ) -> Vec<Self> {
        context
            .get("protected")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default()
    }
}

/// `repaired`, a section, with every change outside the sentences of
/// `targets` reverted to `previous`'s bytes (§10.2). Both sets name this
/// section's sentences (their `unit` is not read). Errs, as a rejected
/// repair, when one change rewrites a target together with a protected
/// sentence, or a protected sentence does not come out byte-identical.
pub(in crate::features::learning) fn confine(
    previous: &Value,
    repaired: &Value,
    targets: &[Protected],
    protected: &[Protected],
) -> Result<Value> {
    confine_at(previous, repaired, "", targets, protected)
}

fn confine_at(
    previous: &Value,
    repaired: &Value,
    path: &str,
    targets: &[Protected],
    protected: &[Protected],
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
            let in_field = |set: &[Protected]| -> Vec<String> {
                set.iter()
                    .filter(|item| item.field == path)
                    .map(|item| item.quote.clone())
                    .collect()
            };
            confine_text(before, after, &in_field(targets), &in_field(protected)).map(Value::String)
        }
        (Value::Object(before), Value::Object(after)) => {
            let mut out = serde_json::Map::new();
            for (key, old) in before {
                let value = match after.get(key) {
                    Some(new) => confine_at(old, new, &child(key), targets, protected)?,
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
                confine_at(old, new, &child(&index.to_string()), targets, protected)
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
        let lead = &text[..spans.first().map_or(text.len(), |span| span.0)];
        let units = spans
            .iter()
            .enumerate()
            .map(|(index, &(start, end))| {
                let next = spans.get(index + 1).map_or(text.len(), |span| span.0);
                (start, end, &text[start..next])
            })
            .collect();
        Self { lead, units }
    }

    fn sentence(&self, index: usize, text: &'t str) -> &'t str {
        let (start, end, _) = self.units[index];
        &text[start..end]
    }
}

/// The longest common subsequence of two sentence lists, as matched index
/// pairs in order.
fn aligned(a: &[&str], b: &[&str]) -> Vec<(usize, usize)> {
    let mut table = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            table[i][j] = if a[i] == b[j] {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }
    let (mut i, mut j, mut pairs) = (0, 0, Vec::new());
    while i < a.len() && j < b.len() {
        if a[i] == b[j] {
            pairs.push((i, j));
            i += 1;
            j += 1;
        } else if table[i + 1][j] >= table[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    pairs
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

fn confine_text(
    before: &str,
    after: &str,
    targets: &[String],
    protected: &[String],
) -> Result<String> {
    if before == after {
        return Ok(after.to_owned());
    }
    let (old, new) = (Pieces::of(before), Pieces::of(after));
    let old_sentences: Vec<&str> = (0..old.units.len())
        .map(|index| old.sentence(index, before))
        .collect();
    let new_sentences: Vec<&str> = (0..new.units.len())
        .map(|index| new.sentence(index, after))
        .collect();
    let pairs = aligned(&old_sentences, &new_sentences);
    // What comes before the first sentence is whitespace no repair needs.
    let mut out = String::from(old.lead);
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
            let touches = |set: &[String], indices: &[usize]| {
                indices.iter().any(|&k| {
                    let (start, end, _) = old.units[k];
                    overlaps_any(before, (start, end), set)
                })
            };
            // An insertion touches the failed sentence beside it.
            let beside: Vec<usize> = if replaced.is_empty() {
                (i.saturating_sub(1)..(match_i + 1).min(old.units.len())).collect()
            } else {
                Vec::new()
            };
            let failed = touches(targets, &replaced) || touches(targets, &beside);
            if failed && touches(protected, &replaced) {
                return Err(invalid(
                    "The repair rewrote a sentence that already passed together with the failed one; the draft is kept.",
                ));
            }
            if failed {
                for k in written {
                    out.push_str(new.units[k].2);
                }
            } else {
                for k in replaced {
                    out.push_str(old.units[k].2);
                }
            }
        }
        if match_i < old.units.len() {
            out.push_str(new.units[match_j].2);
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
    Ok(out)
}

#[cfg(test)]
#[path = "confine_tests.rs"]
mod tests;
