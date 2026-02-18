//! Recent Documents Use Cases

pub mod clear_recent_history;
pub mod get_recent_documents;
pub mod track_access;

pub use clear_recent_history::ClearRecentHistoryUseCase;
pub use get_recent_documents::GetRecentDocumentsUseCase;
pub use track_access::TrackAccessUseCase;
