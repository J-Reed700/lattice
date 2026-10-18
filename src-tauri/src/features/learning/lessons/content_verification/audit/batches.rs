//! Packing an audit's claims into requests the Verifier's window holds
//! (lesson claim review §14.4). No sizes are assumed: a [`Sizer`] measures
//! each candidate request against the loaded model's window.
//!
//! Claims go in lesson order. A claim whose section and passages do not fit
//! one request on their own is split by passages first (each part is judged
//! on its own passages, and the parts' verdicts combine); a passage that
//! does not fit even with the whole section is sent with only the paragraph
//! that holds the claim's quote. A claim that does not fit even so is not
//! sent: it is reported with the tokens its smallest request needs.

use super::AuditClaim;

/// How much of the claim's lesson section a request carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Context {
    /// The whole section, sent once per request however many claims cite it.
    Unit,
    /// Only the paragraph holding the claim's quote, inline with the claim.
    Paragraph,
}

/// One claim, or one part of a split claim, in a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Item {
    /// Index into the audit's claims.
    pub claim: usize,
    /// Indices into that claim's passages: all of them, or this part's.
    pub passages: Vec<usize>,
    pub context: Context,
    /// `(part, of)`, 1-based, when the claim is split.
    pub part: Option<(usize, usize)>,
}

/// A claim no request can hold, with what its smallest request measures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Oversize {
    pub claim: usize,
    pub needed_tokens: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Planned {
    pub batches: Vec<Vec<Item>>,
    pub oversize: Vec<Oversize>,
}

/// Measures candidate requests against the Verifier's window.
pub(crate) trait Sizer {
    /// Whether the request for `items` fits, its answer reservation included.
    fn fits(&self, items: &[Item]) -> bool;
    /// The tokens the request for `items` needs: input and answer.
    fn needed(&self, items: &[Item]) -> usize;
}

/// The batches for `claims`, in lesson order.
pub(crate) fn plan(claims: &[AuditClaim], sizer: &dyn Sizer) -> Planned {
    let mut planned = Planned::default();
    let mut current: Vec<Item> = Vec::new();
    for (index, claim) in claims.iter().enumerate() {
        let whole = Item {
            claim: index,
            passages: (0..claim.passages.len()).collect(),
            context: Context::Unit,
            part: None,
        };
        let items = if sizer.fits(std::slice::from_ref(&whole)) {
            vec![whole]
        } else {
            match split(index, claim, sizer) {
                Ok(parts) => parts,
                Err(oversize) => {
                    planned.oversize.push(oversize);
                    continue;
                }
            }
        };
        for item in items {
            let mut candidate = current.clone();
            candidate.push(item.clone());
            if current.is_empty() || sizer.fits(&candidate) {
                current = candidate;
            } else {
                planned
                    .batches
                    .push(std::mem::replace(&mut current, vec![item]));
            }
        }
    }
    if !current.is_empty() {
        planned.batches.push(current);
    }
    planned
}

/// The parts of a claim too large for one request, or why it has none.
fn split(index: usize, claim: &AuditClaim, sizer: &dyn Sizer) -> Result<Vec<Item>, Oversize> {
    let item = |passages: Vec<usize>, context| Item {
        claim: index,
        passages,
        context,
        part: None,
    };
    if claim.passages.is_empty() {
        let reduced = item(Vec::new(), Context::Paragraph);
        return if sizer.fits(std::slice::from_ref(&reduced)) {
            Ok(vec![reduced])
        } else {
            Err(Oversize {
                claim: index,
                needed_tokens: sizer.needed(&[reduced]),
            })
        };
    }
    let mut parts: Vec<Item> = Vec::new();
    let mut group: Vec<usize> = Vec::new();
    for passage in 0..claim.passages.len() {
        let mut grown = group.clone();
        grown.push(passage);
        if sizer.fits(&[item(grown.clone(), Context::Unit)]) {
            group = grown;
            continue;
        }
        if !group.is_empty() {
            parts.push(item(std::mem::take(&mut group), Context::Unit));
        }
        if sizer.fits(&[item(vec![passage], Context::Unit)]) {
            group = vec![passage];
            continue;
        }
        let reduced = item(vec![passage], Context::Paragraph);
        if !sizer.fits(std::slice::from_ref(&reduced)) {
            return Err(Oversize {
                claim: index,
                needed_tokens: sizer.needed(&[reduced]),
            });
        }
        parts.push(reduced);
    }
    if !group.is_empty() {
        parts.push(item(group, Context::Unit));
    }
    let of = parts.len();
    if of > 1 {
        for (position, part) in parts.iter_mut().enumerate() {
            part.part = Some((position + 1, of));
        }
    }
    Ok(parts)
}

/// The paragraph of `unit_text` that holds `quote`; the quote itself when
/// no paragraph holds it whole.
pub(crate) fn paragraph<'a>(unit_text: &'a str, quote: &'a str) -> &'a str {
    unit_text
        .split("\n\n")
        .map(str::trim)
        .find(|paragraph| !quote.trim().is_empty() && paragraph.contains(quote.trim()))
        .unwrap_or(quote)
}

#[cfg(test)]
#[path = "batches_tests.rs"]
mod tests;
