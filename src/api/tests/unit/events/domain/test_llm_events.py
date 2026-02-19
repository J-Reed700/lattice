"""Unit tests for LLM-related domain events."""

from datetime import datetime

import pytest
from pydantic import ValidationError

from src.events.domain.llm_events import (
    LLMCacheInvalidated,
    LLMHealthCheckPerformed,
    LLMModelsListed,
    QuestionAnswered,
    QuestionAnsweringFailed,
    QuestionAsked,
)


class TestQuestionAsked:
    """Test QuestionAsked event validation and serialization."""

    def test_valid_event_creation(self) -> None:
        """Test creating a valid QuestionAsked event."""
        event = QuestionAsked(
            question="What is machine learning?",
            user_id="user123",
            search_mode="hybrid",
            max_context_docs=5,
            model="llama2",
        )

        assert event.question == "What is machine learning?"
        assert event.user_id == "user123"
        assert event.search_mode == "hybrid"
        assert event.max_context_docs == 5
        assert event.model == "llama2"
        assert isinstance(event.timestamp, datetime)

    def test_event_with_metadata(self) -> None:
        """Test event with additional metadata."""
        metadata = {"temperature": 0.7, "max_tokens": 2000}
        event = QuestionAsked(
            question="Test?",
            metadata=metadata,
        )

        assert event.metadata == metadata

    def test_question_validation_empty(self) -> None:
        """Test that empty question is rejected."""
        with pytest.raises(ValidationError) as exc_info:
            QuestionAsked(question="")

        errors = exc_info.value.errors()
        # Pydantic first checks min_length before validator runs
        assert any("question" in str(error.get("loc", [])) for error in errors)

    def test_question_validation_whitespace(self) -> None:
        """Test that whitespace-only question is rejected."""
        with pytest.raises(ValidationError) as exc_info:
            QuestionAsked(question="   ")

        errors = exc_info.value.errors()
        assert any("Question must not be empty" in str(error) for error in errors)

    def test_question_trimming(self) -> None:
        """Test that question whitespace is trimmed."""
        event = QuestionAsked(question="  What is ML?  ")
        assert event.question == "What is ML?"

    def test_question_max_length(self) -> None:
        """Test that question exceeding max length is rejected."""
        long_question = "a" * 2001
        with pytest.raises(ValidationError) as exc_info:
            QuestionAsked(question=long_question)

        errors = exc_info.value.errors()
        assert any("max_length" in str(error) for error in errors)

    def test_search_mode_validation(self) -> None:
        """Test that invalid search_mode is rejected."""
        with pytest.raises(ValidationError) as exc_info:
            QuestionAsked(question="Test?", search_mode="invalid")

        errors = exc_info.value.errors()
        assert any("search_mode" in str(error) for error in errors)

    def test_all_search_modes_valid(self) -> None:
        """Test that all valid search modes are accepted."""
        for mode in ["vector", "text", "hybrid"]:
            event = QuestionAsked(question="Test?", search_mode=mode)
            assert event.search_mode == mode

    def test_max_context_docs_validation(self) -> None:
        """Test max_context_docs range validation."""
        # Below minimum
        with pytest.raises(ValidationError):
            QuestionAsked(question="Test?", max_context_docs=0)

        # Above maximum
        with pytest.raises(ValidationError):
            QuestionAsked(question="Test?", max_context_docs=51)

        # Valid values
        event1 = QuestionAsked(question="Test?", max_context_docs=1)
        assert event1.max_context_docs == 1

        event50 = QuestionAsked(question="Test?", max_context_docs=50)
        assert event50.max_context_docs == 50

    def test_default_values(self) -> None:
        """Test default values for optional fields."""
        event = QuestionAsked(question="Test?")

        assert event.user_id is None
        assert event.search_mode == "hybrid"
        assert event.max_context_docs == 5
        assert event.model == "llama2"
        assert event.metadata == {}

    def test_event_immutability(self) -> None:
        """Test that event is immutable (frozen)."""
        event = QuestionAsked(question="Test?")

        with pytest.raises(ValidationError):
            event.question = "modified"  # type: ignore


