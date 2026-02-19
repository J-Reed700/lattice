"""
API schemas for Agentic RAG endpoints.
"""

from __future__ import annotations

from datetime import datetime
from typing import Any, Literal

from pydantic import BaseModel, Field


class AgenticQueryRequest(BaseModel):
    """Request schema for agentic RAG query."""

    query: str = Field(
        ..., min_length=1, max_length=2000, description="The question to answer using agentic RAG"
    )

    search_mode: Literal["vector", "text", "hybrid"] = Field(
        default="hybrid", description="Search mode for document retrieval"
    )

    mode: Literal["adaptive", "standard", "self_rag", "crag", "multi_step", "deep_research"] = (
        Field(
            default="adaptive",
            description="RAG mode. Use deep_research for recursive multi-hop web+local research.",
        )
    )

    max_iterations: int = Field(default=5, ge=1, le=10, description="Maximum reasoning iterations")

    max_depth: int = Field(
        default=2,
        ge=1,
        le=4,
        description="Maximum recursion depth for deep_research mode",
    )

    branch_factor: int = Field(
        default=3,
        ge=1,
        le=6,
        description="Max number of parallel sub-questions/queries per deep_research step",
    )

    target_confidence: float = Field(
        default=0.82,
        ge=0.3,
        le=0.99,
        description="Stop deep_research early once confidence meets this threshold",
    )

    min_marginal_gain: float = Field(
        default=0.05,
        ge=0.0,
        le=0.5,
        description="Stop deep_research when expected gain falls below this threshold",
    )

    time_budget_seconds: int = Field(
        default=120,
        ge=30,
        le=600,
        description="Maximum wall-clock time budget for deep_research mode",
    )

    enable_reflection: bool = Field(default=True, description="Enable self-reflection phase")

    enable_correction: bool = Field(default=True, description="Enable self-correction phase")

    streaming: bool = Field(default=False, description="Stream the reasoning process and answer")

    temperature: float = Field(
        default=0.7, ge=0.0, le=2.0, description="Sampling temperature for generation"
    )

    model: str | None = Field(default=None, description="Override default model")

    class Config:
        json_schema_extra = {
            "example": {
                "query": "What are the main differences between the Q3 and Q4 financial reports?",
                "search_mode": "hybrid",
                "mode": "deep_research",
                "max_iterations": 5,
                "max_depth": 2,
                "branch_factor": 3,
                "target_confidence": 0.82,
                "min_marginal_gain": 0.05,
                "time_budget_seconds": 120,
                "enable_reflection": True,
                "enable_correction": True,
                "streaming": False,
                "temperature": 0.7,
            }
        }


class ReasoningStepResponse(BaseModel):
    """A single step in the reasoning chain."""

    phase: str = Field(..., description="Phase of reasoning (planning, action, etc.)")
    thought: str | None = Field(None, description="Agent's thought/reasoning")
    action: str | None = Field(None, description="Action taken (tool name)")
    action_input: dict[str, Any] | None = Field(None, description="Input to the action")
    observation: str | None = Field(None, description="Result of the action")
    iteration: int = Field(..., description="Iteration number")
    timestamp: datetime = Field(..., description="When this step occurred")


class SourceDocumentResponse(BaseModel):
    """Source document used in answer generation."""

    file_path: str = Field(..., description="Path to the file")
    filename: str = Field(..., description="Name of the file")
    score: float = Field(..., description="Relevance score")
    snippet: str | None = Field(None, description="Relevant snippet from document")


