//! Every feature that writes a document's tags holds the container's one tag service.
use super::Container;
use crate::infrastructure::persistence::database::{initialize_database, DatabaseConnection};
use std::sync::Arc;

#[tokio::test]
async fn the_container_hands_one_tag_service_to_tags_and_file_metadata() -> anyhow::Result<()> {
    let data = tempfile::tempdir()?;
    let database = Arc::new(DatabaseConnection::new(data.path().join("app.db")).await?);
    initialize_database(database.database()).await?;
    let container = Container::new(
        database.database().clone(),
        database,
        None,
        "http://127.0.0.1:1",
        "test-model",
        data.path().to_path_buf(),
        Arc::default(),
    )
    .await?;

    let apply = container.apply_tags_use_case();
    let file_metadata = container.update_file_metadata_use_case();

    assert!(Arc::ptr_eq(
        apply.tag_service(),
        file_metadata.tag_service()
    ));
    Ok(())
}
