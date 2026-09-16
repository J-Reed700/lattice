//! Export conversations and journal pages as Markdown files or one JSON file.
//!
//! The output root is always `Container::exports_path()` — the caller does not
//! get to choose it. `plugin_export_*` is webview-callable, so an arbitrary
//! destination would make export an arbitrary-write primitive for the renderer
//! (the same reasoning that confines `create_backup` to the backups folder).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;

use crate::features::backup::export_repository::{
    ExportConversationRow, ExportMessageRow, ExportNoteRow, ExportRepository,
};
use crate::shared::error::AppError;

/// What an export actually wrote.
#[derive(Debug, Clone)]
pub struct ExportSummary {
    /// The timestamped directory the files landed in.
    pub output_dir: String,
    pub conversations: usize,
    pub journal_pages: usize,
}

impl ExportSummary {
    /// Conversations plus journal pages — the number the toast names.
    pub fn count(&self) -> usize {
        self.conversations + self.journal_pages
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Markdown,
    /// `pretty` is the caller's `plugin_export_json` flag. It has to reach the
    /// serialiser or the argument is a control that changes nothing.
    Json {
        pretty: bool,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonMessage {
    id: String,
    role: String,
    content: String,
    created_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonConversation {
    id: String,
    title: String,
    model_name: String,
    created_at: String,
    updated_at: String,
    messages: Vec<JsonMessage>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonNote {
    id: String,
    title: String,
    content: String,
    created_at: String,
    updated_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonExport {
    exported_at: String,
    app_version: String,
    conversations: Vec<JsonConversation>,
    journal_pages: Vec<JsonNote>,
}

/// Lowercase, non-alphanumerics collapsed to `-`, capped at 60 characters.
///
/// Always suffixed with the first 8 characters of the id by `export_file_name`,
/// so two identically titled notes cannot collide — and so a title like
/// `../../etc/passwd` can never contribute a path separator.
fn sanitize_title(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut last_was_dash = false;

    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            out.push('-');
            last_was_dash = true;
        }
    }

    let trimmed = out.trim_matches('-');
    let capped: String = trimmed.chars().take(60).collect();
    let capped = capped.trim_matches('-').to_string();

    if capped.is_empty() {
        "untitled".to_string()
    } else {
        capped
    }
}

fn short_id(id: &str) -> String {
    let short: String = id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(8)
        .collect();
    if short.is_empty() {
        "00000000".to_string()
    } else {
        short
    }
}

fn export_file_name(title: &str, id: &str) -> String {
    format!("{}-{}.md", sanitize_title(title), short_id(id))
}

fn role_label(role: &str) -> &'static str {
    match role {
        "user" => "You",
        "assistant" => "Lattice",
        _ => "System",
    }
}

fn date_only(timestamp: &str) -> &str {
    timestamp.split('T').next().unwrap_or(timestamp)
}

fn timestamp_slug() -> String {
    chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string()
}

pub struct ExportConversationsUseCase {
    repo: Arc<ExportRepository>,
}

impl ExportConversationsUseCase {
    pub fn new(repo: Arc<ExportRepository>) -> Self {
        Self { repo }
    }

    /// Writes into `output_root/lattice-export-<YYYYMMDD-HHMMSS>/`.
    pub async fn execute(
        &self,
        output_root: PathBuf,
        format: ExportFormat,
    ) -> Result<ExportSummary, AppError> {
        let conversations = self.repo.list_conversations().await?;
        let messages = self.repo.list_messages().await?;
        let notes = self.repo.list_journal_pages().await?;

        let output_dir = output_root.join(format!("lattice-export-{}", timestamp_slug()));
        // repository-barrier-allow: export writes its own output directory.
        tokio::fs::create_dir_all(&output_dir)
            .await
            .map_err(|e| AppError::Other(format!("Failed to create export directory: {}", e)))?;

        let mut by_conversation: BTreeMap<String, Vec<ExportMessageRow>> = BTreeMap::new();
        for message in messages {
            by_conversation
                .entry(message.conversation_id.clone())
                .or_default()
                .push(message);
        }

        match format {
            ExportFormat::Markdown => {
                write_markdown(&output_dir, &conversations, &by_conversation, &notes).await?;
            }
            ExportFormat::Json { pretty } => {
                write_json(
                    &output_dir,
                    &conversations,
                    &by_conversation,
                    &notes,
                    pretty,
                )
                .await?;
            }
        }

        Ok(ExportSummary {
            output_dir: output_dir.to_string_lossy().to_string(),
            conversations: conversations.len(),
            journal_pages: notes.len(),
        })
    }
}

async fn write_markdown(
    output_dir: &Path,
    conversations: &[ExportConversationRow],
    by_conversation: &BTreeMap<String, Vec<ExportMessageRow>>,
    notes: &[ExportNoteRow],
) -> Result<(), AppError> {
    let conversations_dir = output_dir.join("conversations");
    let journal_dir = output_dir.join("journal");

    if !conversations.is_empty() {
        // repository-barrier-allow: export writes its own output directory.
        tokio::fs::create_dir_all(&conversations_dir)
            .await
            .map_err(|e| AppError::Other(format!("Failed to create export directory: {}", e)))?;
    }
    if !notes.is_empty() {
        // repository-barrier-allow: export writes its own output directory.
        tokio::fs::create_dir_all(&journal_dir)
            .await
            .map_err(|e| AppError::Other(format!("Failed to create export directory: {}", e)))?;
    }

    for conversation in conversations {
        let messages = by_conversation
            .get(&conversation.id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);

        let mut body = format!(
            "# {}\n\n{} · {} · {} messages\n\n---\n",
            conversation.title,
            date_only(&conversation.created_at),
            conversation.model_name,
            messages.len()
        );

        for message in messages {
            body.push_str(&format!(
                "\n## {}\n\n{}\n",
                role_label(&message.role),
                message.content
            ));
        }

        let path = conversations_dir.join(export_file_name(&conversation.title, &conversation.id));
        // repository-barrier-allow: export writes the file it just named.
        tokio::fs::write(&path, body)
            .await
            .map_err(|e| AppError::Other(format!("Failed to write export file: {}", e)))?;
    }

    for note in notes {
        let body = format!(
            "# {}\n\n{} · updated {}\n\n---\n\n{}\n",
            note.title,
            date_only(&note.created_at),
            date_only(&note.updated_at),
            note.content
        );

        let path = journal_dir.join(export_file_name(&note.title, &note.id));
        // repository-barrier-allow: export writes the file it just named.
        tokio::fs::write(&path, body)
            .await
            .map_err(|e| AppError::Other(format!("Failed to write export file: {}", e)))?;
    }

    Ok(())
}

async fn write_json(
    output_dir: &Path,
    conversations: &[ExportConversationRow],
    by_conversation: &BTreeMap<String, Vec<ExportMessageRow>>,
    notes: &[ExportNoteRow],
    pretty: bool,
) -> Result<(), AppError> {
    let payload = JsonExport {
        exported_at: chrono::Utc::now().to_rfc3339(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        conversations: conversations
            .iter()
            .map(|conversation| JsonConversation {
                id: conversation.id.clone(),
                title: conversation.title.clone(),
                model_name: conversation.model_name.clone(),
                created_at: conversation.created_at.clone(),
                updated_at: conversation.updated_at.clone(),
                messages: by_conversation
                    .get(&conversation.id)
                    .map(|messages| {
                        messages
                            .iter()
                            .map(|message| JsonMessage {
                                id: message.id.clone(),
                                role: message.role.clone(),
                                content: message.content.clone(),
                                created_at: message.created_at.clone(),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            })
            .collect(),
        journal_pages: notes
            .iter()
            .map(|note| JsonNote {
                id: note.id.clone(),
                title: note.title.clone(),
                content: note.content.clone(),
                created_at: note.created_at.clone(),
                updated_at: note.updated_at.clone(),
            })
            .collect(),
    };

    let json = if pretty {
        serde_json::to_string_pretty(&payload)
    } else {
        serde_json::to_string(&payload)
    }
    .map_err(|e| AppError::Serialization(format!("Failed to serialise export: {}", e)))?;

    // repository-barrier-allow: export writes its own output file.
    tokio::fs::write(output_dir.join("lattice-export.json"), json)
        .await
        .map_err(|e| AppError::Other(format!("Failed to write export file: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::SqlitePool;
    use tempfile::tempdir;

    async fn seeded_pool(seed: bool) -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory sqlite");

        for statement in [
            "CREATE TABLE conversations (id TEXT PRIMARY KEY, title TEXT NOT NULL, model_name TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
            "CREATE TABLE conversation_messages (id TEXT PRIMARY KEY, conversation_id TEXT NOT NULL, role TEXT NOT NULL, content TEXT NOT NULL, created_at TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'completed')",
            "CREATE TABLE daily_notes_workspace (id TEXT PRIMARY KEY, title TEXT NOT NULL, content TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
        ] {
            sqlx::query(statement)
                .execute(&pool)
                .await
                .expect("create table");
        }

        if seed {
            sqlx::query(
                "INSERT INTO conversations (id, title, model_name, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
            )
            .bind("conv-11111111")
            .bind("Quarterly review")
            .bind("llama3.2")
            .bind("2026-09-01T10:00:00Z")
            .bind("2026-09-02T10:00:00Z")
            .execute(&pool)
            .await
            .expect("insert conversation");

            for (id, role, content, created) in [
                ("m1", "user", "What did we ship?", "2026-09-01T10:00:00Z"),
                ("m2", "assistant", "Three things.", "2026-09-01T10:01:00Z"),
            ] {
                sqlx::query(
                    "INSERT INTO conversation_messages (id, conversation_id, role, content, created_at, status) VALUES (?, ?, ?, ?, ?, 'completed')",
                )
                .bind(id)
                .bind("conv-11111111")
                .bind(role)
                .bind(content)
                .bind(created)
                .execute(&pool)
                .await
                .expect("insert message");
            }

            sqlx::query(
                "INSERT INTO daily_notes_workspace (id, title, content, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
            )
            .bind("note-22222222")
            .bind("Sep 2 page")
            .bind("- shipped the exporter\n")
            .bind("2026-09-02T09:00:00Z")
            .bind("2026-09-02T18:00:00Z")
            .execute(&pool)
            .await
            .expect("insert note");
        }

        pool
    }

    async fn use_case(seed: bool) -> ExportConversationsUseCase {
        ExportConversationsUseCase::new(Arc::new(ExportRepository::new(seeded_pool(seed).await)))
    }

    #[tokio::test]
    async fn markdown_export_writes_one_file_per_conversation_and_note() {
        let dir = tempdir().expect("tempdir");
        let summary = use_case(true)
            .await
            .execute(dir.path().to_path_buf(), ExportFormat::Markdown)
            .await
            .expect("export");

        assert_eq!(summary.conversations, 1);
        assert_eq!(summary.journal_pages, 1);
        assert_eq!(summary.count(), 2);

        let root = PathBuf::from(&summary.output_dir);
        let conversations: Vec<_> = std::fs::read_dir(root.join("conversations"))
            .expect("conversations dir")
            .filter_map(Result::ok)
            .collect();
        assert_eq!(conversations.len(), 1);

        let body = std::fs::read_to_string(conversations[0].path()).expect("read");
        assert!(body.starts_with("# Quarterly review\n"));
        assert!(body.contains("2026-09-01 · llama3.2 · 2 messages"));
        assert!(body.contains("## You\n\nWhat did we ship?"));
        assert!(body.contains("## Lattice\n\nThree things."));

        let notes: Vec<_> = std::fs::read_dir(root.join("journal"))
            .expect("journal dir")
            .filter_map(Result::ok)
            .collect();
        assert_eq!(notes.len(), 1);
    }

    #[test]
    fn markdown_export_sanitises_titles_and_suffixes_the_id() {
        assert_eq!(
            export_file_name("Quarterly Review — Q3!", "conv-11111111"),
            "quarterly-review-q3-conv1111.md"
        );
        assert_eq!(export_file_name("", "abc"), "untitled-abc.md");
        assert_eq!(export_file_name("   ", "abc"), "untitled-abc.md");
    }

    #[tokio::test]
    async fn markdown_export_never_writes_outside_the_output_directory() {
        let dir = tempdir().expect("tempdir");
        let pool = seeded_pool(false).await;
        sqlx::query(
            "INSERT INTO daily_notes_workspace (id, title, content, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("note-33333333")
        .bind("../../etc/passwd")
        .bind("nope")
        .bind("2026-09-02T09:00:00Z")
        .bind("2026-09-02T09:00:00Z")
        .execute(&pool)
        .await
        .expect("insert note");

        let summary = ExportConversationsUseCase::new(Arc::new(ExportRepository::new(pool)))
            .execute(dir.path().to_path_buf(), ExportFormat::Markdown)
            .await
            .expect("export");

        let root = PathBuf::from(&summary.output_dir);
        let entries: Vec<_> = std::fs::read_dir(root.join("journal"))
            .expect("journal dir")
            .filter_map(Result::ok)
            .collect();
        assert_eq!(entries.len(), 1);

        let written = entries[0].path();
        assert!(
            written.starts_with(&root),
            "export escaped its directory: {}",
            written.display()
        );
        assert_eq!(
            written.file_name().and_then(|n| n.to_str()),
            Some("etc-passwd-note3333.md")
        );
        assert!(!dir.path().parent().unwrap().join("etc").exists());
    }

    #[tokio::test]
    async fn json_export_nests_messages_under_their_conversation() {
        let dir = tempdir().expect("tempdir");
        let summary = use_case(true)
            .await
            .execute(
                dir.path().to_path_buf(),
                ExportFormat::Json { pretty: true },
            )
            .await
            .expect("export");

        let path = PathBuf::from(&summary.output_dir).join("lattice-export.json");
        let raw = std::fs::read_to_string(path).expect("read json");
        let value: serde_json::Value = serde_json::from_str(&raw).expect("parse json");

        let conversations = value["conversations"].as_array().expect("conversations");
        assert_eq!(conversations.len(), 1);
        let messages = conversations[0]["messages"].as_array().expect("messages");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(conversations[0]["modelName"], "llama3.2");
        assert_eq!(value["journalPages"].as_array().expect("notes").len(), 1);
    }

    #[tokio::test]
    async fn export_of_an_empty_vault_writes_the_directory_and_reports_zero() {
        let dir = tempdir().expect("tempdir");
        let summary = use_case(false)
            .await
            .execute(dir.path().to_path_buf(), ExportFormat::Markdown)
            .await
            .expect("export");

        assert_eq!(summary.count(), 0);
        assert!(PathBuf::from(&summary.output_dir).is_dir());
    }
}
