from unittest.mock import AsyncMock, Mock, patch

from httpx import AsyncClient
import pytest


class TestAgentQueryEndpoint:
    @pytest.mark.asyncio()
    async def test_query_success(self, client: AsyncClient):
        with patch("src.api.routes.agent.OllamaService") as mock_ollama, patch(
            "src.api.routes.agent.BaseLangChainService"
        ) as mock_langchain:
            mock_ollama_instance = AsyncMock()
            mock_ollama_instance.health_check.return_value = True
            mock_ollama.__aenter__.return_value = mock_ollama_instance
            mock_ollama.return_value.__aenter__.return_value = mock_ollama_instance

            mock_agent_response = Mock()
            mock_agent_response.success = True
            mock_agent_response.output = "This is a test answer based on the documents."
            mock_agent_response.intermediate_steps = [
                {
                    "observation": {
                        "documents": [
                            {
                                "path": "/test/doc1.txt",
                                "filename": "doc1.txt",
                                "snippet": "Test content snippet",
                                "score": 0.89,
                                "page_number": None,
                            }
                        ]
                    }
                }
            ]
            mock_agent_response.metadata = {"token_count": 450}

            mock_langchain_instance = Mock()
            mock_langchain_instance.invoke_agent = AsyncMock(return_value=mock_agent_response)
            mock_langchain.return_value = mock_langchain_instance

            response = await client.post(
                "/api/v1/agent/query",
                json={
                    "query": "What are the main findings?",
                    "context_limit": 5,
                    "use_local_llm": True,
                    "streaming": False,
                },
            )

            assert response.status_code == 200
            data = response.json()
            assert "answer" in data
            assert "sources" in data
            assert "confidence" in data
            assert "query_time_ms" in data
            assert data["answer"] == "This is a test answer based on the documents."
            assert len(data["sources"]) > 0
            assert 0.0 <= data["confidence"] <= 1.0

    @pytest.mark.asyncio()
    async def test_query_empty_query_validation(self, client: AsyncClient):
        response = await client.post("/api/v1/agent/query", json={"query": "", "context_limit": 5})

        assert response.status_code == 400
        data = response.json()
        assert "Query cannot be empty" in data["detail"]["message"]

    @pytest.mark.asyncio()
    async def test_query_whitespace_only_validation(self, client: AsyncClient):
        response = await client.post(
            "/api/v1/agent/query", json={"query": "   ", "context_limit": 5}
        )

        assert response.status_code == 400

    @pytest.mark.asyncio()
    async def test_query_ollama_unavailable(self, client: AsyncClient):
        with patch("src.api.routes.agent.OllamaService") as mock_ollama:
            mock_ollama_instance = AsyncMock()
            mock_ollama_instance.health_check.return_value = False
            mock_ollama.__aenter__.return_value = mock_ollama_instance
            mock_ollama.return_value.__aenter__.return_value = mock_ollama_instance

            response = await client.post(
                "/api/v1/agent/query", json={"query": "Test query", "context_limit": 5}
            )

            assert response.status_code == 503
            data = response.json()
            assert "Ollama" in data["detail"]

    @pytest.mark.asyncio()
    async def test_query_agent_execution_failed(self, client: AsyncClient):
        with patch("src.api.routes.agent.OllamaService") as mock_ollama, patch(
            "src.api.routes.agent.BaseLangChainService"
        ) as mock_langchain:
            mock_ollama_instance = AsyncMock()
            mock_ollama_instance.health_check.return_value = True
            mock_ollama.__aenter__.return_value = mock_ollama_instance
            mock_ollama.return_value.__aenter__.return_value = mock_ollama_instance

            mock_agent_response = Mock()
            mock_agent_response.success = False
            mock_agent_response.error = "Failed to process query"

            mock_langchain_instance = Mock()
            mock_langchain_instance.invoke_agent = AsyncMock(return_value=mock_agent_response)
            mock_langchain.return_value = mock_langchain_instance

            response = await client.post(
                "/api/v1/agent/query", json={"query": "Test query", "context_limit": 5}
            )

            assert response.status_code == 500
            data = response.json()
            assert "Agent execution failed" in data["detail"]

    @pytest.mark.asyncio()
    async def test_query_with_context_limit(self, client: AsyncClient):
        with patch("src.api.routes.agent.OllamaService") as mock_ollama, patch(
            "src.api.routes.agent.BaseLangChainService"
        ) as mock_langchain:
            mock_ollama_instance = AsyncMock()
            mock_ollama_instance.health_check.return_value = True
            mock_ollama.__aenter__.return_value = mock_ollama_instance
            mock_ollama.return_value.__aenter__.return_value = mock_ollama_instance

            mock_agent_response = Mock()
            mock_agent_response.success = True
            mock_agent_response.output = "Answer"
            mock_agent_response.intermediate_steps = []
            mock_agent_response.metadata = {}

            mock_langchain_instance = Mock()
            mock_langchain_instance.invoke_agent = AsyncMock(return_value=mock_agent_response)
            mock_langchain.return_value = mock_langchain_instance

            response = await client.post(
                "/api/v1/agent/query",
                json={"query": "Test query", "context_limit": 10, "use_local_llm": True},
            )

            assert response.status_code == 200

            call_args = mock_langchain_instance.invoke_agent.call_args
            assert call_args.kwargs["additional_context"]["context_limit"] == 10

    @pytest.mark.asyncio()
    async def test_query_rate_limit_applied(self, client: AsyncClient):
        with patch("src.api.routes.agent.check_rate_limit_by_key") as mock_rate_limit:
            mock_rate_limit.return_value = (False, 30)

            response = await client.post("/api/v1/agent/query", json={"query": "Test query"})

            assert response.status_code == 429


