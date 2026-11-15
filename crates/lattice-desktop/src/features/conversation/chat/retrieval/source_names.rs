//! A source of the conversation is a named thing: its number is its handle's
//! own, given the first time it is shown and kept for the whole conversation.

use lattice_core::application::contracts::sources::SourceDto;
use lattice_core::application::services::model_contract::kept::TurnNames;
use lattice_core::application::services::model_contract::shown::{Drawn, Kind};
use tracing::warn;

use super::source_citations::WEB_SOURCE_PREFIX;

#[cfg(test)]
mod tests;

/// A passage of a document, or a web page, a reply may cite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub document_id: String,
    pub chunk_id: String,
}

impl Source {
    /// The passage a source is. A web page is one source however many searches
    /// returned it, and a recalled passage is the passage it was before the recall.
    pub fn of(source: &SourceDto) -> Self {
        let chunk_id = if source.document_id.starts_with(WEB_SOURCE_PREFIX) {
            String::new()
        } else {
            let recalled = format!("recalled:{}:", source.document_id);
            source
                .chunk_id
                .strip_prefix(&recalled)
                .unwrap_or(&source.chunk_id)
                .to_owned()
        };
        Self {
            document_id: source.document_id.clone(),
            chunk_id,
        }
    }
}

impl Kind for Source {
    const PREFIX: &'static str = "s";
    const DRAWN: Drawn = Drawn::Number;
    type Ref = (String, String);

    fn reference(&self) -> (String, String) {
        (self.document_id.clone(), self.chunk_id.clone())
    }
}

/// Gives every source its number: the one its handle has in the conversation,
/// minted now for a source never shown before. The only place a source gets a number.
pub fn name_sources(names: &TurnNames, sources: &mut [SourceDto]) {
    let things: Vec<Source> = sources.iter().map(Source::of).collect();
    names.with(|scope| {
        let shown = match scope.request().show(&things) {
            Ok(shown) => shown,
            Err(error) => {
                warn!(%error, "Sources could not be named");
                return;
            }
        };
        for (source, handle) in sources.iter_mut().zip(shown.handles()) {
            source.citation_id = Some(handle.number());
        }
    });
}

/// A source as the model is shown it, `[s12]`; empty before it has a number.
pub fn label(source: &SourceDto) -> String {
    source.citation_id.map(marker).unwrap_or_default()
}

/// The written form of the source numbered `number`, `[s12]`.
pub fn marker(number: u32) -> String {
    format!("[{}{number}]", Source::PREFIX)
}
