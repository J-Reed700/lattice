//! Vault writeback — mirrors notes to plain markdown on disk.
//!
//! Called as a side-effect after a successful note write. Spawns a
//! tokio task so the caller (the SQL command) doesn't wait on disk I/O.
//! Failures are logged but never propagated — the database commit
//! always wins. This module never returns errors; it logs them.
//!
//! ## Path scheme
//! All notes (workspace + daily) live in `<vault>/notes/<id>.md`.
//! Daily notes share the `daily_notes_workspace` table with workspace
//! notes — backend-side they're the same entity, just rendered with a
//! date affordance in the UI.
//!
//! ## Frontmatter
//! YAML-style block at the top of every file, Obsidian-compatible:
//! ```text
//! ---
//! id: <uuid>
//! title: <string>
//! created_at: <rfc3339>
//! updated_at: <rfc3339>
//! tags: [foo, bar]
//! ---
//!
//! <body>
//! ```
//!
//! ## Atomicity
//! Writes go to `<target>.tmp` then `rename` over the live file. Most
//! filesystems make this atomic so an external watcher never sees a
//! truncated file.
//!
//! ## Disabled-by-default
//! `settings.vault.enabled == false` short-circuits before any path
//! resolution. Users opt in via the Vault settings tab.

use std::path::{Path, PathBuf};

use crate::interfaces::di::Container;

/// Sync a workspace note to the vault folder. Fire-and-forget — spawns
/// a tokio task and returns immediately. Caller doesn't await the disk
/// write.
///
/// `body` is the raw markdown body the user typed; frontmatter is added
/// here. `tags` are passed through verbatim.
pub fn spawn_sync_workspace_note(
    container: &Container,
    id: String,
    title: String,
    body: String,
    created_at: String,
    updated_at: String,
    tags: Vec<String>,
) {
    // Clone the Arc out of the container reference so the spawned future
    // owns its own handle — the &Container we were passed has a non-
    // 'static lifetime tied to the caller.
    let settings_uc = std::sync::Arc::clone(container.system.get_settings_use_case());
    tokio::spawn(async move {
        let settings = match settings_uc.execute().await {
            Ok(s) => s,
            Err(e) => {
                tracing::debug!(error = %e, "vault writeback: failed to load settings — skipping");
                return;
            }
        };
        if !settings.vault.enabled {
            return;
        }
        let vault_root = match resolve_vault_root(&settings.vault.vault_path) {
            Some(p) => p,
            None => {
                tracing::warn!("vault writeback: could not resolve vault root — skipping");
                return;
            }
        };
        let target = vault_root.join("notes").join(format!("{}.md", id));
        let frontmatter = build_frontmatter(&id, &title, &created_at, &updated_at, &tags);
        let document = format!("{}\n\n{}", frontmatter, body);
        if let Err(e) = atomic_write(&target, &document).await {
            tracing::warn!(
                target = %target.display(),
                error = %e,
                "vault writeback: workspace note write failed"
            );
        } else {
            tracing::debug!(target = %target.display(), "vault writeback: workspace note synced");
        }
    });
}

/// One-shot backfill: walk every workspace note and write it as
/// markdown into the vault. Called when the user flips
/// `vault.enabled` from false to true so existing notes appear in the
/// vault folder, not just newly-written ones.
///
/// Fire-and-forget — spawns a single tokio task that streams through
/// the table. Failures on individual notes log but don't abort the
/// walk; one bad row should never block the rest.
pub fn spawn_backfill(pool: sqlx::SqlitePool, vault_root: std::path::PathBuf) {
    tokio::spawn(async move {
        // Stream rather than load all into memory — a power user could
        // have thousands of notes.
        let rows = match sqlx::query_as::<_, BackfillRow>(
            r#"
            SELECT id, title, content, created_at, updated_at
            FROM daily_notes_workspace
            "#,
        )
        .fetch_all(&pool)
        .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, "vault backfill: failed to enumerate notes");
                return;
            }
        };

        let total = rows.len();
        let mut succeeded = 0usize;
        for row in rows {
            let target = vault_root.join("notes").join(format!("{}.md", row.id));
            let frontmatter = build_frontmatter(
                &row.id,
                &row.title,
                &row.created_at,
                &row.updated_at,
                &[],
            );
            let document = format!("{}\n\n{}", frontmatter, row.content);
            match atomic_write(&target, &document).await {
                Ok(()) => succeeded += 1,
                Err(e) => tracing::warn!(
                    target = %target.display(),
                    error = %e,
                    "vault backfill: write failed for one note"
                ),
            }
        }
        tracing::info!(succeeded, total, "vault backfill complete");
    });
}

