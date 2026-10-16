//! Builds the conversation plugin: its background jobs and its command handler.

use super::*;
use crate::features::conversation::{chat, synthesis};
use std::sync::Arc;
use tauri::plugin::{Builder, TauriPlugin};

pub fn init() -> TauriPlugin<tauri::Wry> {
    Builder::new("conversation")
        .setup(|app, _api| {
            use tauri::Manager;
            let container = app.state::<Container>().inner().clone();
            let jobs = Arc::clone(container.jobs());
            // Registration settles research turns and syntheses the last
            // process left running, then resumes them from their checkpoints.
            tauri::async_runtime::block_on(async {
                jobs.register(
                    chat::DEEP_RESEARCH,
                    Arc::new(chat::DeepResearchJob::new(
                        container.share(),
                        chat::app_event_sink(app.clone()),
                    )),
                    chat::research_job_config(),
                )
                .await?;
                jobs.register(
                    synthesis::JOURNAL_SYNTHESIS,
                    Arc::new(synthesis::JournalSynthesisJob::new(container.share())),
                    synthesis::synthesis_job_config(),
                )
                .await
            })?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            create_conversation,
            set_conversation_model_choice,
            generate_chat_starters,
            get_conversation,
            list_conversations,
            delete_conversation,
            get_conversation_messages,
            rename_conversation,
            chat_with_conversation,
            create_conversation_space,
            list_conversation_spaces,
            create_journal,
            list_journals,
            list_conversation_space_members,
            upsert_conversation_space_member,
            remove_conversation_space_member,
            update_conversation_space,
            archive_conversation_space,
            update_journal,
            archive_journal,
            delete_journal,
            move_conversation_to_space,
            add_conversation_to_journal,
            remove_conversation_from_journal,
            set_conversation_saved,
            set_conversation_bookmarked,
            set_conversation_pinned,
            set_conversation_archived,
            list_space_documents,
            list_conversation_linked_documents,
            remove_conversation_linked_document,
            add_conversation_web_source,
            list_conversation_web_sources,
            remove_conversation_web_source,
            list_document_space_memberships,
            set_document_space_membership,
            set_documents_space_membership,
            add_documents_to_library,
            bookmark_conversation_message,
            unbookmark_conversation_message,
            delete_conversation_message,
            list_message_bookmarks,
            list_conversations_explorer,
            list_journal_conversations,
            set_journal_entry_pinned,
            list_journal_entry_pins,
            synthesize_journal_entries,
            list_journal_syntheses,
            get_journal_synthesis_result,
            mark_journal_synthesis_applied,
            dismiss_journal_synthesis,
            retry_journal_synthesis,
            truncate_conversation_after,
            fork_conversation,
            create_conversation_tangent,
            list_conversation_tangents,
            promote_conversation_tangent,
            continue_in_new_conversation,
            regenerate_response,
            compact_conversation,
            get_conversation_memory,
            manage_knowledge,
        ])
        .build()
}
