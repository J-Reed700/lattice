"""Stealth HTTP utilities for anti-bot evasion.

Provides browser-like request profiles, user-agent rotation, proxy support,
random delays, and FlareSolverr integration.

Configuration (environment variables):
  RECALL_PROXY_URL           - Single residential proxy URL
  RECALL_PROXY_URLS          - Comma-separated proxy URLs for rotation
  RECALL_FLARESOLVERR_URL    - FlareSolverr endpoint (e.g. http://localhost:8191)
  RECALL_STEALTH_MIN_DELAY_MS - Min inter-request delay (default: 500)
  RECALL_STEALTH_MAX_DELAY_MS - Max inter-request delay (default: 2000)
"""

from __future__ import annotations

import asyncio
import logging
import os
import random
from dataclasses import dataclass

import httpx

logger = logging.getLogger(__name__)

# ─── Browser Profiles ────────────────────────────────────────────────────────

CHROME_ACCEPT = (
    "text/html,application/xhtml+xml,application/xml;q=0.9,"
    "image/avif,image/webp,image/apng,*/*;q=0.8,"
    "application/signed-exchange;v=b3;q=0.7"
)
FIREFOX_ACCEPT = (
    "text/html,application/xhtml+xml,application/xml;q=0.9,"
    "image/avif,image/webp,*/*;q=0.8"
)
SAFARI_ACCEPT = "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"


@dataclass(frozen=True)
class BrowserProfile:
    user_agent: str
    sec_ch_ua: str | None
    sec_ch_ua_mobile: str
    sec_ch_ua_platform: str
    accept: str
    accept_language: str


BROWSER_PROFILES: list[BrowserProfile] = [
    BrowserProfile(
        user_agent="Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36",
        sec_ch_ua='"Chromium";v="122", "Not(A:Brand";v="24", "Google Chrome";v="122"',
        sec_ch_ua_mobile="?0",
        sec_ch_ua_platform='"macOS"',
        accept=CHROME_ACCEPT,
        accept_language="en-US,en;q=0.9",
    ),
    BrowserProfile(
        user_agent="Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/121.0.0.0 Safari/537.36",
        sec_ch_ua='"Not A(Brand";v="99", "Google Chrome";v="121", "Chromium";v="121"',
        sec_ch_ua_mobile="?0",
        sec_ch_ua_platform='"Windows"',
        accept=CHROME_ACCEPT,
        accept_language="en-US,en;q=0.9",
    ),
    BrowserProfile(
        user_agent="Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36",
        sec_ch_ua='"Chromium";v="122", "Not(A:Brand";v="24", "Google Chrome";v="122"',
        sec_ch_ua_mobile="?0",
        sec_ch_ua_platform='"Linux"',
        accept=CHROME_ACCEPT,
        accept_language="en-US,en;q=0.9",
    ),
    BrowserProfile(
        user_agent="Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:123.0) Gecko/20100101 Firefox/123.0",
        sec_ch_ua=None,
        sec_ch_ua_mobile="?0",
        sec_ch_ua_platform='"macOS"',
        accept=FIREFOX_ACCEPT,
        accept_language="en-US,en;q=0.5",
    ),
    BrowserProfile(
        user_agent="Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:123.0) Gecko/20100101 Firefox/123.0",
        sec_ch_ua=None,
        sec_ch_ua_mobile="?0",
        sec_ch_ua_platform='"Windows"',
        accept=FIREFOX_ACCEPT,
        accept_language="en-US,en;q=0.5",
    ),
    BrowserProfile(
        user_agent="Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.3 Safari/605.1.15",
        sec_ch_ua=None,
        sec_ch_ua_mobile="?0",
        sec_ch_ua_platform='"macOS"',
        accept=SAFARI_ACCEPT,
        accept_language="en-US,en;q=0.9",
    ),
    BrowserProfile(
        user_agent="Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36 Edg/122.0.0.0",
        sec_ch_ua='"Chromium";v="122", "Not(A:Brand";v="24", "Microsoft Edge";v="122"',
        sec_ch_ua_mobile="?0",
        sec_ch_ua_platform='"Windows"',
        accept=CHROME_ACCEPT,
        accept_language="en-US,en;q=0.9",
    ),
]

_profile_counter = 0


def next_profile() -> BrowserProfile:
    """Round-robin profile selection."""
    global _profile_counter  # noqa: PLW0603
    idx = _profile_counter % len(BROWSER_PROFILES)
    _profile_counter += 1
    return BROWSER_PROFILES[idx]


def random_profile() -> BrowserProfile:
    """Random profile selection."""
    return random.choice(BROWSER_PROFILES)


# ─── Header Builders ─────────────────────────────────────────────────────────

