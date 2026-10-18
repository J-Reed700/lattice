use super::*;
use crate::features::conversation::chat::test_runtime::{FakeChatImages, FakeRuntime};

fn runtime() -> (FakeRuntime, std::sync::Arc<FakeChatImages>) {
    let images = FakeChatImages::staged(Vec::new());
    let mut runtime = FakeRuntime::default();
    runtime.images = images.clone();
    (runtime, images)
}

fn write(dir: &tempfile::TempDir, name: &str) -> String {
    let path = dir.path().join(name);
    std::fs::write(&path, b"picture bytes").unwrap();
    path.to_string_lossy().into_owned()
}

/// One call stages every file, in order, under one draft.
#[tokio::test]
async fn files_are_staged_in_order_under_one_draft() {
    let (runtime, images) = runtime();
    let dir = tempfile::tempdir().unwrap();
    let paths = vec![write(&dir, "chart.png"), write(&dir, "photo.jpg")];

    let staged = attach_message_images_impl(&runtime, "conv".into(), paths)
        .await
        .unwrap();

    assert_eq!(staged.len(), 2);
    assert_eq!(
        images.calls(),
        vec!["store_attachment chart.png", "store_attachment photo.jpg"]
    );
    // The fake writes the draft it was given into the caption.
    let draft = |picture: &ChatImageDto| picture.caption.split(' ').nth(1).unwrap().to_string();
    assert!(draft(&staged[0]).starts_with(DRAFT_REQUEST_PREFIX));
    assert_eq!(draft(&staged[0]), draft(&staged[1]));
}

/// A path that is not a regular file fails the call and releases what the
/// call had already staged.
#[tokio::test]
async fn a_folder_is_refused_and_the_partial_stage_released() {
    let (runtime, images) = runtime();
    let dir = tempfile::tempdir().unwrap();
    let paths = vec![
        write(&dir, "chart.png"),
        dir.path().to_string_lossy().into_owned(),
    ];

    let error = attach_message_images_impl(&runtime, "conv".into(), paths)
        .await
        .unwrap_err();

    assert!(error.to_string().contains("is not a file"), "{error}");
    let calls = images.calls();
    assert_eq!(calls[0], "store_attachment chart.png");
    assert!(calls[1].starts_with("release_images "), "{calls:?}");
}

/// A file over the byte bound is refused from its size, before it is read.
#[tokio::test]
async fn a_file_over_the_byte_bound_is_refused_before_reading() {
    let (runtime, images) = runtime();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("huge.png");
    std::fs::File::create(&path)
        .unwrap()
        .set_len(MAX_IMAGE_BYTES as u64 + 1)
        .unwrap();

    let error = attach_message_images_impl(
        &runtime,
        "conv".into(),
        vec![path.to_string_lossy().into_owned()],
    )
    .await
    .unwrap_err();

    assert!(error.to_string().contains("too large"), "{error}");
    assert!(images.calls().is_empty());
}

#[tokio::test]
async fn staging_needs_a_conversation() {
    let (runtime, _) = runtime();
    assert!(attach_message_images_impl(&runtime, " ".into(), Vec::new())
        .await
        .is_err());
}

#[tokio::test]
async fn release_and_restage_reach_the_picture_store() {
    let picture = crate::features::conversation::chat::test_runtime::test_attachment(1, 64, 64);
    let images = FakeChatImages::staged(vec![picture.clone()]);
    let mut runtime = FakeRuntime::default();
    runtime.images = images.clone();

    let copy = restage_message_image_impl(&runtime, "conv".into(), picture.id.clone())
        .await
        .unwrap();
    assert_ne!(copy.id, picture.id);
    release_message_images_impl(&runtime, "conv".into(), vec![copy.id.clone()])
        .await
        .unwrap();
    release_message_images_impl(&runtime, "conv".into(), Vec::new())
        .await
        .unwrap();

    assert_eq!(
        images.calls(),
        vec![
            format!("restage {}", picture.id),
            format!("release_images {}", copy.id)
        ]
    );
}
