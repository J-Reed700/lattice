"""Conftest for event integration tests."""

import pytest


@pytest.fixture
def tmp_path_factory(tmp_path):
    """Provide tmp_path for tests."""
    return tmp_path
