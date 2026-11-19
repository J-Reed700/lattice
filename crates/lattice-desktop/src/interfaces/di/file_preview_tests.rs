//! Imported files must remain readable through the same command as the viewer.
use super::Container;
use crate::features::file::commands::read_file_bytes_impl;
use crate::infrastructure::persistence::database::{initialize_database, DatabaseConnection};
use crate::infrastructure::storage::ContentAddressedStorage;
use lattice_core::shared::testing::links;
use std::path::Path;
use std::sync::Arc;

#[tokio::test]
async fn library_preview_survives_policy_refresh_without_allowing_siblings() -> anyhow::Result<()> {
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

    let link = imported.path().join("escape.pdf");
    if links::link_file(&private_file, &link)? {
        assert!(
            read_file_bytes_impl(&container, link.to_string_lossy().into_owned())
                .await
                .is_err()
        );
    }
    Ok(())
}

async fn readable(container: &Container, path: &Path) -> bool {
    let allowed = container.file_access_config().validate_path(path).is_ok();
    let read = read_file_bytes_impl(container, path.to_string_lossy().into_owned()).await;
    assert_eq!(
        read.is_ok(),
        allowed,
        "the viewer's command reads exactly what the access policy allows: {read:?}"
    );
    allowed
}

#[tokio::test]
async fn a_saved_web_article_is_readable_through_the_viewers_command() -> anyhow::Result<()> {
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
    let archive = lattice_core::shared::fs::roots::web_archive_root()?;
    tokio::fs::create_dir_all(&archive).await?;
    let article = tempfile::tempdir_in(&archive)?;
    let markdown = article.path().join("article.md");
    let page = article.path().join("page.html");
    tokio::fs::write(&markdown, "# A saved article").await?;
    tokio::fs::write(&page, "<html><body>A saved article</body></html>").await?;

    assert!(
        readable(&container, &markdown).await,
        "a saved web article is readable from startup"
    );
    assert!(
        readable(&container, &page).await,
        "a saved web page is readable from startup"
    );

    container.refresh_allowed_roots().await?;
    assert!(
        readable(&container, &markdown).await,
        "a saved web article stays readable after the readable folders are recomputed"
    );
    assert!(
        readable(&container, &page).await,
        "a saved web page stays readable after the readable folders are recomputed"
    );
    Ok(())
}