class TestQuestionAnswered:
    """Test QuestionAnswered event validation and serialization."""

    def test_valid_event_creation(self) -> None:
        """Test creating a valid QuestionAnswered event."""
        event = QuestionAnswered(
            question="What is ML?",
            answer_length=500,
            sources_count=3,
            search_mode="hybrid",
            model="llama2",
            execution_time_ms=1250.5,
            user_id="user123",
        )

        assert event.question == "What is ML?"
        assert event.answer_length == 500
        assert event.sources_count == 3
        assert event.execution_time_ms == 1250.5
        assert event.user_id == "user123"

    def test_event_with_cache_metadata(self) -> None:
        """Test event with cache metadata."""
        metadata = {"cache_hit": True, "tokens_generated": 150}
        event = QuestionAnswered(
            question="Test?",
            answer_length=100,
            sources_count=1,
            execution_time_ms=100.0,
            metadata=metadata,
        )

        assert event.metadata["cache_hit"] is True
        assert event.metadata["tokens_generated"] == 150

    def test_answer_length_validation(self) -> None:
        """Test that negative answer_length is rejected."""
        with pytest.raises(ValidationError):
            QuestionAnswered(
                question="Test?",
                answer_length=-1,
                sources_count=0,
                execution_time_ms=100.0,
            )

    def test_sources_count_validation(self) -> None:
        """Test that negative sources_count is rejected."""
        with pytest.raises(ValidationError):
            QuestionAnswered(
                question="Test?",
                answer_length=100,
                sources_count=-1,
                execution_time_ms=100.0,
            )

    def test_execution_time_validation(self) -> None:
        """Test that negative execution_time_ms is rejected."""
        with pytest.raises(ValidationError):
            QuestionAnswered(
                question="Test?",
                answer_length=100,
                sources_count=1,
                execution_time_ms=-1.0,
            )

    def test_default_values(self) -> None:
        """Test default values for optional fields."""
        event = QuestionAnswered(
            question="Test?",
            answer_length=100,
            sources_count=1,
            execution_time_ms=100.0,
        )

        assert event.user_id is None
        assert event.search_mode == "hybrid"
        assert event.model == "llama2"


class TestQuestionAnsweringFailed:
    """Test QuestionAnsweringFailed event validation and serialization."""

    def test_valid_event_creation(self) -> None:
        """Test creating a valid QuestionAnsweringFailed event."""
        event = QuestionAnsweringFailed(
            question="Test?",
            error="Ollama connection timeout",
            error_type="connection",
            search_mode="hybrid",
            model="llama2",
            user_id="user123",
        )

        assert event.question == "Test?"
        assert event.error == "Ollama connection timeout"
        assert event.error_type == "connection"
        assert event.user_id == "user123"

    def test_error_validation_empty(self) -> None:
        """Test that empty error is rejected."""
        with pytest.raises(ValidationError) as exc_info:
            QuestionAnsweringFailed(
                question="Test?",
                error="",
            )

        errors = exc_info.value.errors()
        # Pydantic first checks min_length before validator runs
        assert any("error" in str(error.get("loc", [])) for error in errors)

    def test_error_trimming(self) -> None:
        """Test that error whitespace is trimmed."""
        event = QuestionAnsweringFailed(
            question="Test?",
            error="  Error message  ",
        )

        assert event.error == "Error message"

    def test_error_type_validation(self) -> None:
        """Test that invalid error_type is rejected."""
        with pytest.raises(ValidationError):
            QuestionAnsweringFailed(
                question="Test?",
                error="Error",
                error_type="invalid_type",
            )

    def test_all_error_types_valid(self) -> None:
        """Test that all valid error types are accepted."""
        for error_type in ["search", "generation", "connection", "timeout", "other"]:
            event = QuestionAnsweringFailed(
                question="Test?",
                error="Error",
                error_type=error_type,
            )
            assert event.error_type == error_type

    def test_default_values(self) -> None:
        """Test default values for optional fields."""
        event = QuestionAnsweringFailed(
            question="Test?",
            error="Error",
        )

        assert event.error_type == "other"
        assert event.search_mode == "hybrid"
        assert event.model == "llama2"
        assert event.user_id is None


