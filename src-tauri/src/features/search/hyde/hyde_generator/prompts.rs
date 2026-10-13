//! Prompt templates for HyDE expansion, follow-up classification and web-query rewriting.

/// Prompt template for question expansion via HyDE.
///
/// Instructs the LLM to expand intent and retrieval vocabulary without
/// providing an answer.
pub(super) const QUESTION_HYDE_TEMPLATE: &str = r#"You are an expert assistant helping with document retrieval.

Your task: Rewrite and expand the user question into retrieval-oriented text that captures intent and likely terminology found in relevant documents.

Guidelines:
- Write 2-4 short sentences (max 90 words)
- Preserve key entities, places, dates, and constraints from the question
- Add closely related terms, aliases, and corrected spellings when helpful
- Focus on what relevant documents would discuss, not the final answer
- Do NOT answer the question directly
- Do NOT assert facts that are not explicitly present in the query
- Do NOT use first-person narration (no "I", "we")
- Do NOT explain what you're doing; output only the expansion text

Question: {query}

Expansion:"#;

/// Prompt template for follow-up query expansion with conversation context.
///
/// Instructs the LLM to resolve references in the follow-up against recent
/// conversation turns, then emit retrieval-oriented expansion text.
pub(super) const FOLLOWUP_HYDE_TEMPLATE: &str = r#"You are an expert assistant helping with document retrieval.

Your task: Infer what the follow-up question is referring to using recent conversation context, then rewrite it into retrieval-oriented text.

Guidelines:
- Resolve pronouns and references ("those", "it", "they", "that") using context
- Preserve key entities, compounds, nutrients, and constraints already discussed
- Write 2-4 short sentences (max 90 words)
- Focus on terms likely to appear in relevant documents
- Do NOT answer the question directly
- Do NOT invent facts not present in the follow-up or context
- Output only the expansion text

Recent conversation context:
{context}

Follow-up question:
{query}

Expansion:"#;

/// Prompt template for command interpretation.
///
/// Expands a command into a description of what documents would be relevant.
pub(super) const COMMAND_HYDE_TEMPLATE: &str = r#"You are an expert assistant helping with document retrieval.

Your task: Describe what kind of documents would be relevant for this command. Write 2-3 sentences describing the target documents.

Guidelines:
- Focus on document characteristics, not actions
- Use terminology that would appear in relevant documents
- Be specific and concrete
- Do NOT explain what you're doing, just write the description

Command: {query}

Description:"#;

/// Prompt template for contextual follow-up classification.
///
/// The model must output only `1` (context-dependent follow-up) or `0`
/// (standalone query).
pub(super) const FOLLOWUP_CLASSIFIER_TEMPLATE: &str = r#"You are classifying whether a user query depends on previous conversation context.

Return exactly one character:
- 1 = context-dependent follow-up
- 0 = standalone query

Context:
{context}

Query:
{query}

Answer:"#;

/// Prompt template for web search query generation.
///
/// Produces one query string suitable for a web search bar.
///
/// The length rule and the rule against copying out lists are what keep it a
/// query. Unbounded, the model retells the question — and a question a reader
/// spent a paragraph on becomes a paragraph-long "query" that matches nothing.
/// The worked example teaches the same thing by showing it, which a small
/// utility model follows more reliably than it follows a rule.
pub(super) const WEB_SEARCH_QUERY_TEMPLATE: &str = r#"You are rewriting user input into a web search query.

Task:
- Return exactly ONE search query line.
- Use 3 to 8 words. A search engine handed a sentence matches nothing.
- Keep key entities, exact names, and important dates.
- Do not copy out a list of items the input names. Search for what is being
  asked about them.
- Remove filler like "go into detail" or "please explain".
- Prefer specific terms over generic wording.
- Do not answer the question.
- Output only the query text.

Example input: I have read The Hobbit, The Lord of the Rings and The Silmarillion, what else should I read?
Example search query: fantasy novels similar to Tolkien

User input:
{query}

Search query:"#;

/// Prompt template for contextual web search query generation.
///
/// Uses recent context to resolve short follow-ups and generic commands.
///
/// Same rules as [`WEB_SEARCH_QUERY_TEMPLATE`], and they matter more here: the
/// context runs to thousands of characters, and a model asked to rewrite a
/// follow-up against it will happily copy whatever the reader enumerated back
/// out as the query. A reader who lists what they already have is asking about
/// what they do not have, so the list is the one part not to search for.
pub(super) const WEB_SEARCH_QUERY_WITH_CONTEXT_TEMPLATE: &str = r#"You are rewriting user input into a web search query.

Task:
- Return exactly ONE search query line for a search engine.
- Use 3 to 8 words. A search engine handed a sentence matches nothing.
- If the latest user input is short or generic (for example "search web"),
  infer the topic from recent conversation context.
- Keep key entities, exact names, and important dates.
- Do not copy out a list of items the input or the context names. Search for
  what is being asked about them.
- Remove filler and conversational wording.
- Do not answer the question.
- Output only the query text.

Example input: I have read The Hobbit, The Lord of the Rings and The Silmarillion, what else should I read?
Example search query: fantasy novels similar to Tolkien

Recent conversation context:
{context}

Latest user input:
{query}

Search query:"#;

/// Prompt template for deep-research follow-up searches.
///
/// The follow-ups used to be mined from the first search's snippets: the words
/// several results shared, two at a time, bolted onto the query's own words.
/// What several snippets share is the pages' furniture as often as their
/// subject, so they went out as "steps greens garden step learn" and "steps
/// greens garden beginner right". Choosing another angle on a subject takes
/// understanding it, so the utility model writes them from the same reading of
/// the turn the main query came from.
pub(super) const RESEARCH_FOLLOWUPS_TEMPLATE: &str = r#"You are planning web searches for a research question.

The main search is already written. Write {count} MORE searches that each look
at a different part of the same subject — the other things the reader would
need to know to act on it.

Rules:
- One search per line. No numbering, bullets, quotes or commentary.
- Each search is 3 to 8 words, written the way a person types into a search bar.
- Every search names the subject, so it makes sense on its own.
- Each covers a different aspect: for example timing, cost, problems,
  comparisons, how-to, or requirements. Do not reword the main search.
- Never use words about the conversation or the request itself, such as
  "attached", "chat", "file", "previous", "research", "next steps" or "list".
- Do not answer the question.

Example main search: fantasy novels similar to Tolkien
Example searches:
epic fantasy series with invented languages
best standalone fantasy novels for adults
classic mythology retellings like The Silmarillion

Recent conversation context:
{context}

Latest user input:
{query}

Main search: {root}
Searches:"#;