class TestAgentStatusEndpoint:
    @pytest.mark.asyncio()
    async def test_status_success_ollama_available(self, client: AsyncClient):
        with patch("src.api.routes.agent.OllamaService") as mock_ollama, patch(
            "src.api.routes.agent.BaseLangChainService"
        ) as mock_langchain:
            mock_ollama_instance = AsyncMock()
            mock_ollama_instance.health_check.return_value = True
            mock_ollama_instance.list_available_models.return_value = [
                "llama3.2",
                "mistral",
                "llama2",
            ]
            mock_ollama.__aenter__.return_value = mock_ollama_instance
            mock_ollama.return_value.__aenter__.return_value = mock_ollama_instance

            mock_langchain_instance = Mock()
            mock_langchain_instance.get_config_info.return_value = {"initialized": True}
            mock_langchain.return_value = mock_langchain_instance

            response = await client.get("/api/v1/agent/status")

            assert response.status_code == 200
            data = response.json()
            assert data["ollama_available"] is True
            assert "ollama_models" in data
            assert len(data["ollama_models"]) == 3
            assert "llama3.2" in data["ollama_models"]
            assert "integrations" in data
            assert data["agent_initialized"] is True
            assert "default_model" in data

    @pytest.mark.asyncio()
    async def test_status_ollama_unavailable(self, client: AsyncClient):
        with patch("src.api.routes.agent.OllamaService") as mock_ollama, patch(
            "src.api.routes.agent.BaseLangChainService"
        ) as mock_langchain:
            mock_ollama_instance = AsyncMock()
            mock_ollama_instance.health_check.return_value = False
            mock_ollama.__aenter__.return_value = mock_ollama_instance
            mock_ollama.return_value.__aenter__.return_value = mock_ollama_instance

            mock_langchain_instance = Mock()
            mock_langchain_instance.get_config_info.return_value = {"initialized": False}
            mock_langchain.return_value = mock_langchain_instance

            response = await client.get("/api/v1/agent/status")

            assert response.status_code == 200
            data = response.json()
            assert data["ollama_available"] is False
            assert data["ollama_models"] == []

    @pytest.mark.asyncio()
    async def test_status_integrations_structure(self, client: AsyncClient):
        with patch("src.api.routes.agent.OllamaService") as mock_ollama, patch(
            "src.api.routes.agent.BaseLangChainService"
        ) as mock_langchain:
            mock_ollama_instance = AsyncMock()
            mock_ollama_instance.health_check.return_value = True
            mock_ollama_instance.list_available_models.return_value = []
            mock_ollama.__aenter__.return_value = mock_ollama_instance
            mock_ollama.return_value.__aenter__.return_value = mock_ollama_instance

            mock_langchain_instance = Mock()
            mock_langchain_instance.get_config_info.return_value = {"initialized": True}
            mock_langchain.return_value = mock_langchain_instance

            response = await client.get("/api/v1/agent/status")

            assert response.status_code == 200
            data = response.json()
            integrations = data["integrations"]

            assert "google_drive" in integrations
            assert "dropbox" in integrations
            assert "local_files" in integrations

            for integration_name, integration in integrations.items():
                assert "enabled" in integration
                assert "connected" in integration
                assert "last_sync" in integration or integration["last_sync"] is None
                assert "document_count" in integration

            assert integrations["local_files"]["enabled"] is True
            assert integrations["local_files"]["connected"] is True


