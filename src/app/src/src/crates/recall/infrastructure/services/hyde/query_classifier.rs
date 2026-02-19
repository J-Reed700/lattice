use crate::domain::qa::hyde::QueryType;
use lazy_regex::regex;

// Compile-time validated regexes using lazy_regex (eliminates runtime panics)
fn greeting_patterns() -> &'static regex::Regex {
    regex!(r"(?i)^(hi|hello|hey|greetings|good\s+(morning|afternoon|evening))[\s!?]*$")
}

fn question_patterns() -> &'static regex::Regex {
    regex!(r"(?i)(what|how|why|when|where|who|which|can you|could you|explain|tell me)")
}

fn command_patterns() -> &'static regex::Regex {
    regex!(r"(?i)^(search|find|show|list|index|delete|remove|open|create|update)")
}

pub struct QueryClassifier;

impl QueryClassifier {
    pub fn new() -> Self {
        Self
    }

    pub fn classify(&self, query: &str) -> QueryType {
        let trimmed = query.trim();

        // Greeting detection - must be exact match at start
        if greeting_patterns().is_match(trimmed) {
            return QueryType::Greeting;
        }

        // Command detection - starts with command verb
        if command_patterns().is_match(trimmed) {
            return QueryType::Command;
        }

        // Question detection - contains question words
        if question_patterns().is_match(trimmed) {
            return QueryType::Question;
        }

        // Default to Question for natural language queries
        QueryType::Question
    }
}

impl Default for QueryClassifier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_greeting_detection() {
        let classifier = QueryClassifier::new();

        // Should detect greetings
        assert_eq!(classifier.classify("hi"), QueryType::Greeting);
        assert_eq!(classifier.classify("hello"), QueryType::Greeting);
        assert_eq!(classifier.classify("hey"), QueryType::Greeting);
        assert_eq!(classifier.classify("Hi!"), QueryType::Greeting);
        assert_eq!(classifier.classify("Hello?"), QueryType::Greeting);
        assert_eq!(classifier.classify("good morning"), QueryType::Greeting);
        assert_eq!(classifier.classify("good afternoon"), QueryType::Greeting);
        assert_eq!(classifier.classify("good evening"), QueryType::Greeting);
        assert_eq!(classifier.classify("  hello  "), QueryType::Greeting);

        // Should not detect as greeting (has additional content)
        assert_ne!(classifier.classify("hello world"), QueryType::Greeting);
        assert_ne!(classifier.classify("hi, how are you?"), QueryType::Greeting);
    }

    #[test]
    fn test_question_detection() {
        let classifier = QueryClassifier::new();

        // Should detect questions
        assert_eq!(classifier.classify("what is rust?"), QueryType::Question);
        assert_eq!(
            classifier.classify("how do I use this?"),
            QueryType::Question
        );
        assert_eq!(
            classifier.classify("why is the sky blue?"),
            QueryType::Question
        );
        assert_eq!(
            classifier.classify("when was this created?"),
            QueryType::Question
        );
        assert_eq!(
            classifier.classify("where is the file?"),
            QueryType::Question
        );
        assert_eq!(classifier.classify("who wrote this?"), QueryType::Question);
        assert_eq!(
            classifier.classify("which one is better?"),
            QueryType::Question
        );
        assert_eq!(classifier.classify("can you help me?"), QueryType::Question);
        assert_eq!(
            classifier.classify("could you explain this?"),
            QueryType::Question
        );
        assert_eq!(
            classifier.classify("explain machine learning"),
            QueryType::Question
        );
        assert_eq!(
            classifier.classify("tell me about rust"),
            QueryType::Question
        );
    }

    #[test]
    fn test_command_detection() {
        let classifier = QueryClassifier::new();

        // Should detect commands
        assert_eq!(
            classifier.classify("search for documents"),
            QueryType::Command
        );
        assert_eq!(classifier.classify("find all PDFs"), QueryType::Command);
        assert_eq!(classifier.classify("show recent files"), QueryType::Command);
        assert_eq!(classifier.classify("list all tags"), QueryType::Command);
        assert_eq!(
            classifier.classify("index this directory"),
            QueryType::Command
        );
        assert_eq!(
            classifier.classify("delete old documents"),
            QueryType::Command
        );
        assert_eq!(classifier.classify("remove duplicates"), QueryType::Command);
        assert_eq!(classifier.classify("open file.txt"), QueryType::Command);
        assert_eq!(
            classifier.classify("create new document"),
            QueryType::Command
        );
        assert_eq!(classifier.classify("update metadata"), QueryType::Command);
    }

    #[test]
    fn test_default_to_question() {
        let classifier = QueryClassifier::new();

        // Ambiguous or natural language queries default to Question
        assert_eq!(
            classifier.classify("machine learning algorithms"),
            QueryType::Question
        );
        assert_eq!(classifier.classify("rust programming"), QueryType::Question);
        assert_eq!(
            classifier.classify("the quick brown fox"),
            QueryType::Question
        );
    }

    #[test]
    fn test_case_insensitivity() {
        let classifier = QueryClassifier::new();

        assert_eq!(classifier.classify("HELLO"), QueryType::Greeting);
        assert_eq!(classifier.classify("HeLLo"), QueryType::Greeting);
        assert_eq!(classifier.classify("WHAT is this?"), QueryType::Question);
        assert_eq!(classifier.classify("SEARCH documents"), QueryType::Command);
    }

    #[test]
    fn test_whitespace_handling() {
        let classifier = QueryClassifier::new();

        assert_eq!(classifier.classify("  hi  "), QueryType::Greeting);
        assert_eq!(
            classifier.classify("  what is this?  "),
            QueryType::Question
        );
        assert_eq!(classifier.classify("  search files  "), QueryType::Command);
    }
}
