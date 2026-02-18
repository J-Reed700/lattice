"""Unit tests for web service.

Tests:
- SSRF prevention (URL validation)
- Caching (hit/miss, TTL, LRU eviction)
- Web search (DuckDuckGo)
- URL content fetching
- Content extraction (article, raw_text, markdown)
- Timeout handling
- Error handling
"""

from __future__ import annotations

from unittest.mock import AsyncMock, MagicMock, patch

import httpx
import pytest

from src.services.llm.function_calling.web_service import WebCache, WebService
from tests.fixtures.function_calling_fixtures import (
    VALID_URLS,
    create_fetch_url_content_input,
    create_test_html,
    create_web_search_input,
)


@pytest.mark.unit()
class TestWebCache:
    """Test WebCache class."""

    @pytest.mark.asyncio()
    async def test_cache_miss(self) -> None:
        """Test cache returns None on miss."""
        cache = WebCache()

        result = await cache.get("search", query="test")

        assert result is None

    @pytest.mark.asyncio()
    async def test_cache_hit(self) -> None:
        """Test cache returns stored result on hit."""
        cache = WebCache()
        data = {"results": [1, 2, 3]}

        await cache.put("search", data, query="test")
        result = await cache.get("search", query="test")

        assert result == data

    @pytest.mark.asyncio()
    async def test_cache_key_uniqueness(self) -> None:
        """Test different parameters create different cache keys."""
        cache = WebCache()

        await cache.put("search", {"data": "A"}, query="test1")
        await cache.put("search", {"data": "B"}, query="test2")

        result1 = await cache.get("search", query="test1")
        result2 = await cache.get("search", query="test2")

        assert result1 == {"data": "A"}
        assert result2 == {"data": "B"}

    @pytest.mark.asyncio()
    async def test_cache_ttl_expiration(self) -> None:
        """Test cache entries expire after TTL."""
        cache = WebCache(ttl_seconds=0)  # Immediate expiration

        await cache.put("search", {"data": "test"}, query="test")

        # Entry should be expired
        result = await cache.get("search", query="test")
        assert result is None

    @pytest.mark.asyncio()
    async def test_cache_lru_eviction(self) -> None:
        """Test cache evicts oldest entries when full."""
        cache = WebCache(max_size=2)

        # Fill cache
        await cache.put("search", {"data": "1"}, query="q1")
        await cache.put("search", {"data": "2"}, query="q2")

        # Add third entry (should evict first)
        await cache.put("search", {"data": "3"}, query="q3")

        # First should be evicted
        result1 = await cache.get("search", query="q1")
        assert result1 is None

        # Second and third should exist
        result2 = await cache.get("search", query="q2")
        result3 = await cache.get("search", query="q3")
        assert result2 == {"data": "2"}
        assert result3 == {"data": "3"}

    @pytest.mark.asyncio()
    async def test_cache_lru_access_updates_order(self) -> None:
        """Test accessing cache entry moves it to end (LRU)."""
        cache = WebCache(max_size=2)

        await cache.put("search", {"data": "1"}, query="q1")
        await cache.put("search", {"data": "2"}, query="q2")

        # Access first entry (moves to end)
        await cache.get("search", query="q1")

        # Add third entry (should evict second, not first)
        await cache.put("search", {"data": "3"}, query="q3")

        # Second should be evicted
        result2 = await cache.get("search", query="q2")
        assert result2 is None

        # First and third should exist
        result1 = await cache.get("search", query="q1")
        result3 = await cache.get("search", query="q3")
        assert result1 == {"data": "1"}
        assert result3 == {"data": "3"}


