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
async fn a_port_whose_model_failed_to_load_names_the_failure() {
    let port = UnavailableEmbedding::failed_to_load("server exited with code 3");

    let single = port.embed_single("test").await.unwrap_err();
    let batch = port.embed_batch(&["a".to_string()]).await.unwrap_err();

    for error in [single, batch] {
        assert!(matches!(error, AppError::ModelLoadFailed(_)));
        assert!(error.to_string().contains("server exited with code 3"));
    }
}

#[tokio::test]
async fn an_unavailable_port_is_never_ready() {
    assert!(!UnavailableEmbedding::not_installed()
        .is_ready()
        .await
        .unwrap());
    assert!(!UnavailableEmbedding::failed_to_load("x")
        .is_ready()
        .await
        .unwrap());
}

#[test]
fn an_unavailable_port_reports_the_default_dimension() {
    assert_eq!(
        UnavailableEmbedding::not_installed().dimension(),
        DEFAULT_EMBEDDING_DIM
    );
}