class TestAgentSyncEndpoint:
    @pytest.mark.asyncio()
    async def test_sync_local_files_success(self, client: AsyncClient):
        response = await client.post(
            "/api/v1/agent/integrations/sync",
            json={"integration_name": "local_files", "force": False},
        )

        assert response.status_code == 200
        data = response.json()
        assert data["integration_name"] == "local_files"
        assert "documents_synced" in data
        assert "sync_time_ms" in data
        assert "last_sync" in data
        assert data["status"] == "success"
        assert data["errors"] == []

    @pytest.mark.asyncio()
    async def test_sync_google_drive_not_implemented(self, client: AsyncClient):
        response = await client.post(
            "/api/v1/agent/integrations/sync",
            json={"integration_name": "google_drive", "force": False},
        )

        assert response.status_code == 200
        data = response.json()
        assert data["integration_name"] == "google_drive"
        assert data["status"] == "failed"
        assert len(data["errors"]) > 0
        assert "not yet implemented" in data["errors"][0]

    @pytest.mark.asyncio()
    async def test_sync_dropbox_not_implemented(self, client: AsyncClient):
        response = await client.post(
            "/api/v1/agent/integrations/sync", json={"integration_name": "dropbox", "force": False}
        )

        assert response.status_code == 200
        data = response.json()
        assert data["status"] == "failed"
        assert "not yet implemented" in data["errors"][0]

    @pytest.mark.asyncio()
    async def test_sync_invalid_integration_name(self, client: AsyncClient):
        response = await client.post(
            "/api/v1/agent/integrations/sync",
            json={"integration_name": "invalid_integration", "force": False},
        )

        assert response.status_code == 400
        data = response.json()
        assert "Invalid integration name" in data["detail"]["message"]

    @pytest.mark.asyncio()
    async def test_sync_with_force_flag(self, client: AsyncClient):
        response = await client.post(
            "/api/v1/agent/integrations/sync",
            json={"integration_name": "local_files", "force": True},
        )

        assert response.status_code == 200
        data = response.json()
        assert data["status"] == "success"

    @pytest.mark.asyncio()
    async def test_sync_validation_missing_integration_name(self, client: AsyncClient):
        response = await client.post("/api/v1/agent/integrations/sync", json={"force": False})

        assert response.status_code == 422

    @pytest.mark.asyncio()
    async def test_sync_rate_limit_applied(self, client: AsyncClient):
        with patch("src.api.routes.agent.check_rate_limit_by_key") as mock_rate_limit:
            mock_rate_limit.return_value = (False, 30)

            response = await client.post(
                "/api/v1/agent/integrations/sync",
                json={"integration_name": "local_files", "force": False},
            )

            assert response.status_code == 429