def browser_headers(
    profile: BrowserProfile,
    referer: str | None = None,
) -> dict[str, str]:
    """Build a full set of browser-like headers from a profile."""
    headers: dict[str, str] = {
        "User-Agent": profile.user_agent,
        "Accept": profile.accept,
        "Accept-Language": profile.accept_language,
        "Accept-Encoding": "gzip, deflate, br",
        "DNT": "1",
        "Connection": "keep-alive",
        "Upgrade-Insecure-Requests": "1",
        "Sec-Fetch-Dest": "document",
        "Sec-Fetch-Mode": "navigate",
        "Sec-Fetch-User": "?1",
    }

    if profile.sec_ch_ua:
        headers["Sec-CH-UA"] = profile.sec_ch_ua
        headers["Sec-CH-UA-Mobile"] = profile.sec_ch_ua_mobile
        headers["Sec-CH-UA-Platform"] = profile.sec_ch_ua_platform

    if referer:
        headers["Referer"] = referer
        headers["Sec-Fetch-Site"] = "cross-site"
    else:
        headers["Sec-Fetch-Site"] = "none"

    return headers


def search_headers(
    profile: BrowserProfile,
    search_engine_referer: str | None = None,
) -> dict[str, str]:
    """Browser headers with search-engine-specific additions."""
    headers = browser_headers(profile, referer=None)
    headers["Cache-Control"] = "no-cache"
    headers["Pragma"] = "no-cache"

    if search_engine_referer:
        headers["Referer"] = search_engine_referer
        headers["Sec-Fetch-Site"] = "same-origin"

    return headers


# ─── Random Delay ─────────────────────────────────────────────────────────────

async def random_delay(default_min_ms: int = 500, default_max_ms: int = 2000) -> None:
    """Async sleep for a random duration to mimic human browsing."""
    min_ms = int(os.environ.get("RECALL_STEALTH_MIN_DELAY_MS", default_min_ms))
    max_ms = int(os.environ.get("RECALL_STEALTH_MAX_DELAY_MS", default_max_ms))
    max_ms = max(max_ms, min_ms)

    delay_s = random.uniform(min_ms / 1000, max_ms / 1000)
    logger.debug("Stealth: inter-request delay %.0fms", delay_s * 1000)
    await asyncio.sleep(delay_s)


# ─── Proxy Pool ───────────────────────────────────────────────────────────────

def _load_proxy_pool() -> list[str]:
    single = os.environ.get("RECALL_PROXY_URL", "").strip()
    if single:
        logger.info("Stealth: loaded single proxy from RECALL_PROXY_URL")
        return [single]

    multi = os.environ.get("RECALL_PROXY_URLS", "").strip()
    if multi:
        proxies = [p.strip() for p in multi.split(",") if p.strip()]
        if proxies:
            logger.info("Stealth: loaded %d proxies from RECALL_PROXY_URLS", len(proxies))
            return proxies

    return []


_PROXY_POOL: list[str] = _load_proxy_pool()


def random_proxy_url() -> str | None:
    """Pick a random proxy from the configured pool (or None)."""
    if not _PROXY_POOL:
        return None
    return random.choice(_PROXY_POOL)


def get_proxy_url() -> str | None:
    """Get a single proxy URL for httpx (compatible with all httpx versions)."""
    return random_proxy_url()


# ─── FlareSolverr Client ─────────────────────────────────────────────────────

class FlareSolverrClient:
    """Client for the FlareSolverr proxy that solves JS anti-bot challenges."""

    def __init__(self, endpoint: str):
        self.endpoint = endpoint.rstrip("/")
        self._client = httpx.AsyncClient(timeout=120.0)
        logger.info("Stealth: FlareSolverr client -> %s", self.endpoint)

    @classmethod
    def from_env(cls) -> FlareSolverrClient | None:
        url = os.environ.get("RECALL_FLARESOLVERR_URL", "").strip()
        if not url:
            return None
        return cls(url)

    async def solve(self, url: str, timeout_ms: int = 60000) -> str:
        """Send URL to FlareSolverr and return rendered HTML."""
        logger.debug("FlareSolverr: solving challenge for %s", url)
        resp = await self._client.post(
            f"{self.endpoint}/v1",
            json={"cmd": "request.get", "url": url, "maxTimeout": timeout_ms},
        )
        resp.raise_for_status()

        data = resp.json()
        if data.get("status") != "ok":
            msg = f"FlareSolverr status: {data.get('status')}"
            raise RuntimeError(msg)

        solution = data.get("solution")
        if not solution or "response" not in solution:
            msg = "FlareSolverr returned no solution"
            raise RuntimeError(msg)

        return solution["response"]


_FLARESOLVERR: FlareSolverrClient | None = FlareSolverrClient.from_env()


def flaresolverr() -> FlareSolverrClient | None:
    """Get the global FlareSolverr client (or None if unconfigured)."""
    return _FLARESOLVERR


# ─── Stealth HTTP Client Builder ─────────────────────────────────────────────

def build_stealth_httpx_client(
    timeout: float = 30.0,
    follow_redirects: bool = True,
) -> httpx.AsyncClient:
    """Create an httpx.AsyncClient with stealth features (cookies, proxy)."""
    kwargs: dict = {
        "timeout": timeout,
        "follow_redirects": follow_redirects,
        "limits": httpx.Limits(max_keepalive_connections=5, max_connections=10),
    }

    proxy_url = get_proxy_url()
    if proxy_url:
        kwargs["proxy"] = proxy_url

    return httpx.AsyncClient(**kwargs)