class AgenticQueryResponse(BaseModel):
    """Response schema for agentic RAG query."""

    answer: str = Field(..., description="Final answer to the question")

    reasoning_steps: list[ReasoningStepResponse] = Field(
        ..., description="Complete reasoning chain showing agent's thought process"
    )

    sources: list[SourceDocumentResponse] = Field(
        ..., description="Source documents used in the answer"
    )

    reflection: str | None = Field(None, description="Agent's self-reflection on answer quality")

    corrected: bool = Field(False, description="Whether the answer was corrected after reflection")

    total_iterations: int = Field(..., description="Total number of reasoning iterations")

    total_time_ms: int = Field(..., description="Total processing time in milliseconds")

    metadata: dict[str, Any] = Field(
        default_factory=dict, description="Additional metadata about the response"
    )

    class Config:
        json_schema_extra = {
            "example": {
                "answer": "Based on the financial reports, the main differences between Q3 and Q4 are...",
                "reasoning_steps": [
                    {
                        "phase": "planning",
                        "thought": "I need to find both Q3 and Q4 financial reports",
                        "action": None,
                        "observation": None,
                        "iteration": 0,
                        "timestamp": "2024-01-01T12:00:00",
                    }
                ],
                "sources": [
                    {
                        "file_path": "/documents/q3_report.pdf",
                        "filename": "q3_report.pdf",
                        "score": 0.92,
                        "snippet": "Q3 2024 revenue was $5.2M...",
                    }
                ],
                "reflection": "The answer covers the key differences comprehensively...",
                "corrected": False,
                "total_iterations": 3,
                "total_time_ms": 8500,
                "metadata": {"model": "llama2", "search_mode": "hybrid"},
            }
        }


class AgenticStreamEvent(BaseModel):
    """Event emitted during streaming agentic RAG."""

    type: Literal[
        "status",
        "thought",
        "action",
        "observation",
        "answer",
        "reflection",
        "correction",
        "complete",
        "error",
    ] = Field(..., description="Type of event")

    phase: str | None = Field(None, description="Current phase")
    content: str | None = Field(None, description="Event content")
    iteration: int | None = Field(None, description="Current iteration")
    metadata: dict[str, Any] | None = Field(None, description="Additional metadata")

    class Config:
        json_schema_extra = {
            "example": {
                "type": "thought",
                "phase": "planning",
                "content": "I need to search for Q3 and Q4 reports...",
                "iteration": 1,
                "metadata": {},
            }
        }


class AgenticComparisonRequest(BaseModel):
    """Request to compare simple RAG vs agentic RAG."""

    query: str = Field(
        ..., min_length=1, max_length=2000, description="Question to test with both approaches"
    )

    search_mode: Literal["vector", "text", "hybrid"] = Field(
        default="hybrid", description="Search mode for both approaches"
    )

    class Config:
        json_schema_extra = {
            "example": {
                "query": "What are the key findings in the research papers about AI?",
                "search_mode": "hybrid",
            }
        }


class ComparisonResult(BaseModel):
    """Result from one RAG approach."""

    answer: str = Field(..., description="Generated answer")
    time_ms: int = Field(..., description="Processing time")
    sources_count: int = Field(..., description="Number of sources used")
    metadata: dict[str, Any] = Field(default_factory=dict, description="Additional info")


class AgenticComparisonResponse(BaseModel):
    """Response comparing simple vs agentic RAG."""

    query: str = Field(..., description="The test query")

    simple_rag: ComparisonResult = Field(..., description="Results from simple RAG")

    agentic_rag: ComparisonResult = Field(..., description="Results from agentic RAG")

    comparison: dict[str, Any] = Field(..., description="Comparison metrics and analysis")

    class Config:
        json_schema_extra = {
            "example": {
                "query": "What are the key findings?",
                "simple_rag": {
                    "answer": "The key findings are...",
                    "time_ms": 1500,
                    "sources_count": 3,
                    "metadata": {},
                },
                "agentic_rag": {
                    "answer": "Based on comprehensive analysis...",
                    "time_ms": 4500,
                    "sources_count": 5,
                    "metadata": {"iterations": 3, "corrected": True},
                },
                "comparison": {
                    "time_multiplier": 3.0,
                    "additional_sources": 2,
                    "more_comprehensive": True,
                },
            }
        }
