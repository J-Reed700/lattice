#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]

//! IPC command integration tests.
#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(unused_variables)]
#![allow(unused_imports)]

//!
//! Tests verify that IPC commands work correctly end-to-end,
//! simulating frontend calls to the Rust backend.
//!
//! ## Running Tests
//!
//! ```bash
//! cargo test --test '*' ipc_commands
//! ```

#[cfg(test)]
mod link_commands {
    use lattice::commands::links;
    use lattice::extraction::link_parser::DocumentInfo;

    #[tokio::test]
    async fn test_parse_wikilinks_command() {
        let content = "This has [[link one]] and [[link two|display]].".to_string();
        let source_path = "test.md".to_string();

        let result = links::parse_wikilinks(content, source_path).await;

        assert!(result.is_ok());
        let links = result.unwrap();

        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target, "link one");
        assert_eq!(links[1].target, "link two");
        assert_eq!(links[1].display_text, Some("display".to_string()));
    }

    #[tokio::test]
    async fn test_extract_wikilink_targets_command() {
        let content = "See [[note1]], [[note2]], and [[note3]]".to_string();

        let result = links::extract_wikilink_targets(content).await;

        assert!(result.is_ok());
        let targets = result.unwrap();

        assert_eq!(targets.len(), 3);
        assert_eq!(targets, vec!["note1", "note2", "note3"]);
    }

    #[tokio::test]
    async fn test_extract_document_title_command() {
        let content = "# Test Document Title\n\nContent here".to_string();

        let result = links::extract_document_title(content).await;

        assert!(result.is_ok());
        let title = result.unwrap();

        assert_eq!(title, Some("Test Document Title".to_string()));
    }

    #[tokio::test]
    async fn test_extract_title_fallback() {
        let content = "First line without header\nMore content".to_string();

        let result = links::extract_document_title(content).await;

        assert!(result.is_ok());
        let title = result.unwrap();

        assert_eq!(title, Some("First line without header".to_string()));
    }

    #[tokio::test]
    async fn test_resolve_wikilink_command() {
        let target = "test-note".to_string();
        let source_path = "index.md".to_string();
        let documents = vec![
            DocumentInfo {
                file_path: "notes/test-note.md".to_string(),
                title: Some("Test Note".to_string()),
            },
            DocumentInfo {
                file_path: "projects/project.md".to_string(),
                title: Some("Project".to_string()),
            },
        ];

        let result = links::resolve_wikilink(target, source_path, documents).await;

        assert!(result.is_ok());
        let resolved = result.unwrap();

        assert!(resolved.is_some());
        assert!(resolved.unwrap().contains("test-note"));
    }

    #[tokio::test]
    async fn test_resolve_no_match() {
        let target = "nonexistent".to_string();
        let source_path = "index.md".to_string();
        let documents = vec![
            DocumentInfo {
                file_path: "test.md".to_string(),
                title: None,
            },
        ];

        let result = links::resolve_wikilink(target, source_path, documents).await;

        assert!(result.is_ok());
        let resolved = result.unwrap();

        assert!(resolved.is_none());
    }

    #[tokio::test]
    async fn test_find_backlinks_command() {
        use lattice::commands::links::DocumentWithContent;

        let target_path = "notes/target.md".to_string();
        let all_documents = vec![
            DocumentWithContent {
                path: "doc1.md".to_string(),
                content: "This references [[target]]".to_string(),
            },
            DocumentWithContent {
                path: "doc2.md".to_string(),
                content: "This also has [[target#section]]".to_string(),
            },
            DocumentWithContent {
                path: "doc3.md".to_string(),
                content: "No links here".to_string(),
            },
        ];

        let result = links::find_backlinks(target_path, all_documents).await;

        assert!(result.is_ok());
        let backlinks = result.unwrap();

        assert_eq!(backlinks.len(), 2);
        assert_eq!(backlinks[0].source_path, "doc1.md");
        assert_eq!(backlinks[1].source_path, "doc2.md");
    }

    #[tokio::test]
    async fn test_empty_content_handling() {
        let result = links::parse_wikilinks("".to_string(), "test.md".to_string()).await;

        assert!(result.is_ok());
        let links = result.unwrap();

        assert_eq!(links.len(), 0);
    }

    #[tokio::test]
    async fn test_error_handling() {
        let result = links::extract_wikilink_targets("No links here".to_string()).await;

        assert!(result.is_ok());
        let targets = result.unwrap();

        assert_eq!(targets.len(), 0);
    }
}

// DISABLED: extraction_commands tests temporarily disabled due to TagGenerator dependency
// TODO: Re-enable once TagGenerator is re-implemented with local LLM (Ollama)
/*
#[cfg(test)]
mod extraction_commands {
    use lattice::commands::extraction;
    use lattice::extraction::link_parser::DocumentInfo;
    use lattice::extraction::tag_generator::DocumentMetadata;

    #[tokio::test]
    async fn test_parse_wikilinks_extraction_command() {
        let text = "Text with [[link]]".to_string();
        let source_path = Some("test.md".to_string());

        let result = extraction::parse_wikilinks(text, source_path).await;

        assert!(result.is_ok());
        let response = result.unwrap();

        assert_eq!(response.count, 1);
        assert_eq!(response.links.len(), 1);
        assert_eq!(response.links[0].target, "link");
    }

    #[tokio::test]
    async fn test_extract_document_title_extraction_command() {
        let content = "# Title\nContent".to_string();

        let result = extraction::extract_document_title(content).await;

        assert!(result.is_ok());
        let title = result.unwrap();

        assert_eq!(title, Some("Title".to_string()));
    }

    #[tokio::test]
    async fn test_resolve_wikilink_extraction_command() {
        let target = "test".to_string();
        let source_path = "index.md".to_string();
        let available_documents = vec![
            DocumentInfo {
                file_path: "test.md".to_string(),
                title: None,
            },
        ];

        let result = extraction::resolve_wikilink(target, source_path, available_documents).await;

        assert!(result.is_ok());
        let response = result.unwrap();

        assert!(response.resolved_path.is_some());
    }

    #[tokio::test]
    async fn test_extract_and_resolve_links_command() {
        let content = "[[link1]] and [[link2]]".to_string();
        let source_path = "index.md".to_string();
        let available_documents = vec![
            DocumentInfo {
                file_path: "link1.md".to_string(),
                title: None,
            },
            DocumentInfo {
                file_path: "link2.md".to_string(),
                title: None,
            },
        ];

        let result = extraction::extract_and_resolve_links(
            content,
            source_path,
            available_documents,
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap();

        assert!(response.is_object());
        assert_eq!(response["count"], 2);
    }
}
*/
// End of disabled extraction_commands tests

// Anthropic commands and tag generator tests removed - local-first approach only