@pytest.mark.unit()
class TestWebServiceValidation:
    """Test URL validation and SSRF prevention."""

    @pytest.mark.asyncio()
    async def test_validate_url_valid_urls(self) -> None:
        """Test validation passes for valid URLs."""
        service = WebService(cache_enabled=False)

        for url in VALID_URLS:
            # Should not raise
            service._validate_url(url)

    @pytest.mark.asyncio()
    async def test_validate_url_blocks_localhost(self) -> None:
        """Test validation blocks localhost."""
        service = WebService(cache_enabled=False)

        with pytest.raises(ValueError, match="blocked"):
            service._validate_url("http://localhost:8080")

    @pytest.mark.asyncio()
    async def test_validate_url_blocks_127_0_0_1(self) -> None:
        """Test validation blocks loopback IP."""
        service = WebService(cache_enabled=False)

        with pytest.raises(ValueError, match="blocked"):
            service._validate_url("http://127.0.0.1")

    @pytest.mark.asyncio()
    async def test_validate_url_blocks_private_ips(self) -> None:
        """Test validation blocks private IP ranges."""
        service = WebService(cache_enabled=False)

        private_urls = [
            "http://192.168.1.1",
            "http://10.0.0.1",
            "http://172.16.0.1",
        ]

        for url in private_urls:
            with pytest.raises(ValueError, match="blocked"):
                service._validate_url(url)

    @pytest.mark.asyncio()
    async def test_validate_url_blocks_link_local(self) -> None:
        """Test validation blocks link-local addresses (AWS metadata)."""
        service = WebService(cache_enabled=False)

        with pytest.raises(ValueError, match="blocked"):
            service._validate_url("http://169.254.169.254/latest/meta-data")

    @pytest.mark.asyncio()
    async def test_validate_url_blocks_invalid_scheme(self) -> None:
        """Test validation blocks non-HTTP(S) schemes."""
        service = WebService(cache_enabled=False)

        invalid_schemes = [
            "ftp://example.com",
            "file:///etc/passwd",
            "gopher://example.com",
        ]

        for url in invalid_schemes:
            with pytest.raises(ValueError, match="Unsupported URL scheme"):
                service._validate_url(url)

    @pytest.mark.asyncio()
    async def test_validate_url_requires_scheme(self) -> None:
        """Test validation requires URL scheme."""
        service = WebService(cache_enabled=False)

        with pytest.raises(ValueError, match="Invalid URL format"):
            service._validate_url("example.com")

    @pytest.mark.asyncio()
    async def test_validate_url_requires_netloc(self) -> None:
        """Test validation requires network location."""
        service = WebService(cache_enabled=False)

        with pytest.raises(ValueError, match="Invalid URL format"):
            service._validate_url("http://")


@pytest.mark.unit()
class TestWebServiceInitialization:
    """Test WebService initialization and lifecycle."""

    @pytest.mark.asyncio()
    async def test_initialize_creates_http_client(self) -> None:
        """Test initialize creates HTTP client."""
        service = WebService()

        assert service._http_client is None

        await service.initialize()

        assert service._http_client is not None
        assert isinstance(service._http_client, httpx.AsyncClient)

        await service.cleanup()

    @pytest.mark.asyncio()
    async def test_cleanup_closes_http_client(self) -> None:
        """Test cleanup closes HTTP client."""
        service = WebService()
        await service.initialize()

        assert service._http_client is not None

        await service.cleanup()

        assert service._http_client is None

    @pytest.mark.asyncio()
    async def test_http_client_property_raises_if_not_initialized(self) -> None:
        """Test accessing http_client before initialization raises error."""
        service = WebService()

        with pytest.raises(RuntimeError, match="not initialized"):
            _ = service.http_client

    @pytest.mark.asyncio()
    async def test_cache_enabled_by_default(self) -> None:
        """Test cache is enabled by default."""
        service = WebService()

        assert service.cache is not None

    @pytest.mark.asyncio()
    async def test_cache_disabled(self) -> None:
        """Test cache can be disabled."""
        service = WebService(cache_enabled=False)

        assert service.cache is None


@pytest.mark.unit()
class TestWebServiceSearch:
    """Test web search functionality."""

    @pytest.mark.asyncio()
    async def test_search_web_success(self) -> None:
        """Test successful web search."""
        service = WebService(cache_enabled=False)

        mock_ddgs = MagicMock()
        mock_ddgs.text.return_value = [
            {"title": "Result 1", "href": "https://example.com/1", "body": "Snippet 1"},
            {"title": "Result 2", "href": "https://example.com/2", "body": "Snippet 2"},
        ]

        with patch("src.services.llm.function_calling.web_service.DDGS", return_value=mock_ddgs):
            input_data = create_web_search_input(query="test query", max_results=2)
            result = await service.search_web(input_data)

        assert result.query == "test query"
        assert result.result_count == 2
        assert len(result.results) == 2
        assert result.results[0].title == "Result 1"
        assert result.results[0].url == "https://example.com/1"

    @pytest.mark.asyncio()
    async def test_search_web_with_cache_miss(self) -> None:
        """Test web search with cache miss."""
        service = WebService(cache_enabled=True)

        mock_ddgs = MagicMock()
        mock_ddgs.text.return_value = [
            {"title": "Result", "href": "https://example.com", "body": "Snippet"},
        ]

        with patch("src.services.llm.function_calling.web_service.DDGS", return_value=mock_ddgs):
            input_data = create_web_search_input(query="test")
            result = await service.search_web(input_data)

        assert result.result_count == 1

    @pytest.mark.asyncio()
    async def test_search_web_with_cache_hit(self) -> None:
        """Test web search returns cached result."""
        service = WebService(cache_enabled=True)

        mock_ddgs = MagicMock()
        mock_ddgs.text.return_value = [
            {"title": "Result", "href": "https://example.com", "body": "Snippet"},
        ]

        input_data = create_web_search_input(query="test")

        with patch("src.services.llm.function_calling.web_service.DDGS", return_value=mock_ddgs):
            # First call (cache miss)
            result1 = await service.search_web(input_data)

            # Second call (cache hit - should not call DDGS)
            result2 = await service.search_web(input_data)

        # DDGS should only be called once
        assert mock_ddgs.text.call_count == 1
        assert result1 == result2

    @pytest.mark.asyncio()
    async def test_search_web_import_error(self) -> None:
        """Test search raises ImportError if duckduckgo-search not installed."""
        service = WebService(cache_enabled=False)

        with patch("src.services.llm.function_calling.web_service.DDGS", side_effect=ImportError):
            input_data = create_web_search_input()

            with pytest.raises(ImportError, match="duckduckgo-search"):
                await service.search_web(input_data)

    @pytest.mark.asyncio()
    async def test_search_web_runtime_error(self) -> None:
        """Test search handles runtime errors."""
        service = WebService(cache_enabled=False)

        mock_ddgs = MagicMock()
        mock_ddgs.text.side_effect = RuntimeError("Search failed")

        with patch("src.services.llm.function_calling.web_service.DDGS", return_value=mock_ddgs):
            input_data = create_web_search_input()

            with pytest.raises(RuntimeError, match="Web search failed"):
                await service.search_web(input_data)


