"""
Fixtures for middleware tests.

These tests are isolated unit tests that use mocks and don't require
the full application setup.
"""

from unittest.mock import AsyncMock, Mock

import pytest


@pytest.fixture()
def mock_redis():
    """Mock Redis client for rate limiting tests."""
    mock = AsyncMock()
    mock.incr = AsyncMock(return_value=1)
    mock.expire = AsyncMock()
    mock.ttl = AsyncMock(return_value=60)
    mock.delete = AsyncMock()
    return mock


@pytest.fixture()
def mock_request():
    """Mock FastAPI request."""
    request = Mock()
    request.client = Mock()
    request.client.host = "192.168.1.1"
    request.method = "GET"
    request.url = Mock()
    request.url.path = "/"
    request.cookies = {}
    request.headers = {}
    request.state = Mock()
    return request


@pytest.fixture()
def mock_user():
    """Mock authenticated user."""
    user = Mock()
    user.id = 123
    user.email = "test@example.com"
    user.username = "testuser"
    return user
