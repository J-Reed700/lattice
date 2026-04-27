"""Unit tests for UnifiedSearchService."""

from __future__ import annotations

from datetime import UTC, datetime
from unittest.mock import AsyncMock, MagicMock, patch

import pytest
from sqlalchemy.ext.asyncio import AsyncSession

from src.api.schemas.search import SearchFilters, SearchRequest
from src.models.sync import Document
from src.services.search.unified_search import UnifiedSearchService


@pytest.fixture()
def mock_session() -> AsyncMock:
    """Mock database session."""
    session = AsyncMock(spec=AsyncSession)
    return session


@pytest.fixture()
def mock_bm25_engine() -> MagicMock:
    """Mock BM25 search engine."""
    engine = MagicMock()
    engine.search = AsyncMock()
    return engine


@pytest.fixture()
def mock_hybrid_engine() -> MagicMock:
    """Mock hybrid search engine."""
    engine = MagicMock()
    engine.search = AsyncMock()
    return engine


@pytest.fixture()
def unified_service(
    mock_session: AsyncMock,
    mock_bm25_engine: MagicMock,
    mock_hybrid_engine: MagicMock,
) -> UnifiedSearchService:
    """Create UnifiedSearchService with mocked dependencies."""
    return UnifiedSearchService(
        session=mock_session,
        bm25_engine=mock_bm25_engine,
        hybrid_engine=mock_hybrid_engine,
    )


@pytest.fixture()
def sample_document() -> Document:
    """Create a sample document for testing."""
    now = datetime.now(UTC)
    return Document(
        id=1,
        user_id=1,
        device_id=1,
        path="/test/document.txt",
        title="Test Document",
        content="This is test content",
        content_hash="abc123",
        created_at=now,
        modified_at=now,
        version=1,
    )


