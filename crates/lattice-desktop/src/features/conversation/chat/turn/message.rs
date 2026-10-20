//! The turn's message with its evidence held apart, so it can be cut to fit.
//!
//! The prompt numbers its passages and wraps them in the turn's own template,
//! so it is rendered here rather than by the context assembler. Kept in parts,
//! it can still be handed to the assembler as evidence it may give up: the
//! lowest-ranked item goes first when the request will not fit the window, and
//! the turn then drops the sources of whatever the model was not shown.
use super::*;
use crate::features::conversation::chat::prompt_settings::LLMPromptSettingsDto;
use crate::features::conversation::chat::retrieval::SufficiencyVerdict;

/// Where an item of evidence sits in the message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Attachments,
    Folder,
    PriorEvidence,
    Followup,
    Library,
    LinkedPages,
    WebPages,
}

/// One piece of evidence, and the citation numbers it shows.
#[derive(Debug, Clone)]
struct Item {
    section: Section,
    text: String,
    citations: Vec<u32>,
}

/// What the message says besides its evidence.
#[derive(Debug, Clone)]
pub(super) struct MessageFrame {
    pub(super) prompt_settings: LLMPromptSettingsDto,
    pub(super) question: String,
    pub(super) is_greeting: bool,
    /// The turn belongs to an Explorer conversation.
    pub(super) explorer: bool,
    pub(super) flags: SearchFlags,
    pub(super) deep_research_searches: Option<Vec<String>>,
    pub(super) web_search_error: Option<String>,
    pub(super) kb_unavailable_reason: Option<String>,
    pub(super) kb_attempted: bool,
    pub(super) sufficiency: Option<SufficiencyVerdict>,
}

/// The turn's message, its evidence best first.
///
/// The order is the order evidence is given up in, read backwards. The files
/// the reader attached outrank everything; then the folder on screen, the
/// passages earlier turns cited and the document under discussion; then the
/// library passages in their ranked order; then pages linked to the
/// conversation; and last the web results, which the prompt itself calls
/// supplemental.
#[derive(Debug, Clone)]
pub(super) struct TurnMessage {
    frame: MessageFrame,
    items: Vec<Item>,
}

impl TurnMessage {
    pub(super) fn new(frame: MessageFrame) -> Self {
        Self {
            frame,
            items: Vec::new(),
        }
    }

    pub(super) fn with_attachments(self, text: Option<String>, citations: Vec<u32>) -> Self {
        self.with_block(Section::Attachments, text, citations)
    }

    pub(super) fn with_folder(self, text: Option<String>) -> Self {
        self.with_block(Section::Folder, text, Vec::new())
    }

    pub(super) fn with_prior_evidence(self, text: Option<String>, citations: Vec<u32>) -> Self {
        self.with_block(Section::PriorEvidence, text, citations)
    }

    pub(super) fn with_followup(self, text: Option<String>, citations: Vec<u32>) -> Self {
        self.with_block(Section::Followup, text, citations)
    }

    /// Library passages, best first, each beside its citation number.
    pub(super) fn with_library(self, passages: Vec<(u32, String)>) -> Self {
        self.with_entries(Section::Library, passages)
    }

    pub(super) fn with_linked_pages(self, text: Option<String>) -> Self {
        self.with_block(Section::LinkedPages, text, Vec::new())
    }

    /// Web results, best first, each beside its citation number.
    pub(super) fn with_web_pages(self, pages: Vec<(u32, String)>) -> Self {
        self.with_entries(Section::WebPages, pages)
    }

    fn with_block(mut self, section: Section, text: Option<String>, citations: Vec<u32>) -> Self {
        if let Some(text) = text {
            self.items.push(Item {
                section,
                text,
                citations,
            });
        }
        self
    }

    fn with_entries(mut self, section: Section, entries: Vec<(u32, String)>) -> Self {
        self.items
            .extend(entries.into_iter().map(|(citation, text)| Item {
                section,
                text,
                citations: vec![citation],
            }));
        self
    }

    /// How many items of evidence the message carries whole.
    pub(super) fn items(&self) -> usize {
        self.items.len()
    }

    /// The message carrying only the best `keep` items. With every item it is
    /// the message the turn would send if the window held all of it.
    pub(super) fn render(&self, keep: usize) -> String {
        let kept = self.items.get(..keep).unwrap_or(&self.items);
        let section = |wanted: Section| {
            let texts: Vec<&str> = kept
                .iter()
                .filter(|item| item.section == wanted)
                .map(|item| item.text.as_str())
                .collect();
            (!texts.is_empty()).then(|| texts.join("\n\n"))
        };
        let frame = &self.frame;
        PromptMessageBuilder::new(
            &frame.prompt_settings,
            &frame.question,
            frame.is_greeting,
            frame.flags,
        )
        .in_explorer(frame.explorer)
        .with_explorer_context(section(Section::Folder))
        .with_deep_research_searches(frame.deep_research_searches.clone())
        .with_prior_evidence_context(section(Section::PriorEvidence))
        .with_attachment_context(section(Section::Attachments))
        .with_followup_context(section(Section::Followup))
        .with_kb_context(section(Section::Library))
        .with_linked_web_sources_context(section(Section::LinkedPages))
        .with_web_context(section(Section::WebPages))
        .with_web_search_error(frame.web_search_error.clone())
        .with_kb_unavailable_reason(frame.kb_unavailable_reason.clone())
        .with_kb_attempted(frame.kb_attempted)
        .with_kb_sufficiency(frame.sufficiency.as_ref())
        .with_evidence_cut(kept.len() < self.items.len())
        .build()
    }

