//! Imported files must remain readable through the same command as the viewer.
use super::Container;
use crate::features::file::commands::read_file_bytes_impl;
use crate::infrastructure::persistence::database::{initialize_database, DatabaseConnection};
use crate::infrastructure::storage::ContentAddressedStorage;
use std::sync::Arc;

#[tokio::test]
async fn library_preview_survives_policy_refresh_without_allowing_siblings() -> anyhow::Result<()> {
    let data = tempfile::tempdir()?;
    let database = Arc::new(DatabaseConnection::new(data.path().join("app.db")).await?);
    initialize_database(database.pool()).await?;
    let container = Container::new(
        database.pool().clone(),
        database,
        None,
        "http://127.0.0.1:1",
        "test-model",
        data.path().to_path_buf(),
    )
    .await?;
    let root = ContentAddressedStorage::default_library_root()?;
    let imported = tempfile::tempdir_in(&root)?;
    let pdf = imported.path().join("imported.pdf");
    let bytes = b"%PDF-1.7\npreview bytes\n%%EOF";
    tokio::fs::write(&pdf, bytes).await?;
    let path = pdf.to_string_lossy().into_owned();
    assert_eq!(read_file_bytes_impl(&container, path.clone()).await?, bytes);

    let sibling = tempfile::tempdir_in(
        root.parent()
            .ok_or_else(|| anyhow::anyhow!("missing parent"))?,
    )?;
    let private_file = sibling.path().join("private.txt");
    tokio::fs::write(&private_file, "outside the library").await?;
    let private_path = private_file.to_string_lossy().into_owned();
    assert!(read_file_bytes_impl(&container, private_path.clone())
        .await
        .is_err());

    // Settings refresh must retain the library and remove obsolete grants.
    container
        .file_access_config()
        .add_allowed_root(sibling.path().to_path_buf())?;
    container.refresh_allowed_roots().await?;
    assert_eq!(read_file_bytes_impl(&container, path).await?, bytes);
    assert!(read_file_bytes_impl(&container, private_path)
        .await
        .is_err());

    #[cfg(unix)]
    {
        let link = imported.path().join("escape.pdf");
        std::os::unix::fs::symlink(&private_file, &link)?;
        assert!(
            read_file_bytes_impl(&container, link.to_string_lossy().into_owned())
                .await
                .is_err()
        );
    }
    Ok(())
}
