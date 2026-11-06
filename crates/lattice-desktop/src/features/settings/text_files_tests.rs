//! Saving and opening with a stand-in for the platform's dialogs, which picks a
//! path or cancels, and a temporary directory for the files.
use super::*;
use lattice_core::shared::ipc::ApiError;
use std::sync::Mutex;

/// What a dialog was asked: the suggested name (a save dialog only) and the
/// filters.
#[derive(Debug, PartialEq)]
struct Asked {
    suggested_name: Option<String>,
    filters: Vec<FileFilterDto>,
}

/// Answers every dialog with a chosen path, or with `None` as a cancel does,
/// and records what it was asked.
struct Chosen {
    path: Option<PathBuf>,
    asked: Mutex<Vec<Asked>>,
}

impl Chosen {
    fn at(path: &Path) -> Self {
        Self {
            path: Some(path.to_path_buf()),
            asked: Mutex::new(Vec::new()),
        }
    }

    fn cancelled() -> Self {
        Self {
            path: None,
            asked: Mutex::new(Vec::new()),
        }
    }

    fn asked(&self) -> Vec<Asked> {
        std::mem::take(&mut *self.asked.lock().unwrap())
    }
}

#[async_trait]
impl TextFileDialogs for Chosen {
    async fn save_path(
        &self,
        suggested_name: &str,
        filters: &[FileFilterDto],
    ) -> Result<Option<PathBuf>> {
        self.asked.lock().unwrap().push(Asked {
            suggested_name: Some(suggested_name.to_string()),
            filters: filters.to_vec(),
        });
        Ok(self.path.clone())
    }

    async fn open_path(&self, filters: &[FileFilterDto]) -> Result<Option<PathBuf>> {
        self.asked.lock().unwrap().push(Asked {
            suggested_name: None,
            filters: filters.to_vec(),
        });
        Ok(self.path.clone())
    }
}

fn json() -> Vec<FileFilterDto> {
    vec![FileFilterDto {
        name: "JSON".into(),
        extensions: vec!["json".into()],
    }]
}

#[tokio::test]
async fn saving_writes_what_was_given_to_the_path_the_user_chose() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("chosen.json");

    let saved = save(&Chosen::at(&target), "suggested.json", &json(), "{\"a\":1}")
        .await
        .unwrap();

    assert!(saved);
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "{\"a\":1}");
    assert!(
        !dir.path().join("suggested.json").exists(),
        "the suggested name is an offer, not a path"
    );
}

#[tokio::test]
async fn cancelling_the_save_dialog_writes_nothing_and_returns_false() {
    let suggested = "cancelled-save-must-not-be-written.json";

    let saved = save(&Chosen::cancelled(), suggested, &json(), "{}")
        .await
        .unwrap();

    assert!(!saved);
    assert!(
        !Path::new(suggested).exists(),
        "a cancel does not fall back to writing the suggested name"
    );
}

#[tokio::test]
async fn the_save_dialog_is_offered_the_suggested_name_and_the_filters() {
    let dir = tempfile::tempdir().unwrap();
    let dialogs = Chosen::at(&dir.path().join("out.json"));

    save(&dialogs, "lattice-custom-tools.json", &json(), "[]")
        .await
        .unwrap();

    assert_eq!(
        dialogs.asked(),
        vec![Asked {
            suggested_name: Some("lattice-custom-tools.json".into()),
            filters: json(),
        }]
    );
}

#[tokio::test]
async fn a_save_that_cannot_write_fails_naming_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("gone").join("chosen.json");

    let error = save(&Chosen::at(&target), "x.json", &json(), "{}")
        .await
        .unwrap_err();

    let AppError::Io { message, kind } = &error else {
        panic!("expected an I/O error, got {error:?}");
    };
    assert!(message.contains("chosen.json"), "{message}");
    assert_eq!(kind, &ErrorKind::NotFound.to_string());
    assert!(ApiError::from(error)
        .details
        .is_some_and(|details| details.contains("chosen.json")));
}

#[tokio::test]
async fn opening_reads_back_what_saving_wrote() {
    let dir = tempfile::tempdir().unwrap();
    let dialogs = Chosen::at(&dir.path().join("round-trip.json"));
    let contents = "{\"name\": \"café — ünïcode ✓\"}\r\n";

    save(&dialogs, "x.json", &json(), contents).await.unwrap();
    let opened = open(&dialogs, &json()).await.unwrap();

    assert_eq!(opened.as_deref(), Some(contents));
}

#[tokio::test]
async fn cancelling_the_open_dialog_returns_none() {
    let opened = open(&Chosen::cancelled(), &json()).await.unwrap();

    assert_eq!(opened, None);
}

#[tokio::test]
async fn the_open_dialog_is_offered_the_filters() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("in.json");
    std::fs::write(&source, "[]").unwrap();
    let dialogs = Chosen::at(&source);

    open(&dialogs, &json()).await.unwrap();

    assert_eq!(
        dialogs.asked(),
        vec![Asked {
            suggested_name: None,
            filters: json(),
        }]
    );
}

#[tokio::test]
async fn opening_a_missing_file_is_an_error_naming_it() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing.json");

    let error = open(&Chosen::at(&missing), &json()).await.unwrap_err();

    let AppError::FileNotFound { path } = &error else {
        panic!("expected a missing-file error, got {error:?}");
    };
    assert!(path.contains("missing.json"), "{path}");
    assert!(ApiError::from(error).message.contains("missing.json"));
}

#[tokio::test]
async fn opening_a_file_that_is_not_utf8_is_an_error_with_the_reason() {
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("binary.json");
    std::fs::write(&binary, [0xC3u8, 0x28]).unwrap();

    let error = open(&Chosen::at(&binary), &json()).await.unwrap_err();

    let AppError::FileRead { path, reason } = &error else {
        panic!("expected a read error, got {error:?}");
    };
    assert!(path.contains("binary.json"), "{path}");
    assert!(reason.contains("UTF-8"), "{reason}");
    let shown = ApiError::from(error);
    assert!(shown.message.contains("binary.json"), "{shown:?}");
    assert!(shown
        .details
        .is_some_and(|details| details.contains("UTF-8")));
}

#[tokio::test]
async fn opening_drops_one_leading_byte_order_mark() {
    let dir = tempfile::tempdir().unwrap();
    let notepad = dir.path().join("notepad.json");
    std::fs::write(&notepad, "\u{feff}{\"a\":1}").unwrap();

    let opened = open(&Chosen::at(&notepad), &json()).await.unwrap();

    assert_eq!(opened.as_deref(), Some("{\"a\":1}"));
}