@pytest.mark.unit()
class TestWebServiceFetch:
    """Test URL content fetching."""

    @pytest.mark.asyncio()
    async def test_fetch_url_content_success(self) -> None:
        """Test successful URL content fetching."""
        service = WebService(cache_enabled=False)
        await service.initialize()

        html_content = create_test_html(title="Test Page", content="Test content")

        mock_response = MagicMock()
        mock_response.text = html_content
        mock_response.url = "https://example.com/test"
        mock_response.headers = {"content-type": "text/html"}
        mock_response.raise_for_status = MagicMock()

        with patch.object(service.http_client, "get", new=AsyncMock(return_value=mock_response)):
            input_data = create_fetch_url_content_input(url="https://example.com/test")
            result = await service.fetch_url_content(input_data)

        assert result.url == "https://example.com/test"
        assert result.title == "Test Page"
        assert "Test content" in result.content
        assert result.content_truncated is False
        assert result.word_count > 0

        await service.cleanup()

    @pytest.mark.asyncio()
    async def test_fetch_url_content_validates_url(self) -> None:
        """Test fetch validates URL before fetching."""
        service = WebService(cache_enabled=False)

        input_data = create_fetch_url_content_input(url="http://localhost:8080")

        with pytest.raises(ValueError, match="blocked"):
            await service.fetch_url_content(input_data)

    @pytest.mark.asyncio()
    async def test_fetch_url_content_with_cache(self) -> None:
        """Test fetch uses cache."""
        service = WebService(cache_enabled=True)
        await service.initialize()

        html_content = create_test_html()

        mock_response = MagicMock()
        mock_response.text = html_content
        mock_response.url = "https://example.com"
        mock_response.headers = {"content-type": "text/html"}
        mock_response.raise_for_status = MagicMock()

        with patch.object(service.http_client, "get", new=AsyncMock(return_value=mock_response)):
            input_data = create_fetch_url_content_input(url="https://example.com")

            # First call (cache miss)
            result1 = await service.fetch_url_content(input_data)

            # Second call (cache hit)
            result2 = await service.fetch_url_content(input_data)

        # HTTP client should only be called once
        assert service.http_client.get.call_count == 1  # type: ignore
        assert result1 == result2

        await service.cleanup()

    @pytest.mark.asyncio()
    async def test_fetch_url_content_truncates_large_content(self) -> None:
        """Test fetch truncates content exceeding max length."""
        service = WebService(cache_enabled=False)
        await service.initialize()

        large_content = "word " * 20000  # Very large content
        html_content = create_test_html(content=large_content)

        mock_response = MagicMock()
        mock_response.text = html_content
        mock_response.url = "https://example.com"
        mock_response.headers = {"content-type": "text/html"}
        mock_response.raise_for_status = MagicMock()

        with patch.object(service.http_client, "get", new=AsyncMock(return_value=mock_response)):
            input_data = create_fetch_url_content_input(
                url="https://example.com",
                max_content_length=1000,
            )
            result = await service.fetch_url_content(input_data)

        assert len(result.content) <= 1000
        assert result.content_truncated is True

        await service.cleanup()

    @pytest.mark.asyncio()
    async def test_fetch_url_content_http_error(self) -> None:
        """Test fetch handles HTTP errors."""
        service = WebService(cache_enabled=False)
        await service.initialize()

        with patch.object(
            service.http_client,
            "get",
            new=AsyncMock(side_effect=httpx.HTTPError("Connection failed")),
        ):
            input_data = create_fetch_url_content_input(url="https://example.com")

            with pytest.raises(RuntimeError, match="Failed to fetch URL"):
                await service.fetch_url_content(input_data)

        await service.cleanup()