class TestLLMCacheInvalidated:
    """Test LLMCacheInvalidated event validation and serialization."""

    def test_valid_event_creation(self) -> None:
        """Test creating a valid LLMCacheInvalidated event."""
        event = LLMCacheInvalidated(
            reason="Document corpus updated",
            cache_size_before=150,
        )

        assert event.reason == "Document corpus updated"
        assert event.cache_size_before == 150
        assert isinstance(event.timestamp, datetime)

    def test_event_with_metadata(self) -> None:
        """Test event with metadata."""
        metadata = {"documents_added": 10, "documents_removed": 2}
        event = LLMCacheInvalidated(
            reason="Corpus changed",
            cache_size_before=100,
            metadata=metadata,
        )

        assert event.metadata == metadata

    def test_cache_size_validation(self) -> None:
        """Test that negative cache_size_before is rejected."""
        with pytest.raises(ValidationError):
            LLMCacheInvalidated(
                reason="Test",
                cache_size_before=-1,
            )

    def test_default_values(self) -> None:
        """Test default values."""
        event = LLMCacheInvalidated()

        assert event.reason == "manual"
        assert event.cache_size_before == 0
        assert event.metadata == {}

    def test_event_immutability(self) -> None:
        """Test that event is immutable (frozen)."""
        event = LLMCacheInvalidated()

        with pytest.raises(ValidationError):
            event.reason = "modified"  # type: ignore


class TestLLMHealthCheckPerformed:
    """Test LLMHealthCheckPerformed event validation and serialization."""

    def test_valid_event_creation(self) -> None:
        """Test creating a valid LLMHealthCheckPerformed event."""
        event = LLMHealthCheckPerformed(
            is_healthy=True,
            response_time_ms=45.2,
        )

        assert event.is_healthy is True
        assert event.response_time_ms == 45.2
        assert isinstance(event.timestamp, datetime)

    def test_unhealthy_check(self) -> None:
        """Test creating an unhealthy check event."""
        event = LLMHealthCheckPerformed(
            is_healthy=False,
            response_time_ms=5000.0,
        )

        assert event.is_healthy is False

    def test_response_time_validation(self) -> None:
        """Test that negative response_time_ms is rejected."""
        with pytest.raises(ValidationError):
            LLMHealthCheckPerformed(
                is_healthy=True,
                response_time_ms=-1.0,
            )

    def test_event_with_metadata(self) -> None:
        """Test event with metadata."""
        metadata = {"ollama_version": "0.1.17", "models_available": 3}
        event = LLMHealthCheckPerformed(
            is_healthy=True,
            response_time_ms=50.0,
            metadata=metadata,
        )

        assert event.metadata == metadata

    def test_event_immutability(self) -> None:
        """Test that event is immutable (frozen)."""
        event = LLMHealthCheckPerformed(is_healthy=True, response_time_ms=50.0)

        with pytest.raises(ValidationError):
            event.is_healthy = False  # type: ignore


class TestLLMModelsListed:
    """Test LLMModelsListed event validation and serialization."""

    def test_valid_event_creation(self) -> None:
        """Test creating a valid LLMModelsListed event."""
        models = ["llama2", "mistral", "codellama"]
        event = LLMModelsListed(
            models_count=3,
            models=models,
        )

        assert event.models_count == 3
        assert event.models == models
        assert isinstance(event.timestamp, datetime)

    def test_models_count_validation_mismatch(self) -> None:
        """Test that models_count must match models list length."""
        with pytest.raises(ValidationError) as exc_info:
            LLMModelsListed(
                models_count=5,
                models=["llama2", "mistral"],  # Only 2 models
            )

        errors = exc_info.value.errors()
        assert any("models_count" in str(error) for error in errors)

    def test_empty_models_list(self) -> None:
        """Test event with empty models list."""
        event = LLMModelsListed(
            models_count=0,
            models=[],
        )

        assert event.models_count == 0
        assert event.models == []

    def test_models_count_validation_negative(self) -> None:
        """Test that negative models_count is rejected."""
        with pytest.raises(ValidationError):
            LLMModelsListed(
                models_count=-1,
                models=[],
            )

    def test_event_with_metadata(self) -> None:
        """Test event with metadata."""
        metadata = {"ollama_version": "0.1.17", "total_size_gb": 12.5}
        event = LLMModelsListed(
            models_count=2,
            models=["llama2", "mistral"],
            metadata=metadata,
        )

        assert event.metadata == metadata

    def test_default_values(self) -> None:
        """Test default values."""
        event = LLMModelsListed(models_count=0)

        assert event.models == []
        assert event.metadata == {}

    def test_event_immutability(self) -> None:
        """Test that event is immutable (frozen)."""
        event = LLMModelsListed(models_count=1, models=["llama2"])

        with pytest.raises(ValidationError):
            event.models_count = 2  # type: ignore
