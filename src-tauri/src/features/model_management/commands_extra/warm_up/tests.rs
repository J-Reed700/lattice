use super::*;
use lattice_core::application::contracts::settings::SettingsCategory;
use std::collections::HashMap;

/// A container whose active chat model is a downloaded file no engine can
/// start here (there is no app to run a server), with Settings on local
/// models.
async fn container_with_a_chat_model_that_cannot_load(
    dir: &tempfile::TempDir,
) -> (Container, String) {
    let container = crate::tests::common::setup_test_container().await.unwrap();
    let path = dir.path().join("ornith.gguf");
    std::fs::write(&path, b"not a model").unwrap();
    let row = DownloadedModel::new(
        "row-ornith".into(),
        "Ornith-1.5-9B".into(),
        "ornith-1.5-9b".into(),
        ModelLocation::LocalFile { path },
        11,
        "GGUF".into(),
        None,
    )
    .unwrap();
    let repository = container.downloaded_model_repository();
    repository.save(&row).await.unwrap();
    repository
        .set_active_chat_model(row.model_id())
        .await
        .unwrap();
    container
        .update_settings_use_case()
        .update_category(
            SettingsCategory::Llm,
            HashMap::from([("provider".to_string(), serde_json::json!("local"))]),
        )
        .await
        .unwrap();
    (container, row.model_id().to_owned())
}

async fn settings_of(container: &Container) -> serde_json::Value {
    serde_json::to_value(container.get_settings_use_case().execute().await.unwrap()).unwrap()
}

async fn still_active_for_chat(container: &Container, model_id: &str) -> bool {
    container
        .downloaded_model_repository()
        .find_by_model_id(model_id)
        .await
        .unwrap()
        .expect("the row is kept")
        .is_active_for_chat()
}

#[tokio::test]
async fn a_chat_model_that_fails_to_warm_up_stays_selected_and_settings_are_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let (container, model_id) = container_with_a_chat_model_that_cannot_load(&dir).await;
    let before = settings_of(&container).await;

    let failed = warm_up_active_chat_model_impl(&container).await;

    assert!(matches!(failed, Err(AppError::ModelLoadFailed(message))
        if message.contains("Ornith-1.5-9B can't be loaded by Lattice's local engine")));
    assert!(still_active_for_chat(&container, &model_id).await);
    assert_eq!(settings_of(&container).await, before);
}

#[tokio::test]
async fn a_learning_model_that_fails_to_warm_up_changes_no_setting() {
    let dir = tempfile::tempdir().unwrap();
    let (container, model_id) = container_with_a_chat_model_that_cannot_load(&dir).await;
    let unset = warm_up_learning_model_impl(&container).await;
    assert!(matches!(unset, Err(AppError::InvalidConfig(message))
        if message == "No Learning model is set. Choose one in Settings → Models under Learning."));

    for learning_model in [
        serde_json::json!({"kind": "local", "model_id": model_id}),
        serde_json::json!({"kind": "local", "model_id": "deleted-model"}),
    ] {
        container
            .update_settings_use_case()
            .update_category(
                SettingsCategory::Llm,
                HashMap::from([("learningModel".to_string(), learning_model)]),
            )
            .await
            .unwrap();
        let before = settings_of(&container).await;

        assert!(warm_up_learning_model_impl(&container).await.is_err());

        assert_eq!(settings_of(&container).await, before);
        assert!(still_active_for_chat(&container, &model_id).await);
    }
}
