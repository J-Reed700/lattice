#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(deprecated)]


#[cfg(test)]
// Test code - allow common test patterns

mod error_context_tests {
    use std::io;
    use lattice::error::{AppError, ResultExt};

    #[test]
    fn test_result_context() {
        let result: std::result::Result<i32, io::Error> =
            Err(io::Error::new(io::ErrorKind::NotFound, "test"));
        let with_context = result.context("Operation failed");
        assert!(with_context.is_err());
        assert!(with_context
            .unwrap_err()
            .to_string()
            .contains("Operation failed"));
    }

    #[test]
    fn test_result_with_context() {
        let result: std::result::Result<i32, io::Error> =
            Err(io::Error::new(io::ErrorKind::NotFound, "test"));
        let with_context = result.with_context(|| format!("Failed at step {}", 1));
        assert!(with_context.is_err());
        assert!(with_context
            .unwrap_err()
            .to_string()
            .contains("Failed at step 1"));
    }

    #[test]
    fn test_option_context() {
        let opt: Option<i32> = None;
        let result = opt.context("Value not found");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Value not found"));
    }

    #[test]
    fn test_option_with_context() {
        let opt: Option<i32> = None;
        let result = opt.with_context(|| format!("Missing value at index {}", 5));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Missing value at index 5"));
    }

    #[test]
    fn test_context_preserves_ok() {
        let result: std::result::Result<i32, io::Error> = Ok(42);
        let with_context = result.context("This won't be used");
        assert_eq!(with_context.unwrap(), 42);
    }

    #[test]
    fn test_option_context_preserves_some() {
        let opt: Option<i32> = Some(42);
        let result = opt.context("This won't be used");
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_nested_context() {
        let result: std::result::Result<i32, io::Error> =
            Err(io::Error::new(io::ErrorKind::NotFound, "file.txt"));

        let step1 = result.context("Failed to read file");
        let step2 = step1.context("Failed to load configuration");

        let err_msg = step2.unwrap_err().to_string();
        assert!(err_msg.contains("Failed to load configuration"));
        assert!(err_msg.contains("Failed to read file"));
    }
}
