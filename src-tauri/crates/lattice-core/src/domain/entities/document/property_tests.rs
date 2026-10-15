//! Property-based tests for the document entity.

#![cfg(test)]

use super::*;
use proptest::prelude::*;
use std::io::Write;
use std::path::PathBuf;
use tempfile::NamedTempFile;

fn temp_path(file_name: &str) -> PathBuf {
    std::env::temp_dir().join(file_name)
}

fn arbitrary_document_status() -> impl Strategy<Value = DocumentStatus> {
    prop_oneof![
        Just(DocumentStatus::Pending),
        Just(DocumentStatus::Processing),
        Just(DocumentStatus::Indexed),
        Just(DocumentStatus::Failed),
    ]
}

fn arbitrary_language() -> impl Strategy<Value = Language> {
    prop_oneof![
        Just(Language::English),
        Just(Language::Spanish),
        Just(Language::French),
        Just(Language::German),
        Just(Language::Chinese),
        Just(Language::Japanese),
        Just(Language::Python),
        Just(Language::Rust),
        Just(Language::JavaScript),
        Just(Language::TypeScript),
        Just(Language::Go),
        Just(Language::Java),
        Just(Language::Cpp),
        Just(Language::CSharp),
        Just(Language::Unknown),
    ]
}

fn arbitrary_category() -> impl Strategy<Value = Category> {
    prop_oneof![
        Just(Category::Code),
        Just(Category::Documentation),
        Just(Category::Research),
        Just(Category::Notes),
        Just(Category::Reference),
        Just(Category::Tutorial),
        Just(Category::Uncategorized),
    ]
}

fn valid_mime_type() -> impl Strategy<Value = String> {
    prop::sample::select(vec![
        "text/plain".to_string(),
        "text/markdown".to_string(),
        "application/pdf".to_string(),
        "application/json".to_string(),
        "text/html".to_string(),
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document".to_string(),
    ])
}

fn valid_file_content() -> impl Strategy<Value = String> {
    prop::string::string_regex("[a-zA-Z0-9 \\n\\r\\t.,;:!?()\\[\\]{}\"']{10,1000}").unwrap()
}

fn non_empty_string() -> impl Strategy<Value = String> {
    prop::string::string_regex("[a-zA-Z0-9_\\-\\.]{1,100}").unwrap()
}

fn arbitrary_document() -> impl Strategy<Value = Document> {
    (
        arbitrary_document_status(),
        valid_mime_type(),
        valid_file_content(),
        non_empty_string(),
    )
        .prop_map(|(status, mime, content, filename)| {
            let mut temp = NamedTempFile::new().unwrap();
            temp.write_all(content.as_bytes()).unwrap();
            let path = temp.path().to_path_buf();

            let validated_path = ValidatedFilePath::new(path.clone())
                .unwrap_or_else(|_| ValidatedFilePath::new(temp_path("test.txt")).unwrap());

            let checksum = Checksum::new("0".repeat(64)).expect("valid checksum");
            let size_bytes = content.len() as i64;

            let mut doc = Document::new(
                validated_path,
                filename.clone(),
                mime.clone(),
                size_bytes,
                checksum,
            );
            doc.status = status;
            doc
        })
}

proptest! {
    #[test]
    fn prop_document_has_unique_id(_dummy in 0..10u32) {
        let path1 = ValidatedFilePath::new(temp_path("test1.txt")).unwrap();
        let path2 = ValidatedFilePath::new(temp_path("test2.txt")).unwrap();

        let doc1 = Document::new(
            path1,
            "test1.txt".to_string(),
            "text/plain".to_string(),
            100,
            Checksum::new("1".repeat(64)).expect("valid checksum"),
        );
        let doc2 = Document::new(
            path2,
            "test2.txt".to_string(),
            "text/plain".to_string(),
            100,
            Checksum::new("2".repeat(64)).expect("valid checksum"),
        );

        prop_assert_ne!(doc1.id(), doc2.id());
    }

    #[test]
    fn prop_document_id_stable_across_clones(doc in arbitrary_document()) {
        let cloned = doc.clone();
        prop_assert_eq!(doc.id(), cloned.id());
    }

    #[test]
    fn prop_new_document_starts_pending(_dummy in 0..10u32) {
        let path = ValidatedFilePath::new(temp_path("test.txt")).unwrap();
        let doc = Document::new(
            path,
            "test.txt".to_string(),
            "text/plain".to_string(),
            100,
            Checksum::new("a".repeat(64)).expect("valid checksum"),
        );

        prop_assert_eq!(doc.status(), DocumentStatus::Pending);
    }

    #[test]
    fn prop_document_file_name_non_empty(doc in arbitrary_document()) {
        prop_assert!(!doc.file_name().is_empty());
    }

    #[test]
    fn prop_document_creation_time_before_update(doc in arbitrary_document()) {
        prop_assert!(doc.modified_at() <= doc.indexed_at());
    }
}