@pytest.mark.unit()
class TestWebServiceExtraction:
    """Test content extraction modes."""

    def test_extract_article_mode(self) -> None:
        """Test article extraction mode."""
        service = WebService(cache_enabled=False)
        html = create_test_html(title="Article Title", content="Article content here")

        content, title = service._extract_article(html)

        assert title == "Article Title"
        assert "Article content" in content
        # Script and style should be removed
        assert "console.log" not in content
        assert "color: red" not in content

    def test_extract_raw_text_mode(self) -> None:
        """Test raw text extraction mode."""
        service = WebService(cache_enabled=False)
        html = create_test_html(title="Page Title", content="Page content")

        content, title = service._extract_raw_text(html)

        assert title == "Page Title"
        assert "Page content" in content

    def test_extract_markdown_mode(self) -> None:
        """Test markdown extraction mode."""
        service = WebService(cache_enabled=False)
        html = """
        <html>
        <head><title>Test</title></head>
        <body>
            <h1>Main Heading</h1>
            <p>Paragraph text</p>
            <a href="https://example.com">Link</a>
        </body>
        </html>
        """

        content, title = service._extract_markdown(html)

        assert title == "Test"
        assert "# Main Heading" in content
        assert "[Link](https://example.com)" in content

    def test_extract_handles_missing_article_tag(self) -> None:
        """Test extraction handles HTML without article tag."""
        service = WebService(cache_enabled=False)
        html = "<html><body><p>Content without article tag</p></body></html>"

        content, title = service._extract_article(html)

        assert "Content without article tag" in content

    def test_extract_removes_scripts_and_styles(self) -> None:
        """Test extraction removes script and style elements."""
        service = WebService(cache_enabled=False)
        html = """
        <html>
        <body>
            <script>alert('test');</script>
            <style>.test { color: red; }</style>
            <p>Visible content</p>
        </body>
        </html>
        """

        content, _ = service._extract_article(html)

        assert "alert" not in content
        assert "color: red" not in content
        assert "Visible content" in content


@pytest.mark.unit()
class TestWebServiceEdgeCases:
    """Test edge cases and corner cases."""

    @pytest.mark.asyncio()
    async def test_search_with_empty_results(self) -> None:
        """Test search with no results."""
        service = WebService(cache_enabled=False)

        mock_ddgs = MagicMock()
        mock_ddgs.text.return_value = []

        with patch("src.services.llm.function_calling.web_service.DDGS", return_value=mock_ddgs):
            input_data = create_web_search_input(query="nonexistent")
            result = await service.search_web(input_data)

        assert result.result_count == 0
        assert len(result.results) == 0

    @pytest.mark.asyncio()
    async def test_fetch_with_redirect(self) -> None:
        """Test fetch handles URL redirects."""
        service = WebService(cache_enabled=False)
        await service.initialize()

        html_content = create_test_html()

        mock_response = MagicMock()
        mock_response.text = html_content
        # Redirected URL
        mock_response.url = "https://example.com/redirected"
        mock_response.headers = {"content-type": "text/html"}
        mock_response.raise_for_status = MagicMock()

        with patch.object(service.http_client, "get", new=AsyncMock(return_value=mock_response)):
            input_data = create_fetch_url_content_input(url="https://example.com/original")
            result = await service.fetch_url_content(input_data)

        # Should return final URL after redirect
        assert result.url == "https://example.com/redirected"

        await service.cleanup()

    @pytest.mark.asyncio()
    async def test_fetch_without_title(self) -> None:
        """Test fetch handles HTML without title."""
        service = WebService(cache_enabled=False)
        await service.initialize()

        html = "<html><body><p>Content without title</p></body></html>"

        mock_response = MagicMock()
        mock_response.text = html
        mock_response.url = "https://example.com"
        mock_response.headers = {"content-type": "text/html"}
        mock_response.raise_for_status = MagicMock()

        with patch.object(service.http_client, "get", new=AsyncMock(return_value=mock_response)):
            input_data = create_fetch_url_content_input(url="https://example.com")
            result = await service.fetch_url_content(input_data)

        assert result.title is None

        await service.cleanup()
