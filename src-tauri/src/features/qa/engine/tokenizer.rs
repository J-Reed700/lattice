/// Token counting and truncation utilities
///
/// Provides approximate token counting for context management.
/// Uses a simple heuristic: ~4 characters per token (typical for English text).
/// This is faster than running a full tokenizer and sufficient for context budgeting.
use crate::features::qa::engine::types::QAError;

const CHARS_PER_TOKEN: f32 = 4.0;

/// Count approximate tokens in text
///
/// Uses a simple heuristic (4 chars per token) which is fast and reasonably
/// accurate for most text. For more precise counting, consider using a
/// tokenizer library, but this is sufficient for context management.
///
/// # Arguments
/// * `text` - Text to count tokens for
///
/// # Returns
/// Approximate number of tokens
///
/// # Examples
/// ```
/// use lattice::qa::count_tokens;
///
/// let text = "Hello, world!";
/// let count = count_tokens(text);
/// assert!(count > 0);
/// ```
pub fn count_tokens(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }

    // Simple heuristic: 4 characters per token
    // This is reasonably accurate for English text and very fast
    let char_count = text.chars().count();
    ((char_count as f32) / CHARS_PER_TOKEN).ceil() as usize
}

/// Truncate text to fit within token limit
///
/// Truncates the text to approximately fit within the specified token budget.
/// Uses character-based truncation with the same heuristic as count_tokens.
///
/// # Arguments
/// * `text` - Text to truncate
/// * `max_tokens` - Maximum number of tokens allowed
/// * `suffix` - Optional suffix to append if truncated (default: "...")
///
/// # Returns
/// Truncated text with suffix appended if truncation occurred
///
/// # Errors
/// Returns error if max_tokens is 0 or negative
///
/// # Examples
/// ```
/// use lattice::qa::truncate_to_tokens;
///
/// let text = "This is a long text " .repeat(100);
/// let truncated = truncate_to_tokens(&text, 50, Some("...")).unwrap();
/// assert!(truncate_to_tokens(&truncated, 50, None).unwrap().len() <= truncated.len());
/// ```
pub fn truncate_to_tokens(
    text: &str,
    max_tokens: usize,
    suffix: Option<&str>,
) -> Result<String, QAError> {
    if max_tokens == 0 {
        return Err(QAError::InvalidInput(
            "max_tokens must be greater than 0".to_string(),
        ));
    }

    let current_tokens = count_tokens(text);

    // If text already fits, return as-is
    if current_tokens <= max_tokens {
        return Ok(text.to_string());
    }

    let max_chars = (max_tokens as f32 * CHARS_PER_TOKEN) as usize;

    let chars: Vec<char> = text.chars().collect();
    let truncate_at = max_chars.min(chars.len());

    let char_slice = chars.get(..truncate_at).ok_or_else(|| {
        QAError::InvalidInput(format!(
            "Character slice out of bounds: 0..{} (length: {})",
            truncate_at,
            chars.len()
        ))
    })?;
    let truncated: String = char_slice.iter().collect();

    // Append suffix if provided
    if let Some(suf) = suffix {
        Ok(format!("{}{}", truncated, suf))
    } else {
        Ok(truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_tokens_empty() {
        assert_eq!(count_tokens(""), 0);
    }

    #[test]
    fn test_count_tokens_basic() {
        let text = "Hello, world!";
        let count = count_tokens(text);
        assert!(count > 0);
        assert!(count <= 10); // Should be around 3-4 tokens
    }

    #[test]
    fn test_count_tokens_long() {
        let text = "word ".repeat(1000);
        let count = count_tokens(&text);
        assert!(count > 1000); // Should be at least 1000 tokens
    }

    #[test]
    fn test_truncate_fits() {
        let text = "Short text";
        let result = truncate_to_tokens(text, 100, Some("...")).unwrap();
        assert_eq!(result, text);
    }

    #[test]
    fn test_truncate_with_suffix() {
        let text = "This is a very long text ".repeat(100);
        let result = truncate_to_tokens(&text, 50, Some("...")).unwrap();
        assert!(result.ends_with("..."));
        assert!(count_tokens(&result) <= 60); // Allow some margin
    }

    #[test]
    fn test_truncate_without_suffix() {
        let text = "This is a very long text ".repeat(100);
        let result = truncate_to_tokens(&text, 50, None).unwrap();
        assert!(!result.ends_with("..."));
        assert!(count_tokens(&result) <= 60);
    }

    #[test]
    fn test_truncate_zero_tokens() {
        let text = "Some text";
        let result = truncate_to_tokens(text, 0, None);
        assert!(result.is_err());
    }
}
