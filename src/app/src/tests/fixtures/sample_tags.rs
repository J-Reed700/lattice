#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(deprecated)]

use serde::{Deserialize, Serialize};
// Test code - allow common test patterns
#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(unused_variables)]
#![allow(unused_imports)]


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestTag {
    pub name: String,
    pub color: String,
    pub expected_docs: Vec<String>,
}

pub fn get_predefined_tags() -> Vec<TestTag> {
    vec![
        TestTag {
            name: "machine-learning".to_string(),
            color: "#3b82f6".to_string(),
            expected_docs: vec!["test-doc-1".to_string()],
        },
        TestTag {
            name: "ai".to_string(),
            color: "#8b5cf6".to_string(),
            expected_docs: vec!["test-doc-1".to_string()],
        },
        TestTag {
            name: "rust".to_string(),
            color: "#ef4444".to_string(),
            expected_docs: vec!["test-doc-2".to_string()],
        },
        TestTag {
            name: "programming".to_string(),
            color: "#10b981".to_string(),
            expected_docs: vec!["test-doc-2".to_string()],
        },
        TestTag {
            name: "systems".to_string(),
            color: "#f59e0b".to_string(),
            expected_docs: vec!["test-doc-2".to_string(), "test-doc-3".to_string()],
        },
        TestTag {
            name: "design".to_string(),
            color: "#ec4899".to_string(),
            expected_docs: vec!["test-doc-3".to_string()],
        },
        TestTag {
            name: "architecture".to_string(),
            color: "#6366f1".to_string(),
            expected_docs: vec!["test-doc-3".to_string()],
        },
    ]
}

pub fn get_auto_generated_tags(content_type: &str) -> Vec<String> {
    match content_type {
        "machine-learning" => vec![
            "machine-learning".to_string(),
            "ai".to_string(),
            "neural-networks".to_string(),
            "deep-learning".to_string(),
        ],
        "rust-programming" => vec![
            "rust".to_string(),
            "programming".to_string(),
            "systems".to_string(),
        ],
        "systems-design" => vec![
            "systems".to_string(),
            "design".to_string(),
            "architecture".to_string(),
            "scalability".to_string(),
        ],
        _ => vec!["general".to_string(), "notes".to_string()],
    }
}

pub fn get_tag_hierarchy() -> Vec<(String, Vec<String>)> {
    vec![
        (
            "ai".to_string(),
            vec![
                "machine-learning".to_string(),
                "deep-learning".to_string(),
                "neural-networks".to_string(),
            ],
        ),
        (
            "programming".to_string(),
            vec![
                "rust".to_string(),
                "python".to_string(),
                "javascript".to_string(),
            ],
        ),
        (
            "systems".to_string(),
            vec![
                "distributed-systems".to_string(),
                "architecture".to_string(),
                "design".to_string(),
            ],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_predefined_tags() {
        let tags = get_predefined_tags();
        assert!(tags.len() >= 5);

        for tag in &tags {
            assert!(!tag.name.is_empty());
            assert!(tag.color.starts_with('#'));
            assert_eq!(tag.color.len(), 7);
        }
    }

    #[test]
    fn test_auto_generated_tags() {
        let ml_tags = get_auto_generated_tags("machine-learning");
        assert!(ml_tags.contains(&"machine-learning".to_string()));
        assert!(ml_tags.contains(&"ai".to_string()));

        let rust_tags = get_auto_generated_tags("rust-programming");
        assert!(rust_tags.contains(&"rust".to_string()));

        let default_tags = get_auto_generated_tags("unknown");
        assert_eq!(default_tags.len(), 2);
    }

    #[test]
    fn test_tag_hierarchy() {
        let hierarchy = get_tag_hierarchy();
        assert!(hierarchy.len() >= 3);

        let ai_tags = hierarchy.iter().find(|(parent, _)| parent == "ai");
        assert!(ai_tags.is_some());
        assert!(ai_tags.unwrap().1.len() >= 3);
    }
}
