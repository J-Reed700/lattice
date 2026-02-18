"""
Tests for Agentic RAG service.
"""

import asyncio
from datetime import datetime
from unittest.mock import AsyncMock, Mock, patch
from uuid import uuid4

import pytest

from src.services.llm.agentic_rag import (
    AgentAction,
    AgenticRAGService,
    AgenticResponse,
    AgentPhase,
    ToolName,
)
from src.services.search.results import SearchResult, SearchResults


@pytest.fixture()
def mock_search_service():
    """Mock search service."""
    service = Mock()
    service.search = AsyncMock()
    return service


@pytest.fixture()
def mock_ollama_service():
    """Mock Ollama service."""
    service = Mock()
    service.generate_text = AsyncMock()
    service.health_check = AsyncMock(return_value=True)
    return service


@pytest.fixture()
def agentic_service(mock_search_service, mock_ollama_service):
    """Create agentic RAG service with mocks."""
    return AgenticRAGService(
        search_service=mock_search_service,
        ollama_service=mock_ollama_service,
        model="llama2",
        max_iterations=5,
        timeout=60,
    )


class TestAgenticRAGBasic:
    """Basic functionality tests."""

    @pytest.mark.asyncio()
    async def test_service_initialization(self, agentic_service):
        """Test service initializes correctly."""
        assert agentic_service.max_iterations == 5
        assert agentic_service.timeout == 60
        assert agentic_service.enable_reflection is True
        assert agentic_service.enable_correction is True

    @pytest.mark.asyncio()
    async def test_planning_phase(self, agentic_service, mock_ollama_service):
        """Test planning phase generates thoughts."""
        mock_ollama_service.generate_text.return_value = (
            "I need to search for Q3 and Q4 financial reports to compare them."
        )

        planning = await agentic_service._planning_phase("Compare Q3 and Q4 reports")

        assert "Q3" in planning
        assert "Q4" in planning
        mock_ollama_service.generate_text.assert_called_once()

    @pytest.mark.asyncio()
    async def test_action_parsing(self, agentic_service):
        """Test parsing of action responses."""
        response = """
        THOUGHT: I need to search for financial reports
        ACTION: search_documents
        INPUT: {"query": "Q3 financial report"}
        """

        parsed = agentic_service._parse_action_response(response)

        assert "financial reports" in parsed["thought"]
        assert parsed["tool"] == ToolName.SEARCH_DOCUMENTS
        assert parsed["input"]["query"] == "Q3 financial report"

    @pytest.mark.asyncio()
    async def test_finish_action_parsing(self, agentic_service):
        """Test parsing of finish action."""
        response = """
        THOUGHT: I have enough information to answer
        ACTION: finish
        INPUT: {}
        """

        parsed = agentic_service._parse_action_response(response)

        assert parsed["tool"] == ToolName.FINISH

    @pytest.mark.asyncio()
    async def test_reflection_parsing_needs_correction(self, agentic_service):
        """Test reflection parsing when correction is needed."""
        reflection = """
        EVALUATION: Answer is incomplete
        QUALITY_SCORE: 6
        NEEDS_CORRECTION: yes
        ISSUES: Missing industry context
        """

        needs_correction = agentic_service._parse_reflection_needs_correction(reflection)

        assert needs_correction is True

    @pytest.mark.asyncio()
    async def test_reflection_parsing_no_correction(self, agentic_service):
        """Test reflection parsing when no correction needed."""
        reflection = """
        EVALUATION: Answer is comprehensive
        QUALITY_SCORE: 9
        NEEDS_CORRECTION: no
        ISSUES: None
        """

        needs_correction = agentic_service._parse_reflection_needs_correction(reflection)

        assert needs_correction is False


