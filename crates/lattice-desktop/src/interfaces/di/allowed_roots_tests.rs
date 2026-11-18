//! A folder added or removed in Settings must become readable or unreadable at once.
use super::Container;
use crate::features::file::commands::read_file_bytes_impl;
use crate::features::settings::dto::SettingsCategory;
use crate::infrastructure::persistence::database::{initialize_database, DatabaseConnection};
use std::collections::HashMap;
use std::sync::Arc;

fn indexed_paths(paths: &[&str]) -> HashMap<String, serde_json::Value> {
    HashMap::from([("indexedPaths".to_string(), serde_json::json!(paths))])
}

async fn readable(container: &Container, path: &str) -> bool {
    let allowed = container.file_access_config().validate_path(path).is_ok();
    let read = read_file_bytes_impl(container, path.to_owned()).await;
    assert_eq!(
        read.is_ok(),
        allowed,
        "the viewer's command reads exactly what the access policy allows: {read:?}"
    );
    allowed
}

#[tokio::test]
async fn a_watched_folder_is_readable_once_added_and_not_once_removed_or_reset(
) -> anyhow::Result<()> {
    let data = tempfile::tempdir()?;
    let database = DatabaseConnection::new(data.path().join("app.db")).await?;
    initialize_database(database.database()).await?;
    let container = Container::new(
        database.database().clone(),
        None,
        "http://127.0.0.1:1",
        "test-model",
        data.path().to_path_buf(),
        Arc::default(),
    )
    .await?;
    container.refresh_allowed_roots().await?;

    let folder = tempfile::tempdir()?;
    let file = folder.path().join("notes.txt");
    tokio::fs::write(&file, b"a file in a watched folder").await?;
    let path = file.to_string_lossy().into_owned();
    let folder_path = folder.path().to_string_lossy().into_owned();
    let settings = container.update_settings_use_case();

    assert!(
        !readable(&container, &path).await,
        "a folder no setting names is unreadable"
    );

    settings
        .update_category(SettingsCategory::Indexing, indexed_paths(&[&folder_path]))
        .await?;
    assert!(
        readable(&container, &path).await,
        "a folder added in Settings is readable at once"
    );

    settings
        .update_category(SettingsCategory::Indexing, indexed_paths(&[]))
        .await?;
    assert!(
        !readable(&container, &path).await,
        "a folder removed in Settings is unreadable at once"
    );

    settings
        .update_category(SettingsCategory::Indexing, indexed_paths(&[&folder_path]))
        .await?;
    assert!(
        readable(&container, &path).await,
        "a folder added again is readable again"
    );

    container.reset_settings_use_case().reset_all().await?;
    assert!(
        !readable(&container, &path).await,
        "a folder dropped by a reset is unreadable at once"
    );
    Ok(())
}
