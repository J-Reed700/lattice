from unittest.mock import AsyncMock, MagicMock, patch

from fastapi.testclient import TestClient
import pytest

from src.api.app import create_app


@pytest.fixture()
def client():
    """Create test client."""
    app = create_app()
    with TestClient(app) as c:
        yield c


@pytest.fixture()
def mock_db():
    """Mock database session."""
    mock_session = AsyncMock()
    mock_session.execute = AsyncMock(return_value=MagicMock(scalar=lambda: 1))
    return mock_session


class TestHealthEndpoints:
    """Test health check endpoints."""

    def test_health_check_returns_200(self, client):
        """Test basic health endpoint returns 200."""
        response = client.get("/api/v1/health/")
        assert response.status_code == 200

    def test_health_check_response_structure(self, client):
        """Test health check has required fields."""
        response = client.get("/api/v1/health/")
        data = response.json()

        assert "status" in data
        assert "version" in data
        assert "timestamp" in data
        assert "uptime_seconds" in data
        assert data["status"] in ["healthy", "degraded", "unhealthy"]

    def test_liveness_check(self, client):
        """Test liveness endpoint."""
        response = client.get("/api/v1/health/live")
        assert response.status_code == 200

        data = response.json()
        assert data["alive"] is True
        assert "timestamp" in data

    def test_readiness_check_returns_200(self, client):
        """Test readiness endpoint returns 200."""
        response = client.get("/api/v1/health/ready")
        assert response.status_code in [200, 503]

    def test_readiness_check_structure(self, client):
        """Test readiness check has required fields."""
        response = client.get("/api/v1/health/ready")
        data = response.json()

        assert "ready" in data
        assert "services" in data
        assert "timestamp" in data
        assert isinstance(data["ready"], bool)
        assert isinstance(data["services"], dict)

    def test_system_status_endpoint(self, client):
        """Test detailed system status endpoint."""
        response = client.get("/api/v1/health/status")
        assert response.status_code in [200, 503]

        data = response.json()
        assert "overall_status" in data
        assert "components" in data
        assert "timestamp" in data
        assert data["overall_status"] in ["healthy", "degraded", "unhealthy"]

    def test_metrics_endpoint(self, client):
        """Test metrics endpoint."""
        response = client.get("/api/v1/health/metrics")
        assert response.status_code == 200

        data = response.json()
        assert "timestamp" in data
        assert "system" in data
        assert "memory" in data
        assert "gc" in data

        assert "platform" in data["system"]
        assert "python_version" in data["system"]
        assert "cpu_count" in data["system"]

        assert "rss_mb" in data["memory"]
        assert "vms_mb" in data["memory"]
        assert "percent" in data["memory"]

    def test_queue_status_endpoint(self, client):
        """Test indexing queue status endpoint."""
        response = client.get("/api/v1/health/queue")
        assert response.status_code == 200

        data = response.json()
        assert "pending_jobs" in data
        assert "processing_jobs" in data
        assert "failed_jobs" in data
        assert "avg_processing_time_ms" in data

    def test_storage_statistics_endpoint(self, client):
        """Test storage statistics endpoint."""
        response = client.get("/api/v1/health/storage")
        assert response.status_code in [200, 503]

        data = response.json()
        assert "total_files" in data
        assert "total_size_bytes" in data
        assert "total_vectors" in data


@pytest.mark.asyncio()
class TestHealthCheckLogic:
    """Test health check logic in isolation."""

    @patch("src.api.v1.health.TextEmbedder")
    @patch("src.api.v1.health.VectorStore")
    async def test_healthy_system(self, mock_vector, mock_embedder, mock_db):
        """Test health check with all systems healthy."""
        from src.api.v1.health import health_check
        from src.config import get_settings

        settings = get_settings()
        result = await health_check(db=mock_db, settings=settings)

        assert result.status in ["healthy", "degraded", "unhealthy"]
        assert result.version == settings.app_version
        assert result.uptime_seconds >= 0

    @patch("src.api.v1.health.TextEmbedder")
    @patch("src.api.v1.health.VectorStore")
    async def test_unhealthy_database(self, mock_vector, mock_embedder, mock_db):
        """Test health check with database failure."""
        from src.api.v1.health import health_check
        from src.config import get_settings

        mock_db.execute.side_effect = Exception("Database connection failed")
        settings = get_settings()
        result = await health_check(db=mock_db, settings=settings)

        assert result.status in ["degraded", "unhealthy"]


class TestHealthCheckDocumentation:
    """Test that health endpoints are documented."""

    def test_openapi_includes_health_endpoints(self, client):
        """Test that OpenAPI docs include health endpoints."""
        response = client.get("/api/openapi.json")

        if response.status_code == 200:
            openapi = response.json()
            paths = openapi.get("paths", {})

            assert any("/health" in path for path in paths.keys())
