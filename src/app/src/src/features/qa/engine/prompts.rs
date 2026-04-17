/// Prompt templates for Q&A
///
/// System and user prompt templates for RAG-based question answering.
/// Templates are designed for concise, accurate responses grounded in context.
/// System prompt that defines the assistant's behavior
pub const SYSTEM_PROMPT: &str = r#"You are a helpful assistant that answers questions based on provided context.

Your responsibilities:
- Answer questions accurately using ONLY the provided context
- If the context doesn't contain the answer, say "I don't have enough information to answer that question."
- Be concise and direct in your responses
- Include specific details and facts from the context when relevant
- Do not make up information or use knowledge outside the provided context
- Cite which documents you used when answering (by mentioning file names)

Guidelines:
- If multiple documents contain relevant information, synthesize them
- If the context is contradictory, point out the contradiction
- Format your answers clearly with proper structure when appropriate
- Use bullet points or numbered lists for multi-part answers"#;

/// Build user prompt with context and question
///
/// Formats the context and question into a structured prompt for the LLM.
///
/// # Arguments
/// * `context` - Formatted context string with document excerpts
/// * `question` - User's question
///
/// # Returns
/// Complete user prompt ready for the LLM
///
/// # Examples
/// ```
/// use vault_desktop::qa::build_user_prompt;
///
/// let context = "Python is a programming language...";
/// let question = "What is Python?";
/// let prompt = build_user_prompt(context, question);
/// assert!(prompt.contains("Context from relevant documents:"));
/// assert!(prompt.contains(question));
/// assert!(prompt.contains(context));
/// ```
pub fn build_user_prompt(context: &str, question: &str) -> String {
    format!(
        r#"Context from relevant documents:

{}

---

Question: {}

Please provide a concise answer based on the context above. If the context doesn't contain enough information to answer the question, please say so."#,
        context, question
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_prompt_not_empty() {
        assert!(!SYSTEM_PROMPT.is_empty());
        assert!(SYSTEM_PROMPT.contains("context"));
    }

    #[test]
    fn test_build_user_prompt() {
        let context = "Python is a high-level programming language.";
        let question = "What is Python?";
        let prompt = build_user_prompt(context, question);

        assert!(prompt.contains("Context from relevant documents:"));
        assert!(prompt.contains(context));
        assert!(prompt.contains(question));
        assert!(prompt.contains("---"));
    }

    #[test]
    fn test_build_user_prompt_empty_context() {
        let context = "";
        let question = "What is Rust?";
        let prompt = build_user_prompt(context, question);

        assert!(prompt.contains(question));
    }
}
