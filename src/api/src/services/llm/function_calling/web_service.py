"""Web search and URL fetching service for function calling.

This service provides web integration capabilities:
- Web search using DuckDuckGo
- URL content fetching with SSRF protection
- Response caching for performance

Security:
- SSRF prevention (no private IPs, localhost)
- URL validation and sanitization
- Content length limits
- Request timeouts
"""

from __future__ import annotations

import asyncio
from collections import OrderedDict
from datetime import datetime, timedelta
import hashlib
import ipaddress
import logging
import socket
from urllib.parse import urlparse

from bs4 import BeautifulSoup
import httpx

from src.schemas.function_calling import (
    FetchUrlContentInput,
    FetchUrlContentOutput,
    WebSearchInput,
    WebSearchOutput,
    WebSearchResult,
)
from src.services.llm.function_calling.stealth import (
    browser_headers,
    build_stealth_httpx_client,
    flaresolverr,
    random_delay,
    random_profile,
    search_headers,
)

logger = logging.getLogger(__name__)


class WebCache:
    """Simple LRU cache for web requests with TTL."""

    def __init__(self, max_size: int = 500, ttl_seconds: int = 3600):
        self.max_size = max_size
        self.ttl = timedelta(seconds=ttl_seconds)
        self._cache: OrderedDict[str, tuple[dict, datetime]] = OrderedDict()
        self._lock = asyncio.Lock()

    def _make_key(self, operation: str, **params: str) -> str:
        """Create cache key from operation and parameters."""
        key_str = f"{operation}:{params}"
        return hashlib.sha256(key_str.encode()).hexdigest()

    async def get(self, operation: str, **params: str) -> dict | None:
        """Get cached result if available and not expired."""
        key = self._make_key(operation, **params)

        async with self._lock:
            if key in self._cache:
                result, timestamp = self._cache[key]
                if datetime.now() - timestamp < self.ttl:
                    self._cache.move_to_end(key)
                    return result
                del self._cache[key]

        return None

    async def put(self, operation: str, result: dict, **params: str) -> None:
        """Store result in cache with current timestamp."""
        key = self._make_key(operation, **params)

        async with self._lock:
            if len(self._cache) >= self.max_size and key not in self._cache:
                self._cache.popitem(last=False)

            self._cache[key] = (result, datetime.now())
            self._cache.move_to_end(key)


