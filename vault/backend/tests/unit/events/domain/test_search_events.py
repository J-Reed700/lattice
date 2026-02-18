"""Unit tests for search-related domain events."""

from datetime import datetime

import pytest
from pydantic import ValidationError

from src.events.domain.search_events import SearchQueryRecorded, SearchQueryRecordingFailed


class TestSearchQueryRecorded:
    """Test SearchQueryRecorded event validation and serialization."""

    def test_valid_event_creation(self) -> None:
        """Test creating a valid SearchQueryRecorded event."""
        event = SearchQueryRecorded(
            query="machine learning",
            search_type="hybrid",
            result_count=15,
            execution_time_ms=125.5,
            user_id="user123",
        )

        assert event.query == "machine learning"
        assert event.search_type == "hybrid"
        assert event.result_count == 15
        assert event.execution_time_ms == 125.5
        assert event.user_id == "user123"
        assert isinstance(event.timestamp, datetime)
        assert event.metadata == {}

    def test_event_with_metadata(self) -> None:
        """Test event with additional metadata."""
        metadata = {"filters": {"tags": ["python"]}, "reranked": True}
        event = SearchQueryRecorded(
            query="python tutorial",
            search_type="semantic",
            result_count=10,
            execution_time_ms=50.0,
            metadata=metadata,
        )

        assert event.metadata == metadata

    def test_query_validation_empty(self) -> None:
        """Test that empty query is rejected."""
        with pytest.raises(ValidationError) as exc_info:
            SearchQueryRecorded(
                query="",
                search_type="hybrid",
                result_count=0,
                execution_time_ms=10.0,
            )

        errors = exc_info.value.errors()
        # Pydantic first checks min_length before validator runs
        assert any("query" in str(error.get("loc", [])) for error in errors)

    def test_query_validation_whitespace(self) -> None:
        """Test that whitespace-only query is rejected."""
        with pytest.raises(ValidationError) as exc_info:
            SearchQueryRecorded(
                query="   ",
                search_type="hybrid",
                result_count=0,
                execution_time_ms=10.0,
            )

        errors = exc_info.value.errors()
        assert any("Query must not be empty" in str(error) for error in errors)

    def test_query_validation_too_long(self) -> None:
        """Test that query exceeding max length is rejected."""
        long_query = "a" * 501
        with pytest.raises(ValidationError) as exc_info:
            SearchQueryRecorded(
                query=long_query,
                search_type="hybrid",
                result_count=0,
                execution_time_ms=10.0,
            )

        errors = exc_info.value.errors()
        assert any("max_length" in str(error) for error in errors)

    def test_search_type_validation(self) -> None:
        """Test that invalid search_type is rejected."""
        with pytest.raises(ValidationError) as exc_info:
            SearchQueryRecorded(
                query="test",
                search_type="invalid_type",
                result_count=0,
                execution_time_ms=10.0,
            )

        errors = exc_info.value.errors()
        assert any("search_type" in str(error) for error in errors)

    def test_result_count_validation_negative(self) -> None:
        """Test that negative result_count is rejected."""
        with pytest.raises(ValidationError) as exc_info:
            SearchQueryRecorded(
                query="test",
                search_type="hybrid",
                result_count=-1,
                execution_time_ms=10.0,
            )

        errors = exc_info.value.errors()
        assert any("result_count" in str(error) for error in errors)

    def test_execution_time_validation_negative(self) -> None:
        """Test that negative execution_time_ms is rejected."""
        with pytest.raises(ValidationError) as exc_info:
            SearchQueryRecorded(
                query="test",
                search_type="hybrid",
                result_count=0,
                execution_time_ms=-1.0,
            )

        errors = exc_info.value.errors()
        assert any("execution_time_ms" in str(error) for error in errors)

    def test_event_immutability(self) -> None:
        """Test that event is immutable (frozen)."""
        event = SearchQueryRecorded(
            query="test",
            search_type="hybrid",
            result_count=0,
            execution_time_ms=10.0,
        )

        with pytest.raises(ValidationError):
            event.query = "modified"  # type: ignore

    def test_query_trimming(self) -> None:
        """Test that query whitespace is trimmed."""
        event = SearchQueryRecorded(
            query="  test query  ",
            search_type="hybrid",
            result_count=0,
            execution_time_ms=10.0,
        )

        assert event.query == "test query"

    def test_default_user_id(self) -> None:
        """Test that user_id defaults to 'default'."""
        event = SearchQueryRecorded(
            query="test",
            search_type="hybrid",
            result_count=0,
            execution_time_ms=10.0,
        )

        assert event.user_id == "default"

    def test_all_search_types_valid(self) -> None:
        """Test that all valid search types are accepted."""
        for search_type in ["semantic", "keyword", "hybrid"]:
            event = SearchQueryRecorded(
                query="test",
                search_type=search_type,
                result_count=0,
                execution_time_ms=10.0,
            )
            assert event.search_type == search_type

    def test_json_serialization(self) -> None:
        """Test that event can be serialized to JSON."""
        event = SearchQueryRecorded(
            query="test",
            search_type="hybrid",
            result_count=5,
            execution_time_ms=50.0,
            user_id="user123",
        )

        json_str = event.model_dump_json()
        assert "test" in json_str
        assert "hybrid" in json_str
        assert "user123" in json_str


