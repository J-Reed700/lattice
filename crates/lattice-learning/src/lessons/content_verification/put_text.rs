//! A sentence written into or out of a field's text, the way a put back and
//! its undo do it.

use lattice_core::shared::text::same_text::holds_sentence;
use lattice_core::domain::learning::claim_review::{span_of, Anchor, ClaimLedgerEntry, CutPlace};

/// `text` with `sentence` written in at `anchor`: one space between sentences
/// of a paragraph, paragraph breaks kept. `None` when the anchor's sentence is
/// not in `text` exactly once, or when `text` has the sentence already.
pub(crate) fn insert_sentence(text: &str, anchor: &Anchor, sentence: &str) -> Option<String> {
    if holds_sentence(text, sentence) {
        return None;
    }
    let mut out = text.to_owned();
    match anchor {
        Anchor::After(other) => {
            let (_, end) = span_of(text, other)?;
            out.insert_str(end, &format!(" {sentence}"));
        }
        Anchor::Before(other) => {
            let (start, _) = span_of(text, other)?;
            out.insert_str(start, &format!("{sentence} "));
        }
        Anchor::End if text.trim().is_empty() => out = sentence.to_owned(),
        Anchor::End => out.insert_str(text.trim_end().len(), &format!(" {sentence}")),
    }
    Some(out)
}

/// `text` without the one occurrence of `sentence` and a space beside it;
/// `None` when it is not in `text` exactly once.
pub(crate) fn remove_sentence(text: &str, sentence: &str) -> Option<String> {
    let own = span_of(text, sentence)?;
    let (start, end) = [format!(" {sentence}"), format!("{sentence} ")]
        .iter()
        .find_map(|pattern| span_of(text, pattern))
        .unwrap_or(own);
    let mut out = text.to_owned();
    out.replace_range(start..end, "");
    Some(out)
}

/// Where the placing of a sentence a machine cut ended: a place, no place the
/// records name, or places the records and the text cannot tell apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Placing {
    At(Anchor),
    Unknown,
    Ambiguous,
}

/// Every text reachable from `sentence` along `edges`, forwards or backwards,
/// not counting `sentence`.
fn reach<'a>(sentence: &'a str, edges: &[(&'a str, &'a str)], forward: bool) -> Vec<&'a str> {
    let mut seen = vec![sentence];
    let mut next = 0;
    while let Some(&at) = seen.get(next) {
        next += 1;
        for &(from, to) in edges {
            let (here, there) = if forward { (from, to) } else { (to, from) };
            if here == at && !seen.contains(&there) {
                seen.push(there);
            }
        }
    }
    seen.split_off(1)
}

/// Where a cut sentence goes back in `text`, from the neighbours the `cuts`
/// recorded. Each cut was made against the text the earlier cuts left, so a
/// neighbour it names may itself have been cut: the sentences that stood
/// before this one are those the records chain back from it, those that stood
/// after it the ones they chain forward to. It goes after the last of the
/// first that is in the text, or before the first of the second, and only
/// where nothing else stands between them.
pub(crate) fn put_place<'a>(text: &str, sentence: &'a str, cuts: &[&'a CutPlace]) -> Placing {
    let mut edges: Vec<(&str, &str)> = Vec::new();
    for &cut in cuts {
        if let Some(before) = &cut.before {
            edges.push((before.as_str(), cut.sentence.as_str()));
        }
        if let Some(after) = &cut.after {
            edges.push((cut.sentence.as_str(), after.as_str()));
        }
    }
    let earlier = reach(sentence, &edges, false);
    let later = reach(sentence, &edges, true);
    if earlier.iter().any(|name| later.contains(name)) {
        return Placing::Ambiguous;
    }
    let found = |names: &[&'a str]| -> Option<Vec<(&'a str, usize, usize)>> {
        let mut present = Vec::new();
        for name in names.iter().filter(|name| !name.is_empty()) {
            if text.contains(name) {
                let (start, end) = span_of(text, name)?;
                present.push((*name, start, end));
            }
        }
        Some(present)
    };
    let (Some(earlier), Some(later)) = (found(&earlier), found(&later)) else {
        return Placing::Ambiguous;
    };
    let last = earlier.iter().max_by_key(|(_, _, end)| *end);
    let first = later.iter().min_by_key(|(_, start, _)| *start);
    match (last, first) {
        (None, None) => Placing::Unknown,
        (Some((name, _, _)), None) => Placing::At(Anchor::After((*name).to_owned())),
        (None, Some((name, _, _))) => Placing::At(Anchor::Before((*name).to_owned())),
        (Some((name, _, end)), Some((_, start, _))) => {
            let between = text.get(*end..*start);
            if between.is_some_and(|between| between.trim().is_empty()) {
                Placing::At(Anchor::After((*name).to_owned()))
            } else {
                Placing::Ambiguous
            }
        }
    }
}

/// Every cut the machine recorded in `field` of section `unit_index` among
/// `claims`.
pub(crate) fn cuts_in<'a>(
    claims: impl IntoIterator<Item = &'a ClaimLedgerEntry>,
    unit_index: usize,
    field: &str,
) -> Vec<CutPlace> {
    claims
        .into_iter()
        .filter(|claim| {
            let location = claim.location();
            location.unit_index == unit_index && location.field == field
        })
        .filter_map(|claim| claim.cut_place().cloned())
        .collect()
}

/// The cuts in that section, and `entry`'s own even when its recorded section
/// is not the one the text was found in.
pub(crate) fn cuts_for<'a>(
    claims: impl IntoIterator<Item = &'a ClaimLedgerEntry>,
    entry: &ClaimLedgerEntry,
    unit_index: usize,
    field: &str,
) -> Vec<CutPlace> {
    let mut cuts = cuts_in(claims, unit_index, field);
    if entry.location().unit_index != unit_index {
        cuts.extend(entry.cut_place().cloned());
    }
    cuts
}

/// `text` with `sentence` written in where the `cuts` put it; where they
/// cannot say, at the `stored` anchor, else at the end. An end the user chose
/// stands.
pub(crate) fn put_sentence(
    text: &str,
    sentence: &str,
    cuts: &[CutPlace],
    stored: &Anchor,
) -> Option<String> {
    if *stored != Anchor::End {
        let refs: Vec<&CutPlace> = cuts.iter().collect();
        if let Placing::At(anchor) = put_place(text, sentence, &refs) {
            if let Some(put) = insert_sentence(text, &anchor, sentence) {
                return Some(put);
            }
        }
    }
    insert_sentence(text, stored, sentence)
        .or_else(|| insert_sentence(text, &Anchor::End, sentence))
}

/// `text` with `removed` written back between `follows` and `precedes`, the
/// shortest texts that stood on either side of it, but only where they still
/// meet: both, exactly once together; only `precedes`, at the start; only
/// `follows`, at the end; neither, in a blank text. `None` otherwise.
pub(crate) fn put_at_junction(
    text: &str,
    removed: &str,
    follows: Option<&str>,
    precedes: Option<&str>,
) -> Option<String> {
    let at = match (follows, precedes) {
        (Some(follows), Some(precedes)) => {
            let (start, _) = span_of(text, &format!("{follows}{precedes}"))?;
            start + follows.len()
        }
        (None, Some(precedes)) => text.starts_with(precedes).then_some(0)?,
        (Some(follows), None) => text.ends_with(follows).then_some(text.len())?,
        (None, None) => return text.trim().is_empty().then(|| removed.to_owned()),
    };
    Some(format!("{}{removed}{}", text.get(..at)?, text.get(at..)?))
}

#[cfg(test)]
mod put_place_tests;
