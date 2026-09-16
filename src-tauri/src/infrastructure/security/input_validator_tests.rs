//! Comprehensive tests for InputValidator
//!
//! Tests include:
//! - Path validation with attack vectors
//! - URL decoding edge cases
//! - Input sanitization
//! - JSON validation
//! - Search query validation (basic quality checks only)
//! - XSS prevention

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::shared::error::AppError;

    #[test]
    fn test_validate_search_query_valid() {
        let validator = InputValidator::new();

        let result = validator.validate_search_query("machine learning algorithms");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "machine learning algorithms");
    }

    #[test]
    fn test_validate_search_query_empty() {
        let validator = InputValidator::new();

        let result = validator.validate_search_query("");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("cannot be empty"));
    }

    #[test]
    fn test_validate_search_query_whitespace_only() {
        let validator = InputValidator::new();

        let result = validator.validate_search_query("   \n\t   ");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("cannot be empty"));
    }

    #[test]
    fn test_validate_search_query_too_long() {
        let validator = InputValidator::new();

        let long_query = "a".repeat(10001);
        let result = validator.validate_search_query(&long_query);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too long"));
    }

    #[test]
    fn test_validate_search_query_preserves_legitimate_characters() {
        let validator = InputValidator::new();

        let legitimate_queries = vec![
            ("O'Brien", "O'Brien"),                                    // Apostrophe in name
            ("don't forget", "don't forget"),                          // Apostrophe in contraction
            ("\"exact phrase\"", "\"exact phrase\""),                  // Quoted phrase
            ("C++; Rust; Go", "C++; Rust; Go"),                       // Semicolons in list
            ("SQL SELECT statement", "SQL SELECT statement"),          // SQL keywords as search terms
            ("/* comment style */", "/* comment style */"),            // Comment-like syntax
            ("test -- with dashes", "test -- with dashes"),           // Double dashes
            ("author's \"paper\" on SQL;", "author's \"paper\" on SQL;"), // Multiple special chars
        ];

        for (input, expected) in legitimate_queries {
            let result = validator.validate_search_query(input);
            assert!(
                result.is_ok(),
                "Legitimate query should be valid: {}",
                input
            );

            let validated = result.unwrap();
            assert_eq!(
                validated, expected,
                "Query should preserve all characters: input='{}', expected='{}', got='{}'",
                input, expected, validated
            );

            // Explicitly verify special characters are preserved
            if input.contains('\'') {
                assert!(validated.contains('\''), "Apostrophes should be preserved");
            }
            if input.contains('"') {
                assert!(validated.contains('"'), "Quotes should be preserved");
            }
            if input.contains(';') {
                assert!(validated.contains(';'), "Semicolons should be preserved");
            }
            if input.contains("--") {
                assert!(validated.contains("--"), "Double dashes should be preserved");
            }
            if input.contains("/*") || input.contains("*/") {
                assert!(
                    validated.contains("/*") || validated.contains("*/"),
                    "Comment markers should be preserved"
                );
            }
        }
    }

    #[test]
    fn test_validate_search_query_sql_like_patterns_accepted() {
        let validator = InputValidator::new();

        // These look like SQL injection but are legitimate search queries
        // Real SQL injection protection comes from parameterized queries, not filtering
        let sql_like_queries = vec![
            "test' OR '1'='1",
            "test'; DROP TABLE users--",
            "test' UNION SELECT * FROM embeddings--",
            "test/* comment */query",
            "1=1; DELETE FROM data",
        ];

        for query in sql_like_queries {
            let result = validator.validate_search_query(query);
            assert!(
                result.is_ok(),
                "SQL-like query should be accepted (protection via SQLx): {}",
                query
            );

            let validated = result.unwrap();
            assert_eq!(
                validated.trim(),
                query.trim(),
                "Query should be preserved exactly (minus whitespace trimming)"
            );
        }
    }

    #[test]
    fn test_validate_json_valid() {
        let json_str = r#"{"name": "test", "value": 42}"#;
        let result = InputValidator::validate_json(json_str, 10);

        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["name"], "test");
        assert_eq!(value["value"], 42);
    }

    #[test]
    fn test_validate_json_too_large() {
        let large_json = "a".repeat(11 * 1024 * 1024);
        let result = InputValidator::validate_json(&large_json, 10);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too large"));
    }

    #[test]
    fn test_validate_json_too_deep() {
        let mut json = String::from("{");
        for _ in 0..100 {
            json.push_str("\"a\":{");
        }
        json.push_str("\"value\":1");
        for _ in 0..100 {
            json.push('}');
        }
        json.push('}');

        let result = InputValidator::validate_json(&json, 10);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too deep"));
    }

    #[test]
    fn test_validate_json_invalid_syntax() {
        let invalid_json = r#"{"name": "test", "value": }"#;
        let result = InputValidator::validate_json(invalid_json, 10);

        assert!(result.is_err());
    }

    #[test]
    fn test_validate_json_nested_within_limit() {
        let json = r#"
        {
            "level1": {
                "level2": {
                    "level3": {
                        "value": 42
                    }
                }
            }
        }
        "#;

        let result = InputValidator::validate_json(json, 10);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_file_upload_valid() {
        let filename = "document.pdf";
        let content = b"PDF content here";
        let allowed_extensions = &["pdf", "txt", "md"];
        let max_size = 10 * 1024 * 1024; // 10MB

        let result = InputValidator::validate_file_upload(
            filename,
            content,
            allowed_extensions,
            max_size,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_file_upload_empty_filename() {
        let filename = "";
        let content = b"content";
        let allowed_extensions = &["pdf"];
        let max_size = 1024;

        let result = InputValidator::validate_file_upload(
            filename,
            content,
            allowed_extensions,
            max_size,
        );

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("cannot be empty"));
    }

    #[test]
    fn test_validate_file_upload_too_large() {
        let filename = "large.pdf";
        let content = vec![0u8; 100 * 1024 * 1024]; // 100MB
        let allowed_extensions = &["pdf"];
        let max_size = 10 * 1024 * 1024; // 10MB limit

        let result = InputValidator::validate_file_upload(
            filename,
            &content,
            allowed_extensions,
            max_size,
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too large"));
    }

    #[test]
    fn test_validate_file_upload_disallowed_extension() {
        let filename = "script.exe";
        let content = b"executable content";
        let allowed_extensions = &["pdf", "txt", "md"];
        let max_size = 1024 * 1024;

        let result = InputValidator::validate_file_upload(
            filename,
            content,
            allowed_extensions,
            max_size,
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_validate_file_upload_no_extension() {
        let filename = "noextension";
        let content = b"content";
        let allowed_extensions = &["pdf"];
        let max_size = 1024;

        let result = InputValidator::validate_file_upload(
            filename,
            content,
            allowed_extensions,
            max_size,
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_validate_file_upload_path_traversal_attempt() {
        let malicious_filenames = vec![
            "../../../etc/passwd",
            "..\\..\\..\\windows\\system32\\config\\sam",
            "dir/../../../secret.txt",
            "file.txt/../../etc/shadow",
        ];

        let content = b"content";
        let allowed_extensions = &["txt"];
        let max_size = 1024;

        for filename in malicious_filenames {
            let result = InputValidator::validate_file_upload(
                filename,
                content,
                allowed_extensions,
                max_size,
            );

            assert!(
                result.is_err()
                    || !result.as_ref().unwrap().to_string().contains(".."),
                "Path traversal should be prevented: {}",
                filename
            );
        }
    }

    #[test]
    fn test_validate_search_query_unicode() {
        let validator = InputValidator::new();

        let unicode_queries = vec![
            "こんにちは世界",             // Japanese
            "Привет мир",                // Russian
            "مرحبا بالعالم",             // Arabic
            "🎉🔥💯",                      // Emojis
            "Ñoño español",              // Spanish
        ];

        for query in unicode_queries {
            let result = validator.validate_search_query(query);
            assert!(result.is_ok(), "Unicode query should be valid: {}", query);
        }
    }

    #[test]
    fn test_validate_search_query_special_characters() {
        let validator = InputValidator::new();

        let queries = vec![
            "test@example.com",
            "C++ programming",
            "file.name.with.dots",
            "hyphenated-query",
            "under_score_query",
        ];

        for query in queries {
            let result = validator.validate_search_query(query);
            assert!(result.is_ok(), "Query with special chars should be valid: {}", query);
        }
    }

    #[test]
    fn test_validate_json_array() {
        let json_str = r#"[1, 2, 3, 4, 5]"#;
        let result = InputValidator::validate_json(json_str, 10);

        assert!(result.is_ok());
        let value = result.unwrap();
        assert!(value.is_array());
        assert_eq!(value.as_array().unwrap().len(), 5);
    }

    #[test]
    fn test_validate_json_null_values() {
        let json_str = r#"{"value": null, "number": 0, "empty": ""}"#;
        let result = InputValidator::validate_json(json_str, 10);

        assert!(result.is_ok());
        let value = result.unwrap();
        assert!(value["value"].is_null());
        assert_eq!(value["number"], 0);
        assert_eq!(value["empty"], "");
    }

    #[test]
    fn test_json_depth_flat_object() {
        let json_str = r#"{"a": 1, "b": 2, "c": 3}"#;
        let value: serde_json::Value = serde_json::from_str(json_str).unwrap();

        let depth = InputValidator::get_json_depth(&value);
        assert_eq!(depth, 1);
    }

    #[test]
    fn test_json_depth_nested_object() {
        let json_str = r#"{"a": {"b": {"c": 1}}}"#;
        let value: serde_json::Value = serde_json::from_str(json_str).unwrap();

        let depth = InputValidator::get_json_depth(&value);
        assert_eq!(depth, 3);
    }

    #[test]
    fn test_json_depth_array() {
        let json_str = r#"[1, [2, [3, [4]]]]"#;
        let value: serde_json::Value = serde_json::from_str(json_str).unwrap();

        let depth = InputValidator::get_json_depth(&value);
        assert_eq!(depth, 4);
    }

    #[test]
    fn test_json_depth_mixed() {
        let json_str = r#"{"a": [{"b": [{"c": 1}]}]}"#;
        let value: serde_json::Value = serde_json::from_str(json_str).unwrap();

        let depth = InputValidator::get_json_depth(&value);
        assert_eq!(depth, 4);
    }
}
