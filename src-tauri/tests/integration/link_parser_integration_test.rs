#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]

//! Integration tests for wikilink parser.

//!
//! Tests comprehensive link parsing, resolution, and title extraction.
//!
//! ## Running Tests
//!
//! ```bash
//! cargo test --test '*' link_parser
//! ```

use lattice::extraction::link_parser::{LinkParser, WikiLink, DocumentInfo};

#[test]
fn test_simple_wikilink_parsing() {
    let parser = LinkParser::new();
    let content = "This is [[simple link]].";

    let links = parser.parse_document(content, "test.md");

    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target, "simple link");
    assert_eq!(links[0].source_path, "test.md");
    assert!(links[0].display_text.is_none());
    assert!(links[0].header.is_none());
}

#[test]
fn test_wikilink_with_display_text() {
    let parser = LinkParser::new();
    let content = "See [[note|custom display text]].";

    let links = parser.parse_document(content, "test.md");

    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target, "note");
    assert_eq!(links[0].display_text, Some("custom display text".to_string()));
}

#[test]
fn test_wikilink_with_header() {
    let parser = LinkParser::new();
    let content = "Check [[note#section]].";

    let links = parser.parse_document(content, "test.md");

    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target, "note");
    assert_eq!(links[0].header, Some("section".to_string()));
}

#[test]
fn test_wikilink_with_display_and_header() {
    let parser = LinkParser::new();
    let content = "See [[complex|link#with section]].";

    let links = parser.parse_document(content, "test.md");

    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target, "complex");
    assert_eq!(links[0].display_text, Some("link".to_string()));
    assert_eq!(links[0].header, Some("with section".to_string()));
}

#[test]
fn test_wikilink_with_path() {
    let parser = LinkParser::new();
    let content = "See [[projects/ml/notes]].";

    let links = parser.parse_document(content, "test.md");

    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target, "projects/ml/notes");
}