class TestAgenticRAGTools:
    """Test tool execution."""

    @pytest.mark.asyncio()
    async def test_search_documents_tool(self, agentic_service, mock_search_service):
        """Test search documents tool execution."""
        mock_results = [
            SearchResult(
                id=uuid4(),
                file_path="/docs/test.pdf",
                filename="test.pdf",
                mime_type="application/pdf",
                size_bytes=1000,
                modified_at=datetime.utcnow(),
                score=0.9,
                snippet="Test content",
            )
        ]
        mock_search_service.search.return_value = SearchResults.from_results(
            results=mock_results,
            total=1,
            offset=0,
            limit=5,
        )

        action = AgentAction(tool=ToolName.SEARCH_DOCUMENTS, tool_input={"query": "test query"})

        observation = await agentic_service._execute_tool(
            action=action, search_mode="hybrid", filters=None
        )

        assert observation.success is True
        assert len(observation.result) == 1
        assert observation.result[0].filename == "test.pdf"
        mock_search_service.search.assert_called_once()

    @pytest.mark.asyncio()
    async def test_reformulate_query_tool(
        self, agentic_service, mock_ollama_service, mock_search_service
    ):
        """Test query reformulation tool."""
        mock_ollama_service.generate_text.return_value = "improved search query"
        mock_search_service.search.return_value = SearchResults.from_results(
            results=[],
            total=0,
            offset=0,
            limit=5,
        )

        action = AgentAction(tool=ToolName.REFORMULATE_QUERY, tool_input={"original": "bad query"})

        observation = await agentic_service._execute_tool(
            action=action, search_mode="hybrid", filters=None
        )

        assert observation.success is True
        mock_ollama_service.generate_text.assert_called_once()
        assert "bad query" in mock_ollama_service.generate_text.call_args[1]["prompt"]


class TestAgenticRAGEndToEnd:
    """End-to-end integration tests."""

    @pytest.mark.asyncio()
    async def test_simple_question_flow(
        self, agentic_service, mock_ollama_service, mock_search_service
    ):
        """Test complete flow for simple question."""
        mock_ollama_service.generate_text.side_effect = [
            "I need to search for the budget document",
            'THOUGHT: Search for Q4 budget\nACTION: search_documents\nINPUT: {"query": "Q4 budget"}',
            "THOUGHT: I have the information\nACTION: finish\nINPUT: {}",
            "The Q4 budget is $2.5M [Source 1]",
            "EVALUATION: Complete\nQUALITY_SCORE: 9\nNEEDS_CORRECTION: no",
        ]

        mock_results = [
            SearchResult(
                id=uuid4(),
                file_path="/docs/budget.pdf",
                filename="budget.pdf",
                mime_type="application/pdf",
                size_bytes=1000,
                modified_at=datetime.utcnow(),
                score=0.95,
                snippet="Q4 budget is $2.5M",
            )
        ]
        mock_search_service.search.return_value = SearchResults.from_results(
            results=mock_results,
            total=1,
            offset=0,
            limit=5,
        )

        response = await agentic_service.ask_question(
            question="What is the Q4 budget?",
            search_mode="hybrid",
        )

        assert response.answer == "The Q4 budget is $2.5M [Source 1]"
        assert len(response.sources) == 1
        assert response.corrected is False
        assert response.total_iterations > 0

    @pytest.mark.asyncio()
    async def test_complex_question_with_correction(
        self, agentic_service, mock_ollama_service, mock_search_service
    ):
        """Test flow with reflection and correction."""
        mock_ollama_service.generate_text.side_effect = [
            "I need to search for both Q3 and Q4 reports",
            'THOUGHT: Search Q3\nACTION: search_documents\nINPUT: {"query": "Q3 report"}',
            'THOUGHT: Search Q4\nACTION: search_documents\nINPUT: {"query": "Q4 report"}',
            "THOUGHT: Have enough info\nACTION: finish\nINPUT: {}",
            "Q3 revenue was $5M and Q4 was $6M",
            "EVALUATION: Missing growth rate\nQUALITY_SCORE: 6\nNEEDS_CORRECTION: yes",
            "Q3 revenue was $5M and Q4 was $6M, representing 20% growth",
        ]

        mock_results = [
            SearchResult(
                id=uuid4(),
                file_path=f"/docs/report{i}.pdf",
                filename=f"report{i}.pdf",
                mime_type="application/pdf",
                size_bytes=1000,
                modified_at=datetime.utcnow(),
                score=0.9,
                snippet=f"Report {i} content",
            )
            for i in range(2)
        ]
        mock_search_service.search.return_value = SearchResults.from_results(
            results=mock_results,
            total=2,
            offset=0,
            limit=5,
        )

        response = await agentic_service.ask_question(
            question="Compare Q3 and Q4 revenue",
            search_mode="hybrid",
        )

        assert "20% growth" in response.answer
        assert response.corrected is True
        assert response.reflection is not None
        assert len(response.sources) > 0

    @pytest.mark.asyncio()
    async def test_max_iterations_respected(
        self, agentic_service, mock_ollama_service, mock_search_service
    ):
        """Test that max iterations limit is respected."""
        mock_ollama_service.generate_text.side_effect = (
            [
                "Planning thoughts",
            ]
            + [
                'THOUGHT: Keep searching\nACTION: search_documents\nINPUT: {"query": "test"}'
                for _ in range(10)
            ]
            + [
                "Final answer",
                "EVALUATION: Good\nQUALITY_SCORE: 8\nNEEDS_CORRECTION: no",
            ]
        )

        mock_search_service.search.return_value = SearchResults.from_results(
            results=[],
            total=0,
            offset=0,
            limit=5,
        )

        response = await agentic_service.ask_question(
            question="Test question",
            search_mode="hybrid",
        )

        action_steps = [
            step for step in response.reasoning_steps if step.thought.phase == AgentPhase.ACTION
        ]
        assert len(action_steps) <= agentic_service.max_iterations

    @pytest.mark.asyncio()
    async def test_timeout_handling(self, mock_search_service, mock_ollama_service):
        """Test timeout is enforced."""
        service = AgenticRAGService(
            search_service=mock_search_service,
            ollama_service=mock_ollama_service,
            timeout=1,
        )

        async def slow_generate(*args, **kwargs):
            await asyncio.sleep(2)
            return "result"

        mock_ollama_service.generate_text.side_effect = slow_generate

        with pytest.raises(Exception):
            await service.ask_question("Test question")


