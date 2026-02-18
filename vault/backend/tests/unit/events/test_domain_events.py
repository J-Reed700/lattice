"""Unit tests for domain events."""

from __future__ import annotations

import pytest

from src.events.domain import (
    DocumentDeletedEvent,
    DocumentIndexedEvent,
    DocumentIndexingFailedEvent,
)


class TestDocumentIndexedEvent:
    """Test suite for DocumentIndexedEvent."""

    @pytest.mark.unit()
    def test_create_event(self) -> None:
        """Test creating a DocumentIndexedEvent."""
        event = DocumentIndexedEvent(
            document_id=123,
            file_path="/path/to/doc.pdf",
            chunk_count=10,
            embedding_model="all-MiniLM-L6-v2",
            metadata={"user_id": "user-123"},
        )

        assert event.event_type == "document.indexed"
        assert event.data["document_id"] == 123
        assert event.data["file_path"] == "/path/to/doc.pdf"
        assert event.data["chunk_count"] == 10
        assert event.data["embedding_model"] == "all-MiniLM-L6-v2"
        assert event.metadata["user_id"] == "user-123"

    @pytest.mark.unit()
    def test_event_has_id_and_timestamp(self) -> None:
        """Test that event has auto-generated ID and timestamp."""
        event = DocumentIndexedEvent(
            document_id=123,
            file_path="/path/to/doc.pdf",
            chunk_count=10,
            embedding_model="all-MiniLM-L6-v2",
        )

        assert event.event_id is not None
        assert event.timestamp is not None

    @pytest.mark.unit()
    def test_event_immutability(self) -> None:
        """Test that event is immutable."""
        event = DocumentIndexedEvent(
            document_id=123,
            file_path="/path/to/doc.pdf",
            chunk_count=10,
            embedding_model="all-MiniLM-L6-v2",
        )

        with pytest.raises(
            (AttributeError, ValueError), match="(immutable|frozen|cannot assign)"
        ):
            event.data = {"new": "data"}  # type: ignore[misc]

    @pytest.mark.unit()
    def test_metadata_defaults_to_empty_dict(self) -> None:
        """Test that metadata defaults to empty dict if not provided."""
        event = DocumentIndexedEvent(
            document_id=123,
            file_path="/path/to/doc.pdf",
            chunk_count=10,
            embedding_model="all-MiniLM-L6-v2",
        )

        assert event.metadata == {}


class TestDocumentIndexingFailedEvent:
    """Test suite for DocumentIndexingFailedEvent."""

    @pytest.mark.unit()
    def test_create_event(self) -> None:
        """Test creating a DocumentIndexingFailedEvent."""
        event = DocumentIndexingFailedEvent(
            file_path="/path/to/doc.pdf",
            error_type="ExtractionError",
            error_message="Failed to extract text from PDF",
            metadata={"user_id": "user-123"},
        )

        assert event.event_type == "document.indexing_failed"
        assert event.data["file_path"] == "/path/to/doc.pdf"
        assert event.data["error_type"] == "ExtractionError"
        assert event.data["error_message"] == "Failed to extract text from PDF"
        assert event.metadata["user_id"] == "user-123"

    @pytest.mark.unit()
    def test_event_structure(self) -> None:
        """Test event has correct structure."""
        event = DocumentIndexingFailedEvent(
            file_path="/path/to/doc.pdf",
            error_type="ValidationError",
            error_message="Document too large",
        )

        assert hasattr(event, "event_id")
        assert hasattr(event, "event_type")
        assert hasattr(event, "timestamp")
        assert hasattr(event, "data")
        assert hasattr(event, "metadata")


class TestDocumentDeletedEvent:
    """Test suite for DocumentDeletedEvent."""

    @pytest.mark.unit()
    def test_create_event(self) -> None:
        """Test creating a DocumentDeletedEvent."""
        event = DocumentDeletedEvent(
            document_id=123,
            file_path="/path/to/doc.pdf",
            chunk_count=10,
            metadata={"user_id": "user-123", "reason": "manual_deletion"},
        )

        assert event.event_type == "document.deleted"
        assert event.data["document_id"] == 123
        assert event.data["file_path"] == "/path/to/doc.pdf"
        assert event.data["chunk_count"] == 10
        assert event.metadata["user_id"] == "user-123"
        assert event.metadata["reason"] == "manual_deletion"

    @pytest.mark.unit()
    def test_event_json_serializable(self) -> None:
        """Test that event can be serialized to JSON."""
        event = DocumentDeletedEvent(
            document_id=123,
            file_path="/path/to/doc.pdf",
            chunk_count=10,
        )

        json_data = event.model_dump_json()
        assert json_data is not None
        assert isinstance(json_data, str)
