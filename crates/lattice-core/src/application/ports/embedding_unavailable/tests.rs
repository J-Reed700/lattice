use super::*;

#[tokio::test]
async fn a_port_with_no_model_installed_reports_models_not_installed() {
    let port = UnavailableEmbedding::not_installed();

    assert!(matches!(
        port.embed_single("test").await.unwrap_err(),
        AppError::AiModelsNotInstalled(_)
    ));
    assert!(matches!(
        port.embed_batch(&["a".to_string(), "b".to_string()])
            .await
            .unwrap_err(),
        AppError::AiModelsNotInstalled(_)
    ));
}

#[tokio::test]
async fn the_not_installed_error_says_where_to_download_a_model() {
    let error = UnavailableEmbedding::not_installed()
        .embed_single("test")
        .await
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        AppError::AiModelsNotInstalled(
            "No embedding model is installed. Download one in Settings → Models to enable search."
                .into()
        )
        .to_string()
    );
}

#[test]
fn only_a_port_whose_model_failed_to_load_reports_a_load_failure() {
    assert!(UnavailableEmbedding::not_installed()
        .load_failure()
        .is_none());
    let failed = UnavailableEmbedding::failed_to_load(AppError::ModelLoadFailed("boom".into()));
    assert_eq!(failed.load_failure().unwrap().to_string(), "boom");
}

#[tokio::test]
async fn a_port_whose_model_failed_to_load_returns_the_loaders_own_error() {
    let port = UnavailableEmbedding::failed_to_load(AppError::ModelLoadFailed(
        "server exited with code 3".into(),
    ));

    let single = port.embed_single("test").await.unwrap_err();
    let batch = port.embed_batch(&["a".to_string()]).await.unwrap_err();

    for error in [single, batch] {
        assert!(matches!(error, AppError::ModelLoadFailed(_)));
        assert_eq!(error.to_string(), "server exited with code 3");
    }
}

#[tokio::test]
async fn an_unavailable_port_is_never_ready() {
    assert!(!UnavailableEmbedding::not_installed()
        .is_ready()
        .await
        .unwrap());
    assert!(
        !UnavailableEmbedding::failed_to_load(AppError::ModelLoadFailed("x".into()))
            .is_ready()
            .await
            .unwrap()
    );
}

#[test]
fn an_unavailable_port_reports_the_default_dimension() {
    assert_eq!(
        UnavailableEmbedding::not_installed().dimension(),
        DEFAULT_EMBEDDING_DIM
    );
}
