"""
Prompt Templates for Q&A

System and user prompt templates for RAG-based question answering.
Templates are designed for concise, accurate responses grounded in context.

Basic Usage:
    >>> from src.modules.rag_engine.qa.prompts import SYSTEM_PROMPT, USER_PROMPT_TEMPLATE
    >>> context = "The Eiffel Tower is in Paris, France."
    >>> question = "Where is the Eiffel Tower?"
    >>> prompt = USER_PROMPT_TEMPLATE.format(context=context, question=question)
"""

from __future__ import annotations

SYSTEM_PROMPT = """You are a helpful assistant that answers questions based on provided context.

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
- Use bullet points or numbered lists for multi-part answers"""


USER_PROMPT_TEMPLATE = """Context from relevant documents:

{context}

---

Question: {question}

Please provide a concise answer based on the context above. If the context doesn't contain enough information to answer the question, please say so."""


__all__ = ["SYSTEM_PROMPT", "USER_PROMPT_TEMPLATE"]
