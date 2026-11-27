use lattice_core::application::services::model_contract::shown::{Drawn, Kind, Shown};
use lattice_core::application::services::model_contract::units::{
    Grain, Quoted, Texts, UnitRef, Units,
};
use lattice_core::shared::text::same_text;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A citation a part holds, by where it is in the material as shown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CitationRef {
    pub part: String,
    /// `None`: the part's own `sourceIndex` and `quote`.
    pub index: Option<usize>,
}

/// A citation of a part: its own pair, or an element of its `citations`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Citation {
    pub(super) at: CitationRef,
    pub(super) source_index: Option<u64>,
    pub(super) quote: String,
    pub(super) covers: Vec<UnitRef>,
}

impl Kind for Citation {
    const PREFIX: &'static str = "c";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = CitationRef;

    fn reference(&self) -> CitationRef {
        self.at.clone()
    }
}

impl Citation {
    #[cfg(test)]
    pub(crate) fn at(&self) -> &CitationRef {
        &self.at
    }

    pub(crate) fn source_index(&self) -> Option<u64> {
        self.source_index
    }

    pub(crate) fn quote(&self) -> &str {
        &self.quote
    }
}

/// The excerpts a request may show, and the sentences of them a citation may quote.
#[derive(Debug, Clone, Default)]
pub(crate) struct Quotes {
    texts: Texts<u64>,
    quotable: Vec<Quoted>,
}

/// Where the excerpt of a source stands among the texts of a request.
fn place_of(source_index: u64) -> String {
    source_index.to_string()
}

impl Quotes {
    /// From source entries shaped `{"sourceIndex": n, "excerpt": "…", …}`.
    pub(crate) fn of<'a>(sources: impl IntoIterator<Item = &'a Value>) -> Self {
        let mut quotes = Self::default();
        for source in sources {
            if let (Some(source_index), Some(excerpt)) = (
                source.get("sourceIndex").and_then(Value::as_u64),
                source.get("excerpt").and_then(Value::as_str),
            ) {
                quotes.push(source_index, excerpt);
            }
        }
        quotes
    }

    /// Adds an excerpt of source `source_index`. One already held for that source is not added again.
    pub(crate) fn push(&mut self, source_index: u64, excerpt: &str) {
        let held = self
            .texts
            .iter()
            .any(|(of, units)| *of == source_index && units.text() == excerpt);
        if held {
            return;
        }
        self.texts.push(
            source_index,
            &place_of(source_index),
            excerpt,
            Grain::Sentence,
        );
        if let Some((_, units)) = self.texts.iter().last() {
            self.quotable.extend(
                units
                    .quoted()
                    .into_iter()
                    .filter(|sentence| same_text::long_enough_quote(sentence.text())),
            );
        }
    }

    /// The sentences a citation may quote, in the order added.
    pub(crate) fn all(&self) -> &[Quoted] {
        &self.quotable
    }

    /// The source a quotable sentence is of, and its words. `None` for a sentence that is not quotable.
    pub(crate) fn get(&self, at: UnitRef) -> Option<(u64, &str)> {
        if !self.quotable.iter().any(|held| held.reference() == at) {
            return None;
        }
        self.texts.found(at).map(|(source, text)| (*source, text))
    }

    /// `excerpt` (of source `source_index`) as it is, the bracketed handle of each quotable `shown` holds
    /// before its sentence.
    pub(crate) fn marked(
        &self,
        source_index: u64,
        excerpt: &str,
        shown: &Shown<'_, Quoted>,
    ) -> String {
        Units::of_at(excerpt, Grain::Sentence, &place_of(source_index)).marked(shown)
    }
}
