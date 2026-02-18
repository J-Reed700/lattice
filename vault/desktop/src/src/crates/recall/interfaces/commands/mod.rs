pub mod api_boundary;
pub mod backup;
pub mod cache;
// DELETED: pub mod chat; - Legacy command (replaced by conversation_chat.rs and qa_commands.rs)
pub mod config;
pub mod conversation;
pub mod conversation_chat;
pub mod credentials;
pub mod custom_model_commands;
// DELETED: pub mod daily_notes_commands; - Feature removed
pub mod daily_notes_workspace;
pub mod document_list;
pub mod downloads;
pub mod embeddings;
pub mod extraction;
pub mod favorites;
pub mod file;
pub mod function_calling_commands;
pub mod health_commands;
pub mod hf_settings;
// DELETED: pub mod index; - Legacy indexing commands (DDD implementation active)
pub mod indexing_commands; // DDD indexing commands (content-addressed storage)
pub mod initialization;
pub mod mentions;
pub mod metrics_commands;
pub mod qa_commands;
pub mod recent_documents;
pub mod search_commands; // DDD search commands (replaces old search.rs)
                         // DELETED: pub mod settings; - Old SidecarState architecture
pub mod batch_file_import;
pub mod batch_history;
pub mod batch_url_import;
pub mod consolidated;
pub mod llm;
pub mod model_management;
pub mod model_management_commands;
pub mod model_setup;
pub mod tag_commands_full;
pub mod updates_commands;
pub mod web_ingest;

// DDD Command Module Aliases (for backward compatibility during migration)
pub use backup as backup_commands;
pub use cache as cache_commands;
pub use conversation as conversation_commands;
pub use credentials as credentials_commands;
pub use favorites as favorites_commands;
pub use file as file_commands;
pub use health_commands as health;
pub use mentions as mentions_commands;
pub use recent_documents as recent_commands;
pub use tag_commands_full as tags;
// DELETED: pub use settings as settings_commands; - Old SidecarState architecture