#[test]
fn test_multiple_wikilinks() {
    let parser = LinkParser::new();
    let content = r#"
    This is [[link1]].
    Also see [[link2|display]] and [[link3#header]].
    Finally check [[complex|text#section]].
    "#;

    let links = parser.parse_document(content, "test.md");

    assert_eq!(links.len(), 4);
    assert_eq!(links[0].target, "link1");
    assert_eq!(links[1].target, "link2");
    assert_eq!(links[2].target, "link3");
    assert_eq!(links[3].target, "complex");
}

#[test]
fn test_wikilinks_in_different_contexts() {
    let parser = LinkParser::new();
    let content = r#"
# Header with [[link in header]]

Paragraph with [[link in paragraph]].

- List item with [[link in list]]
- Another item

> Quote with [[link in quote]]

`code with [[not a link]]` (should be extracted)

```
[[link in code block]]
```
    "#;

    let links = parser.parse_document(content, "test.md");

    // All should be extracted, even from code
    assert!(links.len() >= 5);

    let targets: Vec<String> = links.iter().map(|l| l.target.clone()).collect();
    assert!(targets.contains(&"link in header".to_string()));
    assert!(targets.contains(&"link in paragraph".to_string()));
    assert!(targets.contains(&"link in list".to_string()));
    assert!(targets.contains(&"link in quote".to_string()));
}

#[test]
fn test_edge_cases() {
    let parser = LinkParser::new();

    // Empty link
    let empty = parser.parse_document("[[]]", "test.md");
    assert_eq!(empty.len(), 0);

    // Nested brackets (should handle gracefully)
    let nested = parser.parse_document("[[[nested]]]", "test.md");
    // Behavior depends on implementation

    // Link with special characters
    let special = parser.parse_document("[[file-name_v2.0]]", "test.md");
    assert_eq!(special[0].target, "file-name_v2.0");

    // Link with spaces
    let spaces = parser.parse_document("[[  spaced link  ]]", "test.md");
    assert!(!spaces.is_empty());
}

#[test]
fn test_title_extraction_from_h1() {
    let parser = LinkParser::new();

    let content = "# My Document Title\n\nContent here";
    let title = parser.extract_title(content);

    assert_eq!(title, Some("My Document Title".to_string()));
}

#[test]
fn test_title_extraction_from_frontmatter() {
    let parser = LinkParser::new();

    let content = r#"---
title: Frontmatter Title
author: Test
---

Content here
"#;

    let title = parser.extract_title(content);

    assert!(title.is_some());
    assert!(title.unwrap().contains("Frontmatter") || title.unwrap().contains("Content"));
}

#[test]
fn test_title_extraction_fallback() {
    let parser = LinkParser::new();

    let content = "First line without markdown\n\nMore content";
    let title = parser.extract_title(content);

    assert_eq!(title, Some("First line without markdown".to_string()));
}

#[test]
fn test_title_extraction_empty_content() {
    let parser = LinkParser::new();

    let title = parser.extract_title("");
    assert!(title.is_none() || title == Some("".to_string()));
}

#[test]
fn test_link_resolution_exact_match() {
    let parser = LinkParser::new();

    let documents = vec![
        DocumentInfo {
            file_path: "notes/todo.md".to_string(),
            title: Some("Todo List".to_string()),
        },
        DocumentInfo {
            file_path: "projects/project.md".to_string(),
            title: Some("Project".to_string()),
        },
    ];

    let resolved = parser.resolve_link("todo", "notes/index.md", &documents);

    assert!(resolved.is_some());
    assert!(resolved.unwrap().contains("todo"));
}

#[test]
fn test_link_resolution_with_extension() {
    let parser = LinkParser::new();

    let documents = vec![
        DocumentInfo {
            file_path: "test.md".to_string(),
            title: None,
        },
    ];

    let resolved = parser.resolve_link("test.md", "index.md", &documents);

    assert_eq!(resolved, Some("test.md".to_string()));
}

#[test]
fn test_link_resolution_by_title() {
    let parser = LinkParser::new();

    let documents = vec![
        DocumentInfo {
            file_path: "notes/my-great-note.md".to_string(),
            title: Some("My Great Note".to_string()),
        },
    ];

    let resolved = parser.resolve_link("My Great Note", "index.md", &documents);

    assert!(resolved.is_some());
}

#[test]
fn test_link_resolution_fuzzy_match() {
    let parser = LinkParser::new();

    let documents = vec![
        DocumentInfo {
            file_path: "ideas/Test Idea.md".to_string(),
            title: Some("Test Idea".to_string()),
        },
    ];

    // Case-insensitive match
    let resolved = parser.resolve_link("test idea", "index.md", &documents);

    assert!(resolved.is_some());
}

#[test]
fn test_link_resolution_no_match() {
    let parser = LinkParser::new();

    let documents = vec![
        DocumentInfo {
            file_path: "note1.md".to_string(),
            title: None,
        },
    ];

    let resolved = parser.resolve_link("nonexistent", "index.md", &documents);

    assert!(resolved.is_none());
}

#[test]
fn test_link_resolution_multiple_candidates() {
    let parser = LinkParser::new();

    let documents = vec![
        DocumentInfo {
            file_path: "notes/test.md".to_string(),
            title: Some("Test Note".to_string()),
        },
        DocumentInfo {
            file_path: "projects/test.md".to_string(),
            title: Some("Test Project".to_string()),
        },
    ];

    let resolved = parser.resolve_link("test", "notes/index.md", &documents);

    assert!(resolved.is_some());
    // Ideally should prefer notes/test.md since source is in notes/
}

#[test]
fn test_line_number_tracking() {
    let parser = LinkParser::new();

    let content = r#"Line 1
Line 2 with [[link1]]
Line 3
Line 4 with [[link2]]
"#;

    let links = parser.parse_document(content, "test.md");

    assert_eq!(links.len(), 2);
    assert_eq!(links[0].line_number, 2);
    assert_eq!(links[1].line_number, 4);
}

#[test]
fn test_context_extraction() {
    let parser = LinkParser::new();

    let content = "This is some context before [[link]] and after.";

    let links = parser.parse_document(content, "test.md");

    assert_eq!(links.len(), 1);
    assert!(links[0].context.len() > 0);
    assert!(links[0].context.contains("[[link]]"));
}

#[test]
fn test_comprehensive_document_parsing() {
    let parser = LinkParser::new();

    let content = r#"---
title: Test Document
---

# Introduction

This document references [[basic link]] and [[display|Custom Text]].

## Section with Links

- Point to [[project/subdir/file]]
- Reference with header: [[note#specific-section]]
- Complex: [[path/to/note|Display#Header]]

## Another Section

Normal paragraph with [[embedded link]] in text.

> Quote with [[quoted link]]

1. Numbered item with [[numbered link]]
2. Another item

[[final link]] at the end.
"#;

    let links = parser.parse_document(content, "comprehensive-test.md");

    // Count expected links
    assert!(links.len() >= 9);

    let targets: Vec<String> = links.iter().map(|l| l.target.clone()).collect();
    assert!(targets.contains(&"basic link".to_string()));
    assert!(targets.contains(&"display".to_string()) || targets.contains(&"Custom Text".to_string()));
    assert!(targets.contains(&"note".to_string()));
    assert!(targets.contains(&"final link".to_string()));

    assert!(links.iter().any(|l| l.line_number > 0));

    assert!(links.iter().all(|l| !l.context.is_empty()));
}

#[test]
fn test_performance_with_large_document() {
    let parser = LinkParser::new();

    let mut content = String::new();
    for i in 0..1000 {
        content.push_str(&format!("This is line {} with [[link{}]].\n", i, i));
    }

    let start = std::time::Instant::now();
    let links = parser.parse_document(&content, "large.md");
    let duration = start.elapsed();

    assert_eq!(links.len(), 1000);
    println!("Parsed 1000 links in {:?}", duration);
    assert!(duration.as_millis() < 100, "Parsing took too long: {:?}", duration);
}