proptest! {
    #[test]
    fn prop_document_status_transition_to_indexed(mut doc in arbitrary_document()) {
        doc.mark_indexed();
        prop_assert_eq!(doc.status(), DocumentStatus::Indexed);
    }

    #[test]
    fn prop_document_status_transition_to_failed(mut doc in arbitrary_document()) {
        doc.mark_failed();
        prop_assert_eq!(doc.status(), DocumentStatus::Failed);
    }

    #[test]
    fn prop_document_status_updates_timestamp(mut doc in arbitrary_document()) {
        let before = *doc.indexed_at();
        std::thread::sleep(std::time::Duration::from_millis(10));
        doc.mark_indexed();
        prop_assert!(doc.indexed_at() >= &before);
    }

    #[test]
    fn prop_is_indexed_true_only_when_indexed(status in arbitrary_document_status()) {
        let path = ValidatedFilePath::new(temp_path("test.txt")).unwrap();
        let mut doc = Document::new(
            path,
            "test.txt".to_string(),
            "text/plain".to_string(),
            100,
            Checksum::new("a".repeat(64)).expect("valid checksum"),
        );
        doc.status = status;

        if status == DocumentStatus::Indexed {
            prop_assert!(doc.is_indexed());
        } else {
            prop_assert!(!doc.is_indexed());
        }
    }

    #[test]
    fn prop_status_serialization_roundtrip(status in arbitrary_document_status()) {
        let json = serde_json::to_string(&status).unwrap();
        let deserialized: DocumentStatus = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(status, deserialized);
    }

    #[test]
    fn prop_status_can_transition_from_any_to_any(
        from_status in arbitrary_document_status(),
        to_status in arbitrary_document_status()
    ) {
        let path = ValidatedFilePath::new(temp_path("test.txt")).unwrap();
        let mut doc = Document::new(
            path,
            "test.txt".to_string(),
            "text/plain".to_string(),
            100,
            Checksum::new("a".repeat(64)).expect("valid checksum"),
        );
        doc.status = from_status;

        match to_status {
            DocumentStatus::Indexed => doc.mark_indexed(),
            DocumentStatus::Failed => doc.mark_failed(),
            DocumentStatus::Pending => doc.mark_pending(),
            DocumentStatus::Processing => doc.mark_processing(),
        }

        prop_assert_eq!(doc.status(), to_status);
    }
}

proptest! {
    #[test]
    fn prop_document_checksum_stable(doc in arbitrary_document()) {
        let checksum1 = doc.checksum().clone();
        let checksum2 = doc.checksum().clone();
        prop_assert_eq!(checksum1, checksum2);
    }

    #[test]
    fn prop_different_content_different_checksum(_dummy in 0..10u32) {
        let path1 = ValidatedFilePath::new(temp_path("test1.txt")).unwrap();
        let path2 = ValidatedFilePath::new(temp_path("test2.txt")).unwrap();

        let doc1 = Document::new(
            path1,
            "test.txt".to_string(),
            "text/plain".to_string(),
            100,
            Checksum::new("c1".repeat(32)).expect("valid checksum"),
        );
        let doc2 = Document::new(
            path2,
            "test.txt".to_string(),
            "text/plain".to_string(),
            100,
            Checksum::new("c2".repeat(32)).expect("valid checksum"),
        );

        prop_assert_ne!(doc1.checksum(), doc2.checksum());
    }

    #[test]
    fn prop_size_bytes_accessible(doc in arbitrary_document()) {
        prop_assert!(doc.size_bytes() >= 0);
    }

    #[test]
    fn prop_checksum_update_marks_pending(mut doc in arbitrary_document()) {
        let new_checksum = Checksum::new("d".repeat(64)).expect("valid checksum");
        doc.update_checksum(new_checksum);
        prop_assert_eq!(doc.status(), DocumentStatus::Pending);
    }
}