class WebService:
    """Service for web search and URL fetching with security controls."""

    def __init__(
        self,
        cache_enabled: bool = True,
        cache_ttl: int = 3600,
        max_cache_size: int = 500,
    ):
        """
        Initialize web service.

        Args:
            cache_enabled: Whether to enable response caching
            cache_ttl: Cache TTL in seconds
            max_cache_size: Maximum number of cached responses
        """
        self.cache = (
            WebCache(max_size=max_cache_size, ttl_seconds=cache_ttl) if cache_enabled else None
        )
        self._http_client: httpx.AsyncClient | None = None

    async def initialize(self) -> None:
        """Initialize HTTP client with stealth features (cookies, proxy)."""
        if self._http_client is None:
            self._http_client = build_stealth_httpx_client(
                timeout=30.0,
                follow_redirects=True,
            )
            logger.info("WebService initialized with stealth features")

    async def cleanup(self) -> None:
        """Cleanup HTTP client."""
        if self._http_client is not None:
            await self._http_client.aclose()
            self._http_client = None
            logger.info("WebService cleaned up")

    @property
    def http_client(self) -> httpx.AsyncClient:
        """Get HTTP client."""
        if self._http_client is None:
            msg = "WebService not initialized. Call initialize() first."
            raise RuntimeError(msg)
        return self._http_client

    def _is_blocked_ip(self, ip_str: str) -> bool:
        """
        Check if IP address should be blocked (SSRF prevention).

        Uses ipaddress library for comprehensive IP validation.

        Args:
            ip_str: IP address string

        Returns:
            True if IP should be blocked
        """
        try:
            ip = ipaddress.ip_address(ip_str)

            # Block private IPs (10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16)
            if ip.is_private:
                return True

            # Block loopback (127.0.0.0/8, ::1)
            if ip.is_loopback:
                return True

            # Block link-local (169.254.0.0/16, fe80::/10)
            if ip.is_link_local:
                return True

            # Block multicast (224.0.0.0/4, ff00::/8)
            if ip.is_multicast:
                return True

            # Block reserved IPs
            if ip.is_reserved:
                return True

            # Block unspecified (0.0.0.0, ::)
            if ip.is_unspecified:
                return True

            # Explicitly block cloud metadata endpoints
            if isinstance(ip, ipaddress.IPv4Address):
                # AWS/Azure/GCP metadata
                if ip == ipaddress.IPv4Address("169.254.169.254"):
                    return True
            elif isinstance(ip, ipaddress.IPv6Address):
                # AWS IPv6 metadata
                if ip == ipaddress.IPv6Address("fd00:ec2::254"):
                    return True

            return False

        except ValueError:
            # Not a valid IP address
            return False

    def _resolve_and_validate_host(self, hostname: str) -> None:
        """
        Resolve hostname to IP and validate (prevents DNS rebinding).

        This prevents CWE-918 DNS rebinding attacks by:
        1. Resolving DNS before fetching
        2. Validating all resolved IPs
        3. Blocking if any IP is private/reserved

        Args:
            hostname: Hostname to resolve and validate

        Raises:
            ValueError: If hostname resolves to blocked IP
        """
        # Block localhost variants immediately
        if hostname.lower() in ("localhost", "0.0.0.0", "[::]"):
            msg = f"Access to localhost blocked: {hostname}"
            raise ValueError(msg)

        # Try to parse as IP address first
        try:
            ip = ipaddress.ip_address(hostname)
            if self._is_blocked_ip(str(ip)):
                msg = f"Access to private/reserved IP blocked: {hostname}"
                raise ValueError(msg)
            return
        except ValueError:
            # Not an IP address, continue to DNS resolution
            pass

        # Resolve hostname to IPs
        try:
            # Get all IP addresses for hostname
            addr_info = socket.getaddrinfo(
                hostname,
                None,
                family=socket.AF_UNSPEC,  # Both IPv4 and IPv6
                type=socket.SOCK_STREAM,
            )

            # Extract unique IPs
            resolved_ips = set()
            for info in addr_info:
                ip_str = info[4][0]
                resolved_ips.add(ip_str)

            # Validate all resolved IPs
            blocked_ips = []
            for ip_str in resolved_ips:
                if self._is_blocked_ip(ip_str):
                    blocked_ips.append(ip_str)

            if blocked_ips:
                msg = (
                    f"Hostname {hostname} resolves to blocked IP(s): {', '.join(blocked_ips)}. "
                    "Access to private networks is not allowed."
                )
                raise ValueError(msg)

            logger.debug(f"DNS validation passed for {hostname}: {resolved_ips}")

        except socket.gaierror as e:
            msg = f"Failed to resolve hostname {hostname}: {e}"
            raise ValueError(msg) from e

    def _validate_url(self, url: str) -> None:
        """
        Validate URL for SSRF prevention with DNS resolution.

        Prevents CWE-918 by resolving DNS and validating IPs.

        Args:
            url: URL to validate

        Raises:
            ValueError: If URL is invalid or blocked
        """
        try:
            parsed = urlparse(url)

            # Must have scheme and netloc
            if not parsed.scheme or not parsed.netloc:
                msg = "Invalid URL format"
                raise ValueError(msg)

            # Must be HTTP or HTTPS
            if parsed.scheme not in ("http", "https"):
                msg = f"Unsupported URL scheme: {parsed.scheme}"
                raise ValueError(msg)

            # Get hostname
            hostname = parsed.hostname or ""
            if not hostname:
                msg = "URL must have a hostname"
                raise ValueError(msg)

            # Resolve DNS and validate IPs (prevents DNS rebinding)
            self._resolve_and_validate_host(hostname)

        except Exception as e:
            logger.warning(f"URL validation failed: {url[:100]} - {e}")
            raise

    async def search_web(self, input_data: WebSearchInput) -> WebSearchOutput:
        """
        Search the web using DuckDuckGo.

        Args:
            input_data: Search parameters

        Returns:
            Web search results

        Raises:
            ImportError: If duckduckgo-search is not installed
            RuntimeError: If search fails
        """
        # Check cache
        if self.cache:
            cached = await self.cache.get(
                "search",
                query=input_data.query,
                max_results=str(input_data.max_results),
                region=input_data.region or "wt-wt",
            )
            if cached:
                logger.debug(f"Web search cache hit: {input_data.query[:50]}...")
                return WebSearchOutput(**cached)

        # Import DuckDuckGo
        try:
            from duckduckgo_search import DDGS
        except ImportError as e:
            msg = "duckduckgo-search package not installed. Install with: pip install duckduckgo-search"
            raise ImportError(msg) from e

        # Perform search with random delay
        try:
            await random_delay(300, 1500)

            profile = random_profile()
            hdrs = search_headers(profile)

            try:
                ddgs = DDGS(headers=hdrs)
            except TypeError:
                ddgs = DDGS()

            with ddgs:
                results = list(
                    ddgs.text(
                        keywords=input_data.query,
                        region=input_data.region or "wt-wt",
                        safesearch=input_data.safesearch,
                        max_results=input_data.max_results,
                    )
                )

            # Map to output schema
            search_results = []
            for r in results:
                search_results.append(
                    WebSearchResult(
                        title=r.get("title", ""),
                        url=r.get("href", ""),
                        snippet=r.get("body", "")[:500],
                        published_date=None,  # DuckDuckGo doesn't provide this reliably
                    )
                )

            output = WebSearchOutput(
                results=search_results,
                query=input_data.query,
                result_count=len(search_results),
            )

            # Cache result
            if self.cache:
                await self.cache.put(
                    "search",
                    output.model_dump(),
                    query=input_data.query,
                    max_results=str(input_data.max_results),
                    region=input_data.region or "wt-wt",
                )

            logger.info(
                f"Web search completed: {len(search_results)} results (query: {input_data.query[:50]}...)"
            )
            return output

        except Exception as e:
            logger.error(f"Web search failed: {e}", exc_info=True)
            msg = f"Web search failed: {e!s}"
            raise RuntimeError(msg) from e

    async def fetch_url_content(self, input_data: FetchUrlContentInput) -> FetchUrlContentOutput:
        """
        Fetch and extract content from a URL.

        Args:
            input_data: Fetch parameters

        Returns:
            Extracted content

        Raises:
            ValueError: If URL is invalid or blocked
            RuntimeError: If fetch fails
        """
        # Validate URL (SSRF prevention)
        self._validate_url(input_data.url)

        # Check cache
        if self.cache:
            cached = await self.cache.get(
                "fetch",
                url=input_data.url,
                extract_mode=input_data.extract_mode,
            )
            if cached:
                logger.debug(f"URL fetch cache hit: {input_data.url[:100]}")
                return FetchUrlContentOutput(**cached)

        # Fetch content with stealth features
        start_time = datetime.now()
        try:
            await random_delay(500, 2000)
            profile = random_profile()
            hdrs = browser_headers(profile)

            response = await self.http_client.get(
                input_data.url,
                headers=hdrs,
                timeout=input_data.timeout_seconds,
            )
            response.raise_for_status()

            # Re-validate final URL after redirects (prevents redirect-based SSRF)
            final_url = str(response.url)
            if final_url != input_data.url:
                logger.debug(f"URL redirected to: {final_url[:100]}")
                self._validate_url(final_url)

            # Extract content based on mode
            if input_data.extract_mode == "article":
                content, title = self._extract_article(response.text)
            elif input_data.extract_mode == "raw_text":
                content, title = self._extract_raw_text(response.text)
            else:  # markdown
                content, title = self._extract_markdown(response.text)

            # Truncate if needed
            truncated = False
            if len(content) > input_data.max_content_length:
                content = content[: input_data.max_content_length]
                truncated = True

            # Calculate metrics
            fetch_time_ms = (datetime.now() - start_time).total_seconds() * 1000
            word_count = len(content.split())

            output = FetchUrlContentOutput(
                url=str(response.url),  # May differ if redirected
                title=title,
                content=content,
                content_truncated=truncated,
                word_count=word_count,
                fetch_time_ms=fetch_time_ms,
                content_type=response.headers.get("content-type"),
            )

            # Cache result
            if self.cache:
                await self.cache.put(
                    "fetch",
                    output.model_dump(),
                    url=input_data.url,
                    extract_mode=input_data.extract_mode,
                )

            logger.info(f"URL fetch completed: {word_count} words (URL: {input_data.url[:50]}...)")
            return output

        except httpx.HTTPError as e:
            logger.error(f"URL fetch failed: {e}", exc_info=True)
            msg = f"Failed to fetch URL: {e!s}"
            raise RuntimeError(msg) from e

    def _extract_article(self, html: str) -> tuple[str, str | None]:
        """Extract main article content from HTML."""
        soup = BeautifulSoup(html, "lxml")

        # Remove script and style elements
        for script in soup(["script", "style"]):
            script.decompose()

        # Try to find main content area
        main_content = (
            soup.find("article") or soup.find("main") or soup.find("div", class_="content")
        )

        # Extract title
        title = None
        if title_tag := soup.find("title"):
            title = title_tag.get_text().strip()

        # Extract text
        if main_content:
            text = main_content.get_text(separator="\n", strip=True)
        else:
            text = soup.get_text(separator="\n", strip=True)

        # Clean up whitespace
        lines = [line.strip() for line in text.splitlines() if line.strip()]
        return "\n\n".join(lines), title

    def _extract_raw_text(self, html: str) -> tuple[str, str | None]:
        """Extract all text from HTML."""
        soup = BeautifulSoup(html, "lxml")

        # Remove script and style
        for script in soup(["script", "style"]):
            script.decompose()

        # Extract title
        title = None
        if title_tag := soup.find("title"):
            title = title_tag.get_text().strip()

        # Get all text
        text = soup.get_text(separator="\n", strip=True)
        lines = [line.strip() for line in text.splitlines() if line.strip()]
        return "\n".join(lines), title

    def _extract_markdown(self, html: str) -> tuple[str, str | None]:
        """Extract text preserving some markdown-like formatting."""
        soup = BeautifulSoup(html, "lxml")

        # Remove script and style
        for script in soup(["script", "style"]):
            script.decompose()

        # Extract title
        title = None
        if title_tag := soup.find("title"):
            title = title_tag.get_text().strip()

        # Convert headings to markdown
        for i in range(1, 7):
            for heading in soup.find_all(f"h{i}"):
                heading.string = f"{'#' * i} {heading.get_text()}\n"

        # Convert links
        for link in soup.find_all("a"):
            text = link.get_text()
            href = link.get("href", "")
            link.string = f"[{text}]({href})"

        # Get text
        text = soup.get_text(separator="\n", strip=True)
        lines = [line.strip() for line in text.splitlines() if line.strip()]
        return "\n\n".join(lines), title
