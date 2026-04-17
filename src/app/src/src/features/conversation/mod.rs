//! # Conversation feature
//!
//! Persistent chat conversations: CRUD on conversations and messages,
//! the conversation summarizer (background saga), the conversation
//! service (infra), and the full chat-with-RAG controller pipeline
//! (`chat.rs` + `chat/`). Largest feature by file count after search.
//!
//! ## File layout
//!
//! | Path                                              | Canonical module path                                                          |
//! |---------------------------------------------------|--------------------------------------------------------------------------------|
//! | `dto.rs`                                          | `crate::application::dtos::conversation_dto`                                   |
//! | `message_bookmark_dto.rs`                         | `crate::application::dtos::conversation_message_bookmark_dto`                  |
//! | `space_dto.rs`                                    | `crate::application::dtos::conversation_space_dto`                             |
//! | `mapper.rs`                                       | `crate::application::mappers::conversation_mapper` (application mapper)        |
//! | `summarizer.rs`                                   | `crate::application::services::conversation_summarizer`                        |
//! | `use_cases/`                                      | `crate::application::use_cases::conversation`                                  |
//! | `domain/conversation.rs`                          | `crate::domain::conversation`                                                  |
//! | `domain/summary.rs`                               | `crate::domain::conversation_summary`                                          |
//! | `events.rs`                                       | `crate::infrastructure::events::conversation_events`                           |
//! | `repository.rs`                                   | `crate::infrastructure::persistence::repositories::conversation_repository`    |
//! | `persistence_mapper.rs`                           | `crate::infrastructure::persistence::mappers::conversation_mapper` (infra mapper) |
//! | `saga.rs`                                         | `crate::infrastructure::sagas::conversation_summary_saga`                      |
//! | `service.rs`                                      | `crate::infrastructure::services::conversation_service`                        |
//! | `trait_def.rs`                                    | `crate::infrastructure::services::traits` (merged re-exports)                  |
//! | `mocks.rs`                                        | `crate::infrastructure::services::mocks` (merged re-exports)                   |
//! | `commands.rs`                                     | `crate::interfaces::commands::conversation` (basic CRUD commands)              |
//! | `plugin_impl.rs`                                  | `crate::interfaces::commands::conversation_plugin_impl`                        |
//! | `chat.rs`                                         | `crate::interfaces::commands::conversation_chat` (chat controller — top file)  |
//! | `chat/{cancellation,persistence,prompting,tool_loop,verification}.rs` | private modules of `conversation_chat`                  |
//! | `chat/retrieval/`                                 | RAG pipeline (15 files: kb_retrieval, pipeline, policy, rerank, etc.)          |
//! | `plugin.rs`                                       | `crate::plugins::conversation_plugin`                                          |
//!
//! ## Deliberately NOT moved (shared infra)
//!
//! - `application/ports/conversation_repository_port.rs` — shared
//!   port consumed by use cases.
//! - DI wiring in `interfaces/di/{modules,container}.rs` — per oracle
//!   ruling, do not dismantle the DI container.
//!
//! ## Two `conversation_mapper.rs` files
//!
//! Application and persistence layers both had a file named
//! `conversation_mapper.rs`. To coexist in the feature root they were
//! disambiguated:
//! - `mapper.rs`               → application/mappers
//! - `persistence_mapper.rs`   → infrastructure/persistence/mappers
//! Same pattern as `features/embedding/persistence_mapper.rs`.
//!
//! ## `chat.rs` + `chat/` Rust pattern
//!
//! `chat.rs` declares submodules (`mod cancellation; mod retrieval;`
//! …). Rust resolves those submodules from the physical location of
//! `chat.rs` — after the Strangler Fig redirect, that is
//! `features/conversation/chat.rs`, with submodules at
//! `features/conversation/chat/*.rs`. The pattern survives `git mv`
//! cleanly because Rust's module-lookup is filesystem-relative.
//!
//! ## RAG retrieval pipeline
//!
//! `chat/retrieval/` is a 15-file orchestration layer that imports
//! heavily from `crate::infrastructure::search::*` (search engine) and
//! `crate::domain::qa::*` (question-answering domain). These imports
//! continue to resolve through the Strangler Fig redirects established
//! by the search and qa migrations. The retrieval subtree is *only*
//! consumed by `chat.rs` next door — no external feature imports it.