proptest! {
    #[test]
    fn prop_document_language_can_be_set(mut doc in arbitrary_document(), lang in arbitrary_language()) {
        doc.set_language(lang.clone());
        prop_assert_eq!(doc.language(), &lang);
    }

    #[test]
    fn prop_document_category_can_be_set(mut doc in arbitrary_document(), cat in arbitrary_category()) {
        doc.set_category(cat.clone());
        prop_assert_eq!(doc.category(), &cat);
    }

    #[test]
    fn prop_quality_score_clamped(mut doc in arbitrary_document(), score in -10.0f32..10.0f32) {
        doc.set_quality_score(score);
        let result = doc.quality_score();
        prop_assert!((0.0..=1.0).contains(&result));
    }

    #[test]
    fn prop_access_count_increments(mut doc in arbitrary_document()) {
        let before = doc.access_count();
        doc.increment_access();
        prop_assert_eq!(doc.access_count(), before + 1);
    }

    #[test]
    fn prop_word_count_non_negative(mut doc in arbitrary_document(), count in -100i32..1000i32) {
        doc.set_word_count(count);
        prop_assert!(doc.word_count() >= 0);
    }
}

proptest! {
    #[test]
    fn prop_mime_type_preserved(mime in valid_mime_type()) {
        let path = ValidatedFilePath::new(temp_path("test.txt")).unwrap();
        let doc = Document::new(
            path,
            "test.txt".to_string(),
            mime.clone(),
            100,
            Checksum::new("a".repeat(64)).expect("valid checksum"),
        );

        prop_assert_eq!(doc.mime_type(), &mime);
    }

    #[test]
    fn prop_is_text_detects_text_mime(_dummy in 0..10u32) {
        let path = ValidatedFilePath::new(temp_path("test.txt")).unwrap();
        let doc = Document::new(
            path,
            "test.txt".to_string(),
            "text/plain".to_string(),
            100,
            Checksum::new("a".repeat(64)).expect("valid checksum"),
        );

        prop_assert!(doc.is_text());
    }

    #[test]
    fn prop_is_pdf_detects_pdf(_dummy in 0..10u32) {
        let path = ValidatedFilePath::new(temp_path("test.pdf")).unwrap();
        let doc = Document::new(
            path,
            "test.pdf".to_string(),
            "application/pdf".to_string(),
            100,
            Checksum::new("a".repeat(64)).expect("valid checksum"),
        );

        prop_assert!(doc.is_pdf());
    }

    #[test]
    fn prop_size_human_readable_format(doc in arbitrary_document()) {
        let size_str = doc.size_human_readable();
        prop_assert!(size_str.contains("B") || size_str.contains("KB") || size_str.contains("MB"));
    }
}

proptest! {
    #[test]
    fn prop_document_serialization_roundtrip(doc in arbitrary_document()) {
        let json = serde_json::to_string(&doc).unwrap();
        let deserialized: Document = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(doc.id(), deserialized.id());
        prop_assert_eq!(doc.status(), deserialized.status());
        prop_assert_eq!(doc.checksum(), deserialized.checksum());
    }

    #[test]
    fn prop_document_debug_includes_id(doc in arbitrary_document()) {
        let debug_str = format!("{:?}", doc);
        let id_str = doc.id().to_string();
        prop_assert!(debug_str.contains(&id_str));
    }

    #[test]
    fn prop_document_clone_creates_equal(doc in arbitrary_document()) {
        let cloned = doc.clone();
        prop_assert_eq!(doc.id(), cloned.id());
        prop_assert_eq!(doc.status(), cloned.status());
        prop_assert_eq!(doc.checksum(), cloned.checksum());
        prop_assert_eq!(doc.file_name(), cloned.file_name());
    }
}
