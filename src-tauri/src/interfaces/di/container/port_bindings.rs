//! Ports that cross features, bound on the composition root: each one is
//! owned by one feature and satisfied from another, and this is the one place
//! allowed to name both.

use super::Container;
use lattice_core::shared::error::Result;

/// The note-index sink's readiness gate, implemented over the embedding
/// runtime and its loader: the same single-flight load with retry cooldown
/// that `get_or_load_embedding` gives every other caller, then the shared
/// readiness check the `index_file` command applies. Cross-feature — the
/// port is indexing's, the runtime is embedding's — so it is bound here,
/// where both may be named.
#[async_trait::async_trait]
impl crate::features::indexing::note_index_sink::EmbeddingReadiness
    for crate::features::embedding::runtime::ActiveEmbedding
{
    async fn ensure_ready(&self) -> Result<()> {
        crate::features::indexing::commands::require_embedding_ready(self.get_or_load().await).await
    }
}

/// The note-index sink's chunk-size preference, implemented over the
/// settings use case. The impl sits at the composition root because it
/// crosses two features — indexing's port, satisfied from settings — and
/// the root is the one place allowed to name both.
#[async_trait::async_trait]
impl crate::features::indexing::note_index_sink::ChunkSizePreference
    for crate::features::settings::use_cases::GetSettingsUseCase
{
    async fn chunk_size(&self) -> Result<u32> {
        self.execute()
            .await
            .map(|settings| settings.indexing.chunk_size)
    }
}

/// The reranker setup's toggle, implemented over the settings use case, for
/// the same reason as the chunk-size preference: search's port, satisfied
/// from settings, bound where both may be named.
#[async_trait::async_trait]
impl crate::features::search::reranker_setup::RerankingToggle for Container {
    async fn enabled(&self) -> Result<bool> {
        self.get_settings_use_case()
            .execute()
            .await
            .map(|settings| settings.search.enable_reranking)
    }
}

/// The journal synthesis's entry reads, implemented over the references and
/// daily-notes repositories. The impl sits at the composition root for the
/// same reason as its siblings: conversation's port, satisfied from two
/// other features, bound where all three may be named.
#[async_trait::async_trait]
impl crate::features::conversation::chat::ports::SynthesisEntries for Container {
    async fn references_created_since(
        &self,
        cutoff: &str,
    ) -> lattice_core::shared::error::Result<
        Vec<lattice_core::application::contracts::references::PassageReferenceRecord>,
    > {
        crate::features::references::repository::PassageReferenceRepository::new(
            self.db_pool().clone(),
        )
        .list_created_since(cutoff)
        .await
    }

    async fn notes_in_range(
        &self,
        start_date: &str,
        end_date: &str,
    ) -> lattice_core::shared::error::Result<
        Vec<crate::features::conversation::chat::ports::StoredJournalPage>,
    > {
        let rows = crate::features::daily_notes::repository::DailyNotesRepository::new(
            self.db_pool().clone(),
        )
        .list_in_date_range(start_date, end_date)
        .await?;
        // Field for field, so a column added to the row fails to compile here.
        Ok(rows
            .into_iter()
            .map(|row| {
                let crate::features::daily_notes::repository::WorkspaceNoteRecord {
                    revision,
                    id,
                    title,
                    journal_id,
                    content,
                    linked_document_ids,
                    linked_conversation_ids,
                    highlights_json,
                    sticky_notes_json,
                    conversation_snapshots_json,
                    sources_json,
                    created_at,
                    updated_at,
                } = row;
                crate::features::conversation::chat::ports::StoredJournalPage {
                    revision,
                    id,
                    title,
                    journal_id,
                    content,
                    linked_document_ids,
                    linked_conversation_ids,
                    highlights_json,
                    sticky_notes_json,
                    conversation_snapshots_json,
                    sources_json,
                    created_at,
                    updated_at,
                }
            })
            .collect())
    }

    async fn hydrate_note(
        &self,
        page: crate::features::conversation::chat::ports::StoredJournalPage,
    ) -> lattice_core::shared::error::Result<
        lattice_core::application::contracts::notes::WorkspaceNoteDto,
    > {
        let crate::features::conversation::chat::ports::StoredJournalPage {
            revision,
            id,
            title,
            journal_id,
            content,
            linked_document_ids,
            linked_conversation_ids,
            highlights_json,
            sticky_notes_json,
            conversation_snapshots_json,
            sources_json,
            created_at,
            updated_at,
        } = page;
        let row = crate::features::daily_notes::repository::WorkspaceNoteRecord {
            revision,
            id,
            title,
            journal_id,
            content,
            linked_document_ids,
            linked_conversation_ids,
            highlights_json,
            sticky_notes_json,
            conversation_snapshots_json,
            sources_json,
            created_at,
            updated_at,
        };
        crate::features::daily_notes::commands::hydrate_workspace_note(
            &crate::features::daily_notes::repository::DailyNotesRepository::new(
                self.db_pool().clone(),
            ),
            self,
            row,
        )
        .await
    }
}

/// The conversation archive a journal page recovers its web citations from,
/// read through the conversation repository. Daily notes is the record and
/// conversations are built on it, so the port is daily notes' and is bound
/// here, where both may be named.
#[async_trait::async_trait]
impl crate::features::daily_notes::ports::LinkedConversationArchive for Container {
    async fn archived_web_page(
        &self,
        conversation_id: &str,
        url: &str,
    ) -> Result<Option<crate::features::daily_notes::ports::ArchivedWebPage>> {
        let snapshot = crate::features::conversation::repository::ConversationRepository::new(
            self.db_pool().clone(),
        )
        .conversation_web_source_snapshot(conversation_id.to_string(), url.to_string())
        .await?;
        // Field for field, so a field added to the archive fails to compile here.
        Ok(snapshot.map(|snapshot| {
            let crate::features::conversation::workspace_dto::ConversationWebSourceSnapshotDto {
                title,
                content,
                fetched_at,
                truncated,
            } = snapshot;
            crate::features::daily_notes::ports::ArchivedWebPage {
                title,
                content,
                fetched_at,
                truncated,
            }
        }))
    }
}

/// The vault's markdown mirror as daily notes' projection. The vault is a
/// projection of the journal onto files, so it serves daily notes' port
/// rather than daily notes calling into the vault; the binding sits here
/// because the container owns the writer.
impl crate::features::daily_notes::ports::NoteProjection for Container {
    fn note_saved(&self) {
        crate::features::vault::writeback::spawn_sync_workspace_note(self.vault_writer());
    }

    fn note_deleted(&self) {
        crate::features::vault::writeback::spawn_delete_workspace_note(self.vault_writer());
    }
}

/// Explorer's folder threads, deleted through the chat's conversation delete.
/// Conversations are built on Explorer, so the port is Explorer's and is bound
/// here, where both may be named.
#[async_trait::async_trait]
impl crate::features::explorer::folders::FolderThreads for Container {
    async fn delete_thread(&self, conversation_id: &str) -> Result<()> {
        crate::features::conversation::chat::ports::ChatPolicy::delete_conversation(
            self,
            conversation_id,
        )
        .await
    }
}
