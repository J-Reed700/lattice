"""
Agentic RAG Types

Data classes and enums for advanced RAG patterns.

Basic Usage:
    >>> from src.modules.rag_engine.types import RAGMode, AgenticResult, RetrievalAssessment
    >>>
    >>> # Use enums for RAG modes
    >>> mode = RAGMode.SELF_RAG
    >>>
    >>> # Assessment results
    >>> assessment = RetrievalAssessment.HIGHLY_RELEVANT
"""

from dataclasses import dataclass, field
from enum import Enum
from typing import Any, Optional


class RAGMode(str, Enum):
    """RAG execution modes.

    STANDARD: Traditional RAG (retrieve + generate)
    SELF_RAG: Self-reflective RAG with quality assessment
    CRAG: Corrective RAG with fallback sources
    MULTI_STEP: Multi-step reasoning for complex questions
    DEEP_RESEARCH: Recursive research with adaptive query expansion
    ADAPTIVE: Automatically selects best mode based on question
    """

    STANDARD = "standard"
    SELF_RAG = "self_rag"
    CRAG = "crag"
    MULTI_STEP = "multi_step"
    DEEP_RESEARCH = "deep_research"
    ADAPTIVE = "adaptive"


class RetrievalAssessment(str, Enum):
    """Assessment of retrieval quality.

    Used by Self-RAG to evaluate if retrieved documents
    are relevant to the question.
    """

    HIGHLY_RELEVANT = "highly_relevant"
    RELEVANT = "relevant"
    PARTIALLY_RELEVANT = "partially_relevant"
    NOT_RELEVANT = "not_relevant"


class AnswerAssessment(str, Enum):
    """Assessment of answer quality.

    Used by Self-RAG to verify if generated answer
    is grounded in source documents.
    """

    FULLY_SUPPORTED = "fully_supported"
    PARTIALLY_SUPPORTED = "partially_supported"
    NOT_SUPPORTED = "not_supported"
    HALLUCINATION = "hallucination"


class DocumentQuality(str, Enum):
    """Overall document quality assessment.

    Used by CRAG to decide if fallback is needed.
    """

    HIGH = "high"
    MEDIUM = "medium"
    LOW = "low"


@dataclass
class SourceCitation:
    """Citation to a source document.

    Attributes:
        file_path: Path to source document
        snippet: Relevant excerpt
        score: Relevance score (0.0-1.0)
        citation_id: Citation number in answer [1], [2], etc.

    Example:
        >>> citation = SourceCitation(
        ...     file_path="C:\\docs\\ml.txt",
        ...     snippet="Machine learning is...",
        ...     score=0.92,
        ...     citation_id=1
        ... )
    """

    file_path: str
    snippet: str
    score: float
    citation_id: int


@dataclass
class ReasoningStep:
    """Single step in multi-step reasoning.

    Attributes:
        question: Sub-question being answered
        answer: Answer to sub-question
        sources: Documents used for this step
        confidence: Confidence in this step's answer

    Example:
        >>> step = ReasoningStep(
        ...     question="What is supervised learning?",
        ...     answer="Supervised learning uses labeled data...",
        ...     sources=[doc1, doc2],
        ...     confidence=0.85
        ... )
    """

    question: str
    answer: str
    sources: list[dict[str, Any]]
    confidence: float


@dataclass
class AgenticResult:
    """Result from agentic RAG execution.

    Comprehensive result including answer, verification info,
    sources, and execution metadata.

    Attributes:
        answer: Generated answer text
        mode: RAG mode used
        confidence: Overall confidence score (0.0-1.0)
        citations: List of source citations
        sources: Raw source documents
        iterations: Number of retrieval iterations
        reasoning_steps: Steps for multi-step reasoning (optional)
        retrieval_assessment: Quality of retrieval (optional)
        answer_assessment: Quality of answer (optional)
        fallback_used: Whether fallback source was used (CRAG)
        execution_time_ms: Total execution time
        metadata: Additional metadata

    Example:
        >>> result = AgenticResult(
        ...     answer="Machine learning is a branch of AI...",
        ...     mode=RAGMode.SELF_RAG,
        ...     confidence=0.92,
        ...     citations=[citation1, citation2],
        ...     sources=[doc1, doc2],
        ...     iterations=1,
        ...     answer_assessment=AnswerAssessment.FULLY_SUPPORTED,
        ...     execution_time_ms=1250
        ... )
    """

    answer: str
    mode: RAGMode
    confidence: float
    citations: list[SourceCitation] = field(default_factory=list)
    sources: list[dict[str, Any]] = field(default_factory=list)
    iterations: int = 1
    reasoning_steps: Optional[list[ReasoningStep]] = None
    retrieval_assessment: Optional[RetrievalAssessment] = None
    answer_assessment: Optional[AnswerAssessment] = None
    fallback_used: bool = False
    execution_time_ms: Optional[float] = None
    metadata: dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for serialization."""
        return {
            "answer": self.answer,
            "mode": self.mode.value,
            "confidence": self.confidence,
            "citations": [
                {
                    "file_path": c.file_path,
                    "snippet": c.snippet,
                    "score": c.score,
                    "citation_id": c.citation_id,
                }
                for c in self.citations
            ],
            "sources": self.sources,
            "iterations": self.iterations,
            "reasoning_steps": [
                {
                    "question": step.question,
                    "answer": step.answer,
                    "sources": step.sources,
                    "confidence": step.confidence,
                }
                for step in (self.reasoning_steps or [])
            ],
            "retrieval_assessment": self.retrieval_assessment.value
            if self.retrieval_assessment
            else None,
            "answer_assessment": self.answer_assessment.value
            if self.answer_assessment
            else None,
            "fallback_used": self.fallback_used,
            "execution_time_ms": self.execution_time_ms,
            "metadata": self.metadata,
        }


class RAGError(Exception):
    """Base exception for agentic RAG errors."""



class RetrievalFailedError(RAGError):
    """Failed to retrieve relevant documents after multiple attempts."""



class VerificationFailedError(RAGError):
    """Answer failed verification checks."""



class QueryDecompositionError(RAGError):
    """Failed to decompose complex query into sub-questions."""



__all__ = [
    "RAGMode",
    "RetrievalAssessment",
    "AnswerAssessment",
    "DocumentQuality",
    "SourceCitation",
    "ReasoningStep",
    "AgenticResult",
    "RAGError",
    "RetrievalFailedError",
    "VerificationFailedError",
    "QueryDecompositionError",
]