class TestSearchQueryRecordingFailed:
    """Test SearchQueryRecordingFailed event validation and serialization."""

    def test_valid_event_creation(self) -> None:
        """Test creating a valid SearchQueryRecordingFailed event."""
        event = SearchQueryRecordingFailed(
            query="test query",
            search_type="hybrid",
            result_count=10,
            execution_time_ms=50.0,
            user_id="user123",
            error="Database connection timeout",
        )

        assert event.query == "test query"
        assert event.search_type == "hybrid"
        assert event.result_count == 10
        assert event.execution_time_ms == 50.0
        assert event.user_id == "user123"
        assert event.error == "Database connection timeout"
        assert isinstance(event.timestamp, datetime)

    def test_event_with_metadata(self) -> None:
        """Test event with error metadata."""
        metadata = {"retry_count": 3, "last_error": "Connection refused"}
        event = SearchQueryRecordingFailed(
            query="test",
            search_type="hybrid",
            result_count=0,
            execution_time_ms=10.0,
            error="Failed after retries",
            metadata=metadata,
        )

        assert event.metadata == metadata

    def test_query_validation_empty(self) -> None:
        """Test that empty query is rejected."""
        with pytest.raises(ValidationError) as exc_info:
            SearchQueryRecordingFailed(
                query="",
                search_type="hybrid",
                result_count=0,
                execution_time_ms=10.0,
                error="Test error",
            )

        errors = exc_info.value.errors()
        # Pydantic first checks min_length before validator runs
        assert any("query" in str(error.get("loc", [])) for error in errors)

    def test_error_validation_empty(self) -> None:
        """Test that empty error is rejected."""
        with pytest.raises(ValidationError) as exc_info:
            SearchQueryRecordingFailed(
                query="test",
                search_type="hybrid",
                result_count=0,
                execution_time_ms=10.0,
                error="",
            )

        errors = exc_info.value.errors()
        # Pydantic first checks min_length before validator runs
        assert any("error" in str(error.get("loc", [])) for error in errors)

    def test_error_validation_whitespace(self) -> None:
        """Test that whitespace-only error is rejected."""
        with pytest.raises(ValidationError) as exc_info:
            SearchQueryRecordingFailed(
                query="test",
                search_type="hybrid",
                result_count=0,
                execution_time_ms=10.0,
                error="   ",
            )

        errors = exc_info.value.errors()
        assert any("Error must not be empty" in str(error) for error in errors)

    def test_error_trimming(self) -> None:
        """Test that error whitespace is trimmed."""
        event = SearchQueryRecordingFailed(
            query="test",
            search_type="hybrid",
            result_count=0,
            execution_time_ms=10.0,
            error="  Error message  ",
        )

        assert event.error == "Error message"

    def test_event_immutability(self) -> None:
        """Test that event is immutable (frozen)."""
        event = SearchQueryRecordingFailed(
            query="test",
            search_type="hybrid",
            result_count=0,
            execution_time_ms=10.0,
            error="Test error",
        )

        with pytest.raises(ValidationError):
            event.error = "modified"  # type: ignore

    def test_default_user_id(self) -> None:
        """Test that user_id defaults to 'default'."""
        event = SearchQueryRecordingFailed(
            query="test",
            search_type="hybrid",
            result_count=0,
            execution_time_ms=10.0,
            error="Test error",
        )

        assert event.user_id == "default"

    def test_json_serialization(self) -> None:
        """Test that event can be serialized to JSON."""
        event = SearchQueryRecordingFailed(
            query="test",
            search_type="hybrid",
            result_count=0,
            execution_time_ms=10.0,
            error="Database error",
        )

        json_str = event.model_dump_json()
        assert "test" in json_str
        assert "Database error" in json_str
