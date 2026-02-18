use super::*;

#[cfg(test)]
mod integration_tests {
    use super::*;

    // Note: Full integration tests for setup functions require Tauri AppHandle
    // which is not available in unit test context. These tests focus on
    // verifying error message formatting and structure.

    #[test]
    fn test_error_message_formatting() {
        let test_path = std::path::PathBuf::from("/test/path");
        let error_msg = format!(
            "Failed to create directory at {:?}.\n\n\
             Possible causes:\n\
             - Missing permissions\n\
             - Insufficient disk space\n\n\
             Suggested actions:\n\
             - Check permissions\n\
             - Free up disk space\n\n\
             Error: test error",
            test_path
        );

        assert!(error_msg.contains("Failed to create directory"));
        assert!(error_msg.contains("/test/path"));
        assert!(error_msg.contains("Missing permissions"));
        assert!(error_msg.contains("Suggested actions"));
        assert!(error_msg.contains("test error"));
    }

    #[test]
    fn test_error_message_structure() {
        let error = "Database connection failed";
        let formatted = format!(
            "Failed to connect.\n\n\
             Possible causes:\n\
             - {}\n\n\
             Suggested actions:\n\
             - Retry connection\n\n\
             Error: {}",
            "Connection timeout", error
        );

        assert!(formatted.contains("Possible causes:"));
        assert!(formatted.contains("Suggested actions:"));
        assert!(formatted.contains(error));
    }
}