class TestAgenticRAGStreaming:
    """Test streaming functionality."""

    @pytest.mark.asyncio()
    async def test_streaming_events(
        self, agentic_service, mock_ollama_service, mock_search_service
    ):
        """Test streaming produces correct events."""
        mock_ollama_service.generate_text.side_effect = [
            "Planning",
            'THOUGHT: Search\nACTION: search_documents\nINPUT: {"query": "test"}',
            "THOUGHT: Done\nACTION: finish\nINPUT: {}",
            "Answer text",
            "EVALUATION: Good\nQUALITY_SCORE: 9\nNEEDS_CORRECTION: no",
        ]

        mock_search_service.search.return_value = SearchResults.from_results(
            results=[],
            total=0,
            offset=0,
            limit=5,
        )

        events = []
        async for event in agentic_service.ask_question_stream("Test question"):
            events.append(event)

        event_types = [e["type"] for e in events]
        assert "status" in event_types
        assert "thought" in event_types
        assert "action" in event_types
        assert "answer" in event_types
        assert "complete" in event_types


class TestAgenticRAGComparison:
    """Test comparison with simple RAG."""

    @pytest.mark.asyncio()
    async def test_performance_characteristics(self, agentic_service):
        """Test that agentic RAG has expected performance characteristics."""
        with patch.object(agentic_service, "ask_question") as mock_ask:
            mock_ask.return_value = AgenticResponse(
                answer="Test answer",
                reasoning_steps=[],
                sources=[],
                reflection=None,
                corrected=False,
                total_iterations=3,
                total_time_ms=8000,
                metadata={},
            )

            response = await agentic_service.ask_question("Test")

            assert response.total_time_ms > 5000
            assert response.total_iterations >= 3


class TestAgenticRAGPrompts:
    """Test prompt generation."""

    def test_planning_prompt_includes_question(self):
        """Test planning prompt includes the question."""
        from src.services.llm.agentic_rag import AgenticRAGPrompts

        prompt = AgenticRAGPrompts.create_planning_prompt("What is the budget?")

        assert "What is the budget?" in prompt
        assert "plan" in prompt.lower() or "think" in prompt.lower()

    def test_action_prompt_includes_tools(self):
        """Test action prompt includes available tools."""
        from src.services.llm.agentic_rag import AgenticRAGPrompts

        prompt = AgenticRAGPrompts.create_action_decision_prompt(
            question="Test", planning_thoughts="Test plan", previous_observations=[], iteration=1
        )

        assert "search_documents" in prompt
        assert "finish" in prompt
        assert "THOUGHT:" in prompt
        assert "ACTION:" in prompt

    def test_reflection_prompt_structure(self):
        """Test reflection prompt has correct structure."""
        from src.services.llm.agentic_rag import AgenticRAGPrompts

        prompt = AgenticRAGPrompts.create_reflection_prompt(
            question="Test question", answer="Test answer", sources_count=3
        )

        assert "EVALUATION" in prompt
        assert "QUALITY_SCORE" in prompt
        assert "NEEDS_CORRECTION" in prompt


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