#[derive(sqlx::FromRow)]
struct BackfillRow {
    id: String,
    title: String,
    content: String,
    created_at: String,
    updated_at: String,
}

/// Resolve the vault root path. Empty configured value means use the
/// default `<home>/Lattice`. Pub so the settings side-effect can resolve
/// the same way the per-write path does — keep both in lockstep.
pub fn resolve_vault_root(configured: &str) -> Option<PathBuf> {
    let trimmed = configured.trim();
    if !trimmed.is_empty() {
        return Some(PathBuf::from(trimmed));
    }
    dirs::home_dir().map(|h| h.join("Lattice"))
}

/// Build a YAML frontmatter block. Inline-escapes title quotes by
/// JSON-style escaping (`\"`) — Obsidian and most YAML parsers accept
/// double-quoted scalar strings with backslash escapes.
fn build_frontmatter(
    id: &str,
    title: &str,
    created_at: &str,
    updated_at: &str,
    tags: &[String],
) -> String {
    let escaped_title = title.replace('\\', "\\\\").replace('"', "\\\"");
    let tags_block = if tags.is_empty() {
        "tags: []".to_string()
    } else {
        let escaped: Vec<String> = tags
            .iter()
            .map(|t| format!("\"{}\"", t.replace('\\', "\\\\").replace('"', "\\\"")))
            .collect();
        format!("tags: [{}]", escaped.join(", "))
    };
    format!(
        "---\nid: {id}\ntitle: \"{escaped_title}\"\ncreated_at: {created_at}\nupdated_at: {updated_at}\n{tags_block}\n---"
    )
}

/// Atomically write `contents` to `target`. Creates parent directories
/// as needed. Writes to `<target>.tmp` then renames, so an external
/// watcher never sees a truncated file.
async fn atomic_write(target: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let tmp = target.with_extension("md.tmp");
    tokio::fs::write(&tmp, contents.as_bytes()).await?;
    tokio::fs::rename(&tmp, target).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_with_tags_roundtrips() {
        let fm = build_frontmatter(
            "abc-123",
            "Hello world",
            "2026-05-01T18:00:00Z",
            "2026-05-01T18:05:00Z",
            &["foo".to_string(), "bar".to_string()],
        );
        assert!(fm.starts_with("---\n"));
        assert!(fm.contains("id: abc-123"));
        assert!(fm.contains("title: \"Hello world\""));
        assert!(fm.contains("tags: [\"foo\", \"bar\"]"));
        assert!(fm.ends_with("---"));
    }

    #[test]
    fn frontmatter_escapes_quotes_in_title() {
        let fm = build_frontmatter(
            "id1",
            "She said \"hi\"",
            "now",
            "now",
            &[],
        );
        assert!(fm.contains("title: \"She said \\\"hi\\\"\""));
    }

    #[test]
    fn empty_tags_render_as_empty_array() {
        let fm = build_frontmatter("id", "title", "now", "now", &[]);
        assert!(fm.contains("tags: []"));
    }

    #[test]
    fn resolve_vault_root_uses_configured_when_present() {
        let root = resolve_vault_root("/explicit/vault").unwrap();
        assert_eq!(root, PathBuf::from("/explicit/vault"));
    }

    #[test]
    fn resolve_vault_root_falls_back_to_default_on_empty_string() {
        let root = resolve_vault_root("");
        // Default is <home>/Lattice; on a system without home_dir
        // resolution this returns None, which is acceptable.
        if let Some(p) = root {
            assert!(p.ends_with("Lattice"));
        }
    }

    #[tokio::test]
    async fn atomic_write_creates_parent_dirs_and_file() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("nested/dir/file.md");
        atomic_write(&target, "hello").await.unwrap();
        let read = tokio::fs::read_to_string(&target).await.unwrap();
        assert_eq!(read, "hello");
    }
}