    /// Drop the sources only the items past the first `kept` showed. A source
    /// the model was not shown is not a source of its answer; one no item
    /// shows at all is left as it is.
    pub(super) fn keep_sources(&self, kept: usize, sources: &mut Vec<SourceDto>) {
        let Some(cut) = self.items.get(kept..).filter(|cut| !cut.is_empty()) else {
            return;
        };
        let shown: HashSet<u32> = self
            .items
            .get(..kept)
            .unwrap_or_default()
            .iter()
            .flat_map(|item| item.citations.iter().copied())
            .collect();
        let unshown: HashSet<u32> = cut
            .iter()
            .flat_map(|item| item.citations.iter().copied())
            .filter(|citation| !shown.contains(citation))
            .collect();
        sources.retain(|source| {
            source
                .citation_id
                .is_none_or(|citation| !unshown.contains(&citation))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::conversation::chat::prompt_settings::normalize_prompt_settings;

    fn frame() -> MessageFrame {
        MessageFrame {
            prompt_settings: normalize_prompt_settings(LLMPromptSettingsDto::default()),
            question: "When does the contract renew?".into(),
            is_greeting: false,
            explorer: false,
            flags: SearchFlags::from_preferences(None),
            deep_research_searches: None,
            web_search_error: None,
            kb_unavailable_reason: None,
            kb_attempted: true,
            sufficiency: None,
        }
    }

    fn passage(citation: u32) -> (u32, String) {
        (
            citation,
            format!("[{citation}] Document: d{citation}.md\nContent: passage {citation}"),
        )
    }

    fn page(citation: u32) -> (u32, String) {
        (
            citation,
            format!("[{citation}] Page {citation}\nhttps://example.com/{citation}"),
        )
    }

    fn source(citation: Option<u32>) -> SourceDto {
        SourceDto {
            page_number: None,
            from_ocr: false,
            document_id: "d".into(),
            chunk_id: format!("c{citation:?}"),
            content: String::new(),
            score: 0.9,
            path: None,
            position: None,
            file_name: "d.md".into(),
            file_path: "d.md".into(),
            mime_type: "text/markdown".into(),
            category: "Text".into(),
            file_size_bytes: 0,
            modified_at: String::new(),
            excerpt: None,
            highlights: None,
            section: None,
            chunk_index: None,
            chunk_excerpts: None,
            citation_id: citation,
            web_snapshot: None,
        }
    }

    fn message() -> TurnMessage {
        TurnMessage::new(frame())
            .with_attachments(Some("Attached Files:\n[1] notes.txt".into()), vec![1])
            .with_library(vec![passage(2), passage(3), passage(4)])
            .with_web_pages(vec![page(5), page(6)])
    }

    #[test]
    fn the_whole_message_is_the_one_the_prompt_builder_renders() {
        let whole = message().render(6);
        let built = PromptMessageBuilder::new(
            &frame().prompt_settings,
            "When does the contract renew?",
            false,
            SearchFlags::from_preferences(None),
        )
        .with_attachment_context(Some("Attached Files:\n[1] notes.txt".into()))
        .with_kb_context(Some(
            [passage(2).1, passage(3).1, passage(4).1].join("\n\n"),
        ))
        .with_web_context(Some([page(5).1, page(6).1].join("\n\n")))
        .with_kb_attempted(true)
        .build();
        assert_eq!(whole, built);
        assert!(!whole.contains("Context window:"));
    }

    #[test]
    fn web_pages_go_before_library_passages_and_the_lowest_ranked_goes_first() {
        let message = message();
        let four = message.render(4);
        assert!(four.contains("passage 4") && !four.contains("Page 5"));
        let two = message.render(2);
        assert!(two.contains("passage 2") && !two.contains("passage 3"));
        assert!(two.contains("notes.txt"));
        assert!(two.contains("Context window:"), "the cut is stated");
    }

    #[test]
    fn a_message_with_every_item_cut_says_evidence_was_found_but_did_not_fit() {
        let bare = message().render(0);
        assert!(!bare.contains("passage 2") && !bare.contains("notes.txt"));
        assert!(bare.contains("no room"), "{bare}");
        assert!(!bare.contains("no passage came back"));
    }

    #[test]
    fn only_the_sources_of_cut_items_are_dropped() {
        let mut sources: Vec<SourceDto> = (1..=6).map(|id| source(Some(id))).collect();
        sources.push(source(None));
        message().keep_sources(3, &mut sources);
        let kept: Vec<Option<u32>> = sources.iter().map(|source| source.citation_id).collect();
        assert_eq!(kept, vec![Some(1), Some(2), Some(3), None]);

        let mut all: Vec<SourceDto> = (1..=6).map(|id| source(Some(id))).collect();
        message().keep_sources(6, &mut all);
        assert_eq!(all.len(), 6, "nothing is dropped when everything fits");
    }
}
