use super::*;
use lattice_core::domain::models::downloaded::{DownloadedModel, ModelLocation};
use lattice_core::domain::models::metadata::ModelType;
use std::path::{Path, PathBuf};

/// The `models` table holding at most one row.
struct Rows(Option<DownloadedModel>);

#[async_trait::async_trait]
impl DownloadedModelRepository for Rows {
    async fn delete(&self, _model_id: &str) -> Result<()> {
        unreachable!("a check never deletes")
    }
    async fn find_by_model_id(&self, model_id: &str) -> Result<Option<DownloadedModel>> {
        Ok(self.0.clone().filter(|row| row.model_id() == model_id))
    }
    async fn list(&self) -> Result<Vec<DownloadedModel>> {
        unreachable!("a check reads one row")
    }
    async fn delete_by_model_id_if_not_active(
        &self,
        _model_id: &str,
        _active_model_id: &str,
    ) -> Result<bool> {
        unreachable!("a check never deletes")
    }
}

fn row(name: &str, path: &Path, model_type: ModelType) -> DownloadedModel {
    DownloadedModel::new_with_catalog(
        "row-1".into(),
        name.into(),
        "ornith-1.5-9b".into(),
        ModelLocation::LocalFile {
            path: path.to_path_buf(),
        },
        100,
        "qwen3".into(),
        None,
        |_| Some(model_type),
    )
    .unwrap()
}

fn gguf(path: &Path) -> DownloadedModel {
    row("Ornith-1.5-9B", path, ModelType::LanguageModel)
}

fn local() -> ModelRef {
    ModelRef::Local {
        model_id: "ornith-1.5-9b".into(),
    }
}

fn no_key(_: &str) -> Result<bool> {
    Ok(false)
}

async fn check(model: &ModelRef, rows: Rows, settings: &LLMSettingsDto) -> Result<()> {
    check_available(model, settings, &rows, no_key).await
}

fn message(error: AppError) -> String {
    match error {
        AppError::InvalidInput(message) | AppError::InvalidConfig(message) => message,
        other => panic!("unexpected error: {other:?}"),
    }
}

#[tokio::test]
async fn a_downloaded_chat_model_whose_file_is_on_disk_is_available() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let rows = Rows(Some(gguf(file.path())));
    check(&local(), rows, &LLMSettingsDto::default())
        .await
        .unwrap();
}

#[tokio::test]
async fn a_model_that_is_no_longer_downloaded_is_refused_with_the_fix() {
    let error = check(&local(), Rows(None), &LLMSettingsDto::default())
        .await
        .unwrap_err();
    assert!(matches!(error, AppError::InvalidInput(_)));
    assert_eq!(
        message(error),
        "ornith-1.5-9b is not downloaded any more. Download it in Settings → Models or pick another model"
    );
}

#[tokio::test]
async fn a_downloaded_model_whose_file_is_gone_names_where_it_was() {
    let gone = PathBuf::from("/nonexistent/lattice/ornith.gguf");
    let error = check(
        &local(),
        Rows(Some(gguf(&gone))),
        &LLMSettingsDto::default(),
    )
    .await
    .unwrap_err();
    assert_eq!(
        message(error),
        "Ornith-1.5-9B's file is missing from /nonexistent/lattice/ornith.gguf. Download it again in Settings → Models or pick another model"
    );
}

#[tokio::test]
async fn a_row_that_is_not_a_chat_model_is_refused() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let embedding = row("Embedder", file.path(), ModelType::TextEmbeddings);
    let error = check(&local(), Rows(Some(embedding)), &LLMSettingsDto::default())
        .await
        .unwrap_err();
    assert_eq!(
        message(error),
        "Embedder is not a chat model. Pick another model"
    );
}

#[tokio::test]
async fn a_server_model_needs_its_connection_and_nothing_more() {
    let ollama = ModelRef::Ollama {
        model: "qwen3:8b".into(),
    };
    let llama_cpp = ModelRef::LlamaCpp {
        model: "qwen3.5-27b".into(),
    };
    let mut settings = LLMSettingsDto::default();
    settings.ollama_url = "http://localhost:11434".into();
    settings.llama_cpp.url = "http://localhost:8080".into();
    check(&ollama, Rows(None), &settings).await.unwrap();
    check(&llama_cpp, Rows(None), &settings).await.unwrap();

    settings.ollama_url = " ".into();
    settings.llama_cpp.url = String::new();
    let ollama_error = check(&ollama, Rows(None), &settings).await.unwrap_err();
    assert!(matches!(ollama_error, AppError::InvalidConfig(_)));
    assert_eq!(
        message(ollama_error),
        "No Ollama connection is configured. Add one in Settings → Chat"
    );
    assert_eq!(
        message(check(&llama_cpp, Rows(None), &settings).await.unwrap_err()),
        "No llama.cpp connection is configured. Add one in Settings → Chat"
    );
}

#[tokio::test]
async fn a_cloud_model_needs_its_provider_key() {
    let model = ModelRef::Cloud {
        provider: CloudProvider::Openai,
        model: "gpt-x".into(),
    };
    let settings = LLMSettingsDto::default();
    let asked = std::sync::Mutex::new(Vec::new());
    check_available(&model, &settings, &Rows(None), |provider| {
        asked.lock().unwrap().push(provider.to_string());
        Ok(true)
    })
    .await
    .unwrap();
    assert_eq!(*asked.lock().unwrap(), ["openai"]);

    let error = check(&model, Rows(None), &settings).await.unwrap_err();
    assert!(matches!(error, AppError::InvalidConfig(_)));
    assert_eq!(
        message(error),
        "No OpenAI API key is saved. Add it in Settings → Chat"
    );
}
