//! Tool vocabulary: the tools the registry offers, the calls a model makes and
//! their results, and the input and output records of the built-in tools.

mod calls;
mod dto;

pub use calls::{FunctionCall, FunctionResult, RegistryStats, ToolDefinition};
pub use dto::{
    AttachmentItem, CustomQueryToolInput, CustomQueryToolOutput, DocumentEvidence,
    DocumentEvidenceMatch, DocumentListItem, DocumentMetadata, DocumentResult,
    FetchUrlContentInput, FilterMode, FunctionCallError, GetDocumentInput, GetDocumentOutput,
    ListAttachmentsOutput, ListDocumentsInput, ListDocumentsOutput, SearchMode,
    SemanticSearchInput, SemanticSearchOutput, SortField, SortOrder, WikiSearchInput,
    WikiSearchOutput, WikiSearchResult, WikiSummaryInput, WikiSummaryOutput,
};
