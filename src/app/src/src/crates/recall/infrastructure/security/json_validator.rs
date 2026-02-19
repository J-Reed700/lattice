use crate::shared::error::{AppError, Result};
use serde::de::DeserializeOwned;
use serde_json::Value;

/// SECURITY FIX: Safe JSON deserialization (CWE-502)
/// Prevents unsafe deserialization attacks
pub struct JsonValidator;

impl JsonValidator {
    /// Safely deserialize JSON with size and depth limits
    pub fn safe_deserialize<T: DeserializeOwned>(
        json_str: &str,
        max_size: usize,
        max_depth: usize,
    ) -> Result<T> {
        // Check size limit
        if json_str.len() > max_size {
            return Err(AppError::InvalidInput(format!(
                "JSON too large: {} bytes (max: {} bytes)",
                json_str.len(),
                max_size
            )));
        }

        // First parse to Value to check structure
        let value: Value = serde_json::from_str(json_str)?;

        // Check depth
        let depth = Self::calculate_depth(&value);
        if depth > max_depth {
            return Err(AppError::InvalidInput(format!(
                "JSON nesting too deep: {} (max: {})",
                depth, max_depth
            )));
        }

        // Check for dangerous patterns
        Self::check_dangerous_patterns(&value)?;

        // Now deserialize to target type
        Ok(serde_json::from_value(value)?)
    }

    /// Calculate JSON depth
    fn calculate_depth(value: &Value) -> usize {
        match value {
            Value::Object(map) => 1 + map.values().map(Self::calculate_depth).max().unwrap_or(0),
            Value::Array(arr) => 1 + arr.iter().map(Self::calculate_depth).max().unwrap_or(0),
            _ => 0,
        }
    }

    /// Check for dangerous patterns in JSON
    fn check_dangerous_patterns(value: &Value) -> Result<()> {
        Self::check_value_recursive(value, 0)
    }

    fn check_value_recursive(value: &Value, depth: usize) -> Result<()> {
        // Prevent stack overflow from recursion
        if depth > 100 {
            return Err(AppError::InvalidInput(
                "JSON structure too complex".to_string(),
            ));
        }

        match value {
            Value::Object(map) => {
                for (key, val) in map {
                    // Check for dangerous keys
                    if Self::is_dangerous_key(key) {
                        return Err(AppError::InvalidInput(format!(
                            "Dangerous JSON key detected: {}",
                            key
                        )));
                    }
                    Self::check_value_recursive(val, depth + 1)?;
                }
            }
            Value::Array(arr) => {
                // Check array size
                if arr.len() > 10000 {
                    return Err(AppError::InvalidInput(format!(
                        "Array too large: {} elements",
                        arr.len()
                    )));
                }
                for val in arr {
                    Self::check_value_recursive(val, depth + 1)?;
                }
            }
            Value::String(s) => {
                // Check string size
                if s.len() > 1_000_000 {
                    return Err(AppError::InvalidInput(format!(
                        "String value too large: {} bytes",
                        s.len()
                    )));
                }
                // Check for code injection attempts
                if Self::contains_code_pattern(s) {
                    return Err(AppError::InvalidInput(
                        "Potentially dangerous string content".to_string(),
                    ));
                }
            }
            _ => {}
        }

        Ok(())
    }

    /// Check if a key name is potentially dangerous
    fn is_dangerous_key(key: &str) -> bool {
        let dangerous_patterns = [
            "__proto__",
            "prototype",
            "constructor",
            "$where",
            "$gt",
            "$lt",
            "$ne",
            "$regex",
            "eval",
            "exec",
            "system",
            "spawn",
        ];

        let key_lower = key.to_lowercase();
        dangerous_patterns
            .iter()
            .any(|&pattern| key_lower.contains(pattern))
    }

