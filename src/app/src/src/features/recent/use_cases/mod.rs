//! Recent documents feature — use cases.

pub mod clear_history;
pub mod get_recent;
pub mod track_access;

pub use clear_history::ClearRecentHistoryUseCase;
pub use get_recent::GetRecentDocumentsUseCase;
pub use track_access::TrackAccessUseCase;
