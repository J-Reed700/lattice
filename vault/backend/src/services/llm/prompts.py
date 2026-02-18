"""RAG prompts for LLM Q&A.

NOTE: This module re-exports shared prompt utilities from src.utils.prompts.
Kept for backward compatibility. Additional prompts (like create_followup_prompt)
remain here as they are service-specific extensions.
"""

from src.utils.prompts import (
    create_context_from_results,
    create_rag_prompt,
    create_rag_system_prompt,
)

__all__ = [
    "create_context_from_results",
    "create_rag_prompt",
    "create_rag_system_prompt",
    "create_followup_prompt",
]


def create_followup_prompt(question: str, previous_answer: str, context: str) -> str:
    """
    Create prompt for follow-up questions with conversation history.

    Args:
        question: New follow-up question
        previous_answer: Previous answer from the assistant
        context: Updated context from new search

    Returns:
        Prompt with conversation history
    """
    return f"""You are a helpful AI assistant continuing a conversation about the user's documents.

Previous Answer:
{previous_answer}

Updated Context:
{context}

Follow-up Question: {question}

Instructions:
- Build on the previous conversation
- Use the updated context to provide more detailed information
- Cite sources using [Source N] notation
- Be concise and accurate

Answer:"""