    /// Check if string contains code patterns
    /// Uses context-aware matching to avoid false positives on legitimate paths
    fn contains_code_pattern(s: &str) -> bool {
        let s_lower = s.to_lowercase();

        // Check for HTML/JavaScript injection patterns
        if s_lower.contains("<script") && s_lower.contains(">") {
            return true;
        }

        if s_lower.contains("javascript:") {
            return true;
        }

        if s_lower.contains("vbscript:") {
            return true;
        }

        if s_lower.contains("data:text/html") {
            return true;
        }

        // Check for HTML event handlers
        let event_handlers = ["onload=", "onerror=", "onclick=", "onmouseover="];
        for pattern in &event_handlers {
            if s_lower.contains(pattern) {
                return true;
            }
        }

        // Only flag eval/setTimeout if followed by parentheses AND
        // not part of a REAL file path (actual path structure with multiple components)
        let function_patterns = ["eval(", "settimeout(", "setinterval("];
        for pattern in &function_patterns {
            if s_lower.contains(pattern) {
                if let Some(idx) = s_lower.find(pattern) {
                    // Word boundary check (part of larger word like "evaluation" or "llama-eval")
                    if idx > 0 {
                        let prev_char = s.chars().nth(idx - 1);
                        if prev_char
                            .map(|c| c.is_alphanumeric() || c == '-' || c == '_')
                            .unwrap_or(false)
                        {
                            continue; // Part of legitimate word/identifier
                        }
                    }

                    // Path check: Must be ACTUAL path structure (multiple components)
                    if s.contains('/') || s.contains('\\') {
                        let components: Vec<&str> = s
                            .split(&['/', '\\'][..])
                            .filter(|c| !c.is_empty())
                            .collect();

                        // Require >= 3 components for legitimate paths
                        // Real paths: "/home/user/eval()/documents" (4 components)
                        // Malicious: "eval(hack)/etc/passwd" (3 components) or "eval(x)/" (1-2 components)
                        if components.len() >= 3 {
                            // Check if any component is JUST the pattern with empty/simple parens
                            // like "eval()" or "setTimeout" - not "eval(alert(1))"
                            let has_legitimate_component = components.iter().any(|comp| {
                                let comp_lower = comp.to_lowercase();
                                if comp_lower.contains(pattern) {
                                    // Check if parens are empty or contain only simple content
                                    // Legitimate: "eval()" or "eval(1)" or "setTimeout"
                                    // Malicious: "eval(alert(1))" or "setTimeout(hack)"
                                    if let Some(paren_start) = comp.find('(') {
                                        if let Some(paren_end) = comp.rfind(')') {
                                            if paren_end > paren_start {
                                                let inside_parens =
                                                    &comp[paren_start + 1..paren_end];
                                                // Empty parens or pure numeric content ONLY
                                                // Don't allow alphabetic identifiers like "hack" or "mal"
                                                let is_simple = inside_parens.is_empty()
                                                    || inside_parens
                                                        .chars()
                                                        .all(|c| c.is_numeric());
                                                return is_simple;
                                            }
                                        }
                                    }
                                    // Pattern exists but no complete parentheses pair
                                    // Could be partial like "setTimeout" directory name
                                    return !comp.contains('(') || !comp.contains(')');
                                }
                                false
                            });

                            if has_legitimate_component {
                                continue; // Likely legitimate path
                            }
                        }
                    }

                    return true; // Detected code injection
                }
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestStruct {
        name: String,
        value: i32,
    }

    #[test]
    fn test_safe_deserialize() {
        let json = r#"{"name": "test", "value": 42}"#;
        let result: TestStruct = JsonValidator::safe_deserialize(json, 1024, 10).unwrap();

        assert_eq!(result.name, "test");
        assert_eq!(result.value, 42);
    }

    #[test]
    fn test_size_limit() {
        let large_json = format!(r#"{{"data": "{}"}}"#, "x".repeat(1000));
        let result: Result<Value> = JsonValidator::safe_deserialize(
            &large_json,
            100, // Small size limit
            10,
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_depth_limit() {
        let deep_json = r#"{"a":{"b":{"c":{"d":{"e":"f"}}}}}"#;

        // Should fail with depth limit 3
        let result: Result<Value> = JsonValidator::safe_deserialize(deep_json, 1024, 3);
        assert!(result.is_err());

        // Should pass with depth limit 10
        let result: Result<Value> = JsonValidator::safe_deserialize(deep_json, 1024, 10);
        assert!(result.is_ok());
    }

    #[test]
    fn test_dangerous_keys() {
        let dangerous_json = r#"{"__proto__": {"isAdmin": true}}"#;
        let result: Result<Value> = JsonValidator::safe_deserialize(dangerous_json, 1024, 10);

        assert!(result.is_err());
    }

    #[test]
    fn test_code_patterns() {
        let dangerous_json = r#"{"html": "<script>alert('xss')</script>"}"#;
        let result: Result<Value> = JsonValidator::safe_deserialize(dangerous_json, 1024, 10);

        assert!(result.is_err());
    }

    #[test]
    fn test_legitimate_paths_allowed() {
        // Test file paths containing function-like patterns
        let legitimate_cases = vec![
            r#"{"path": "/home/user/eval()/documents"}"#,
            r#"{"path": "C:\\Users\\setTimeout\\file.txt"}"#,
            r#"{"model": "llama-eval(1)"}"#,
            r#"{"name": "evaluation-model"}"#,
            r#"{"dir": "/var/lib/eval()/data"}"#,
            r#"{"file": "\\server\\eval()\\share"}"#,
        ];

        for json in legitimate_cases {
            let result: Result<Value> = JsonValidator::safe_deserialize(json, 1024, 10);
            assert!(
                result.is_ok(),
                "Legitimate value should be allowed: {}",
                json
            );
        }
    }

    #[test]
    fn test_actual_code_patterns_rejected() {
        // Test actual code injection attempts
        let dangerous_cases = vec![
            r#"{"code": "eval(alert(1))"}"#,
            r#"{"script": "<script>alert(1)</script>"}"#,
            r#"{"url": "javascript:void(0)"}"#,
            r#"{"url": "vbscript:msgbox(1)"}"#,
            r#"{"html": "<img onerror=alert(1)>"}"#,
            r#"{"html": "<body onload=alert(1)>"}"#,
            r#"{"data": "data:text/html,<script>alert(1)</script>"}"#,
            r#"{"code": "setTimeout(malicious)"}"#,
            r#"{"code": "setInterval(bad)"}"#,
        ];

        for json in dangerous_cases {
            let result: Result<Value> = JsonValidator::safe_deserialize(json, 1024, 10);
            assert!(
                result.is_err(),
                "Dangerous pattern should be rejected: {}",
                json
            );
        }
    }

    #[test]
    fn test_word_boundary_detection() {
        // Test that function patterns as part of larger words are allowed
        let legitimate_words = vec![
            r#"{"status": "evaluation in progress"}"#,
            r#"{"type": "medieval history"}"#,
            r#"{"name": "setTimeout_config"}"#, // Has path check, but also word boundary
        ];

        for json in legitimate_words {
            let result: Result<Value> = JsonValidator::safe_deserialize(json, 1024, 10);
            assert!(
                result.is_ok(),
                "Legitimate word should be allowed: {}",
                json
            );
        }
    }

    #[test]
    fn test_contains_code_pattern_directly() {
        // Direct tests of the contains_code_pattern function

        // Should NOT be flagged (legitimate values)
        assert!(!JsonValidator::contains_code_pattern(
            "/home/user/eval()/documents"
        ));
        assert!(!JsonValidator::contains_code_pattern("llama-eval(1)"));
        assert!(!JsonValidator::contains_code_pattern("evaluation-model"));
        assert!(!JsonValidator::contains_code_pattern(
            "C:\\Users\\setTimeout\\file.txt"
        ));
        assert!(!JsonValidator::contains_code_pattern(
            "/var/lib/setInterval()/data"
        ));
        assert!(!JsonValidator::contains_code_pattern("medieval"));

        // SHOULD be flagged (actual code injection)
        assert!(JsonValidator::contains_code_pattern("eval(alert(1))"));
        assert!(JsonValidator::contains_code_pattern(
            "<script>alert(1)</script>"
        ));
        assert!(JsonValidator::contains_code_pattern("javascript:void(0)"));
        assert!(JsonValidator::contains_code_pattern("vbscript:msgbox(1)"));
        assert!(JsonValidator::contains_code_pattern(
            "data:text/html,<script>"
        ));
        assert!(JsonValidator::contains_code_pattern(
            "<img onerror=alert(1)>"
        ));
        assert!(JsonValidator::contains_code_pattern(
            "<body onload=malicious()>"
        ));
    }

    #[test]
    fn test_injection_attempts_with_path_characters_rejected() {
        // CRITICAL: These injection attempts with '/' should be REJECTED
        assert!(JsonValidator::contains_code_pattern("eval(alert(1))/"));
        assert!(JsonValidator::contains_code_pattern("/eval(malicious())"));
        assert!(JsonValidator::contains_code_pattern(
            "setTimeout(hack)/etc/passwd"
        ));
        assert!(JsonValidator::contains_code_pattern("\\eval(bad)"));
        assert!(JsonValidator::contains_code_pattern("eval(code)/"));
        assert!(JsonValidator::contains_code_pattern("setInterval(mal)/x"));
    }

    #[test]
    fn test_legitimate_paths_still_allowed() {
        // These are REAL paths and should still be allowed
        assert!(!JsonValidator::contains_code_pattern(
            "/home/user/eval()/documents"
        ));
        assert!(!JsonValidator::contains_code_pattern(
            "C:\\Users\\setTimeout\\file.txt"
        ));
        assert!(!JsonValidator::contains_code_pattern(
            "/var/lib/setInterval()/data/file.db"
        ));
        assert!(!JsonValidator::contains_code_pattern(
            "/path/to/evaluation-directory/file.txt"
        ));
    }

    #[test]
    fn test_edge_cases_for_path_validation() {
        // Edge cases: patterns with numeric content (should be allowed in paths)
        assert!(!JsonValidator::contains_code_pattern(
            "/home/user/eval(1)/documents"
        ));
        assert!(!JsonValidator::contains_code_pattern(
            "/var/lib/setTimeout(123)/data"
        ));

        // Edge cases: short paths with patterns (should be REJECTED)
        assert!(JsonValidator::contains_code_pattern("eval(x)/"));
        assert!(JsonValidator::contains_code_pattern("eval()/x"));

        // Edge case: 3 components but with code in parens (should be REJECTED)
        assert!(JsonValidator::contains_code_pattern(
            "setTimeout(malware)/etc/passwd"
        ));

        // Edge case: legitimate 3-component path (should be ALLOWED)
        assert!(!JsonValidator::contains_code_pattern(
            "/var/setTimeout/file"
        ));
    }
}