class TestUnifiedSearchService:
    """Test suite for UnifiedSearchService."""

    @pytest.mark.unit()
    @pytest.mark.asyncio()
    async def test_search_bm25_mode(
        self,
        unified_service: UnifiedSearchService,
        mock_bm25_engine: MagicMock,
        sample_document: Document,
    ):
        """Test BM25 search mode."""
        # Arrange
        request = SearchRequest(query="test query", mode="bm25", limit=10, offset=0)

        mock_bm25_engine.search.return_value = [
            {
                "document_id": 1,
                "file_path": "/test/document.txt",
                "filename": "document.txt",
                "bm25_score": 0.85,
            }
        ]

        # Mock document fetch
        mock_result = AsyncMock()
        mock_result.scalar_one_or_none.return_value = sample_document
        unified_service.session.execute = AsyncMock(return_value=mock_result)

        # Act
        result = await unified_service.search(request)

        # Assert
        assert len(result.results) == 1
        assert result.total == 1
        assert result.results[0].score == 0.85
        assert result.results[0].file_path == "/test/document.txt"
        mock_bm25_engine.search.assert_called_once()

    @pytest.mark.unit()
    @pytest.mark.asyncio()
    async def test_search_hybrid_mode(
        self,
        unified_service: UnifiedSearchService,
        mock_hybrid_engine: MagicMock,
        sample_document: Document,
    ):
        """Test hybrid search mode."""
        # Arrange
        request = SearchRequest(
            query="test query",
            mode="hybrid",
            limit=10,
            offset=0,
            semantic_weight=0.7,
            keyword_weight=0.3,
        )

        mock_hybrid_engine.search.return_value = [
            {
                "file_id": 1,
                "file_path": "/test/document.txt",
                "filename": "document.txt",
                "score": 0.92,
            }
        ]

        # Mock document fetch
        mock_result = AsyncMock()
        mock_result.scalar_one_or_none.return_value = sample_document
        unified_service.session.execute = AsyncMock(return_value=mock_result)

        # Mock settings
        with patch("src.services.search.unified_search.get_settings") as mock_settings:
            mock_settings.return_value.hybrid_search_enabled = True
            mock_settings.return_value.hybrid_default_strategy = "rrf"

            # Act
            result = await unified_service.search(request)

            # Assert
            assert len(result.results) == 1
            assert result.total == 1
            assert result.results[0].score == 0.92
            assert result.fusion_strategy == "rrf"
            mock_hybrid_engine.search.assert_called_once()

    @pytest.mark.unit()
    @pytest.mark.asyncio()
    async def test_search_vector_mode(self, unified_service: UnifiedSearchService):
        """Test vector search mode."""
        # Arrange
        request = SearchRequest(query="test query", mode="vector", limit=10, offset=0)

        # Mock SearchService.search
        with patch.object(
            unified_service.search_service, "search", new_callable=AsyncMock
        ) as mock_search:
            mock_search.return_value = []

            # Act
            result = await unified_service.search(request)

            # Assert
            assert result.results == []
            mock_search.assert_called_once()

    @pytest.mark.unit()
    @pytest.mark.asyncio()
    async def test_search_with_pagination(
        self,
        unified_service: UnifiedSearchService,
        mock_bm25_engine: MagicMock,
        sample_document: Document,
    ):
        """Test search with pagination."""
        # Arrange
        request = SearchRequest(query="test query", mode="bm25", limit=5, offset=5)

        # Return 15 results
        mock_bm25_engine.search.return_value = [
            {
                "document_id": i,
                "file_path": f"/test/doc{i}.txt",
                "filename": f"doc{i}.txt",
                "bm25_score": 0.8,
            }
            for i in range(15)
        ]

        # Mock document fetch
        mock_result = AsyncMock()
        mock_result.scalar_one_or_none.return_value = sample_document
        unified_service.session.execute = AsyncMock(return_value=mock_result)

        # Act
        result = await unified_service.search(request)

        # Assert
        assert result.total == 15
        assert len(result.results) == 10  # Paginated results from offset 5

    @pytest.mark.unit()
    @pytest.mark.asyncio()
    async def test_search_records_history(
        self, unified_service: UnifiedSearchService, mock_bm25_engine: MagicMock
    ):
        """Test that search records history."""
        # Arrange
        request = SearchRequest(query="test query", mode="bm25", limit=10, offset=0)

        mock_bm25_engine.search.return_value = []

        # Mock history service
        with patch.object(
            unified_service.history_service,
            "record_search",
            new_callable=AsyncMock,
        ) as mock_record:
            # Act
            await unified_service.search(request)

            # Assert
            mock_record.assert_called_once()
            call_args = mock_record.call_args[1]
            assert call_args["query"] == "test query"
            assert call_args["search_type"] == "bm25"

    @pytest.mark.unit()
    @pytest.mark.asyncio()
    async def test_search_with_filters(
        self,
        unified_service: UnifiedSearchService,
        mock_bm25_engine: MagicMock,
    ):
        """Test search with filters."""
        # Arrange
        filters = SearchFilters(mime_types=["application/pdf"], extensions=["pdf"])
        request = SearchRequest(
            query="test query", mode="bm25", limit=10, offset=0, filters=filters
        )

        mock_bm25_engine.search.return_value = []

        # Act
        result = await unified_service.search(request)

        # Assert
        assert result.results == []
        mock_bm25_engine.search.assert_called_once()

    @pytest.mark.unit()
    @pytest.mark.asyncio()
    async def test_map_bm25_results(
        self, unified_service: UnifiedSearchService, sample_document: Document
    ):
        """Test mapping BM25 results to SearchResultItem."""
        # Arrange
        raw_results = [
            {
                "document_id": 1,
                "file_path": "/test/document.txt",
                "filename": "document.txt",
                "bm25_score": 0.85,
            }
        ]

        # Mock document fetch
        mock_result = AsyncMock()
        mock_result.scalar_one_or_none.return_value = sample_document
        unified_service.session.execute = AsyncMock(return_value=mock_result)

        # Act
        results = await unified_service._map_bm25_results(raw_results)

        # Assert
        assert len(results) == 1
        assert results[0].score == 0.85
        assert results[0].file_path == "/test/document.txt"

    @pytest.mark.unit()
    @pytest.mark.asyncio()
    async def test_map_hybrid_results(
        self, unified_service: UnifiedSearchService, sample_document: Document
    ):
        """Test mapping hybrid results to SearchResultItem."""
        # Arrange
        raw_results = [
            {
                "file_id": 1,
                "file_path": "/test/document.txt",
                "filename": "document.txt",
                "score": 0.92,
            }
        ]

        # Mock document fetch
        mock_result = AsyncMock()
        mock_result.scalar_one_or_none.return_value = sample_document
        unified_service.session.execute = AsyncMock(return_value=mock_result)

        # Act
        results = await unified_service._map_hybrid_results(raw_results)

        # Assert
        assert len(results) == 1
        assert results[0].score == 0.92
        assert results[0].file_path == "/test/document.txt"

    @pytest.mark.unit()
    @pytest.mark.asyncio()
    async def test_get_document_by_id(
        self, unified_service: UnifiedSearchService, sample_document: Document
    ):
        """Test fetching document by ID."""
        # Arrange
        mock_result = AsyncMock()
        mock_result.scalar_one_or_none.return_value = sample_document
        unified_service.session.execute = AsyncMock(return_value=mock_result)

        # Act
        doc = await unified_service._get_document_by_id(1)

        # Assert
        assert doc is not None
        assert doc.id == 1
        assert doc.path == "/test/document.txt"

    @pytest.mark.unit()
    @pytest.mark.asyncio()
    async def test_create_search_result_item(
        self, unified_service: UnifiedSearchService, sample_document: Document
    ):
        """Test creating SearchResultItem from Document."""
        # Act
        result_item = unified_service._create_search_result_item(
            doc=sample_document, score=0.9, snippet="Test snippet"
        )

        # Assert
        assert result_item.id == 1
        assert result_item.file_path == "/test/document.txt"
        assert result_item.filename == "Test Document"
        assert result_item.score == 0.9
        assert result_item.snippet == "Test snippet"

    @pytest.mark.unit()
    @pytest.mark.asyncio()
    async def test_search_handles_error(
        self, unified_service: UnifiedSearchService, mock_bm25_engine: MagicMock
    ):
        """Test that search handles errors properly."""
        # Arrange
        request = SearchRequest(query="test query", mode="bm25", limit=10, offset=0)

        mock_bm25_engine.search.side_effect = Exception("Search engine error")

        # Act & Assert
        with pytest.raises(Exception) as exc_info:
            await unified_service.search(request)

        assert "Search engine error" in str(exc_info.value)
