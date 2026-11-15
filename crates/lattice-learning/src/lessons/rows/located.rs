//! Where a sentence a request names stands among the rows.

use super::*;

impl Rows {
    /// The units of the text at `pointer` that overlap an occurrence of `quote` in it as shown.
    pub(crate) fn units_over(&self, pointer: &str, quote: &str) -> Vec<UnitRef> {
        let quote = quote.trim();
        let Some(held) = self.fields.iter().find(|held| held.pointer == pointer) else {
            return Vec::new();
        };
        let Some(text) = self.base.pointer(pointer).and_then(Value::as_str) else {
            return Vec::new();
        };
        if quote.is_empty() {
            return Vec::new();
        }
        let found: Vec<(usize, usize)> = text
            .match_indices(quote)
            .map(|(at, said)| (at, at + said.len()))
            .collect();
        held.units
            .spans()
            .into_iter()
            .filter(|(_, range)| {
                found
                    .iter()
                    .any(|span| same_text::overlaps((range.start, range.end), *span))
            })
            .map(|(unit, _)| unit)
            .collect()
    }

    /// The units the citation `at` is for.
    pub(crate) fn covered_by(&self, at: &CitationRef) -> Vec<UnitRef> {
        self.citations
            .iter()
            .find(|held| held.at == *at)
            .map(|held| held.covers.clone())
            .unwrap_or_default()
    }
}

impl Rows {
    /// The unit that follows `unit` in its text as shown.
    pub(crate) fn next_of(&self, unit: UnitRef) -> Option<UnitRef> {
        let spans = self.field_of(&unit)?.units.spans();
        let at = spans.iter().position(|(held, _)| *held == unit)?;
        spans.get(at + 1).map(|(held, _)| *held)
    }
}
