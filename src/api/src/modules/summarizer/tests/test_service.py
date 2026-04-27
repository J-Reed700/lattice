"""
Tests for summarization service.

These tests verify the core summarization functionality,
model management, and error handling.
"""


from datetime import UTC, datetime

import pytest

from src.modules.summarizer.model_manager import ModelManager, ModelNotFoundError
from src.modules.summarizer.prompts import PromptBuilder
from src.modules.summarizer.service import LLAMA_CPP_AVAILABLE, SummarizerService
from src.modules.summarizer.types import SummarizerConfig, SummaryType


def _seed_catalog(manager: ModelManager) -> None:
    manager._catalog_cache = {
        "phi-3.5-mini-instruct": {
            "repo_id": "bartowski/Phi-3.5-mini-instruct-GGUF",
            "full_name": "Phi-3.5-mini-instruct",
            "parameters": 3.8,
            "languages": ["en"],
            "description": "Seeded test model",
            "quantizations": {
                "Q4_K_M": {
                    "size_gb": 2.3,
                    "size_bytes": 2_300_000_000,
                    "url": "https://huggingface.co/bartowski/Phi-3.5-mini-instruct-GGUF/resolve/main/Phi-3.5-mini-instruct-Q4_K_M.gguf",
                    "filename": "Phi-3.5-mini-instruct-Q4_K_M.gguf",
                }
            },
        }
    }
    manager._catalog_updated_at = datetime.now(UTC)


@pytest.fixture()
def config():
    """Test configuration."""
    return SummarizerConfig(
        default_model="phi-3.5-mini",
        max_context_length=2048,  # Smaller for tests
        max_tokens=256,
        threads=2,
        enable_caching=False,  # Disable for unit tests
    )


@pytest.fixture()
async def service(config):
    """Create summarizer service."""
    if not LLAMA_CPP_AVAILABLE:
        pytest.skip("llama-cpp-python is not installed")
    svc = SummarizerService(config)
    yield svc
    await svc.cleanup()


class TestSummarizerService:
    """Test summarization service functionality."""

    @pytest.mark.asyncio()
    async def test_summarize_basic(self, service):
        """Test basic summarization."""
        text = (
            """
        Artificial intelligence (AI) is intelligence demonstrated by machines,
        in contrast to the natural intelligence displayed by humans and animals.
        Leading AI textbooks define the field as the study of intelligent agents.
        """
            * 10
        )  # Make it long enough

        summary = await service.summarize(text=text, summary_type=SummaryType.TLDR, max_words=50)

        assert summary.text
        assert summary.summary_type == SummaryType.TLDR
        assert summary.source_length > 0
        assert summary.summary_length > 0
        assert summary.compression_ratio > 1
        assert summary.generation_time > 0
        assert summary.tokens_per_second > 0

    @pytest.mark.asyncio()
    async def test_summarize_types(self, service):
        """Test different summary types."""
        text = "Long text content here. " * 50

        types = [SummaryType.TLDR, SummaryType.ABSTRACTIVE, SummaryType.BULLET_POINTS]

        for summary_type in types:
            summary = await service.summarize(text=text, summary_type=summary_type)
            assert summary.summary_type == summary_type
            assert len(summary.text) > 0

    @pytest.mark.asyncio()
    async def test_summarize_invalid_input(self, service):
        """Test validation of invalid inputs."""
        # Too short
        with pytest.raises(ValueError, match="at least 100 characters"):
            await service.summarize(text="Too short")

        # Empty
        with pytest.raises(ValueError, match="at least 100 characters"):
            await service.summarize(text="")

        # Too long
        with pytest.raises(ValueError, match="maximum length of 50000 characters"):
            await service.summarize(text="x" * 60000)

    @pytest.mark.asyncio()
    async def test_batch_summarize(self, service):
        """Test batch summarization."""
        texts = [
            "Document one content. " * 20,
            "Document two content. " * 20,
            "Document three content. " * 20,
        ]

        response = await service.batch_summarize(
            texts=texts, summary_type=SummaryType.TLDR, max_words=50, parallel=True
        )

        assert response.total_count == 3
        assert response.success_count > 0
        assert len(response.summaries) > 0

    def test_list_models(self, service):
        """Test model listing."""
        _seed_catalog(service.model_manager)
        models = service.list_models()

        assert len(models) > 0

        for model in models:
            assert model.name
            assert model.full_name
            assert model.size_gb >= 0
            assert model.parameters >= 0


class TestModelManager:
    """Test model management functionality."""

    def test_list_available_models(self, config):
        """Test listing available models."""
        manager = ModelManager(config)
        _seed_catalog(manager)
        models = manager.list_available_models()

        assert len(models) > 0

        for model in models:
            assert model.name
            assert model.quantization == config.quantization

    def test_get_model_path(self, config):
        """Test model path resolution."""
        manager = ModelManager(config)
        _seed_catalog(manager)

        path = manager.get_model_path("phi-3.5-mini")
        assert path is not None
        assert "phi-3.5-mini" in str(path)

    def test_get_model_path_invalid(self, config):
        """Test invalid model name."""
        manager = ModelManager(config)
        _seed_catalog(manager)

        with pytest.raises(ModelNotFoundError):
            manager.get_model_path("nonexistent-model")

    def test_is_model_downloaded(self, config):
        """Test download status check."""
        manager = ModelManager(config)
        _seed_catalog(manager)

        # Most models won't be downloaded in test environment
        downloaded = manager.is_model_downloaded("phi-3.5-mini")
        assert isinstance(downloaded, bool)

    def test_get_disk_usage(self, config):
        """Test disk usage calculation."""
        manager = ModelManager(config)
        usage = manager.get_disk_usage()

        assert "total_bytes" in usage
        assert "total_gb" in usage
        assert "model_count" in usage
        assert usage["total_bytes"] >= 0


class TestPromptBuilder:
    """Test prompt building."""

    def test_build_prompt_types(self):
        """Test prompt generation for all summary types."""
        text = "Sample text for testing."

        for summary_type in SummaryType:
            system, user = PromptBuilder.build_prompt(
                text=text, summary_type=summary_type, max_words=100
            )

            assert system
            assert user
            assert text in user

    def test_optimize_for_context(self):
        """Test context window optimization."""
        long_text = "word " * 10000
        optimized = PromptBuilder.optimize_for_context(
            text=long_text, max_context_length=1000, reserve_output_tokens=200
        )

        # Should be truncated
        assert len(optimized) < len(long_text)

    def test_chunk_long_text(self):
        """Test text chunking."""
        long_text = "sentence. " * 1000
        chunks = PromptBuilder.chunk_long_text(text=long_text, chunk_size=100, overlap=20)

        assert len(chunks) > 1
        for chunk in chunks:
            assert len(chunk) <= 120  # chunk_size + some margin

    def test_validate_summary(self):
        """Test summary validation."""
        # Valid abstractive
        assert PromptBuilder.validate_summary(
            "This is a valid summary with enough words to pass.",
            SummaryType.ABSTRACTIVE,
            min_words=5,
        )

        # Too short
        assert not PromptBuilder.validate_summary("Short", SummaryType.ABSTRACTIVE, min_words=10)

        # Valid bullet points
        assert PromptBuilder.validate_summary(
            "• Point one\n• Point two\n• Point three", SummaryType.BULLET_POINTS, min_words=5
        )


@pytest.mark.integration()
class TestSummarizationIntegration:
    """Integration tests requiring downloaded models."""

    @pytest.mark.asyncio()
    async def test_full_summarization_pipeline(self, service, request):
        """Test complete summarization pipeline."""
        if not request.config.getoption("--run-integration", default=False):
            pytest.skip("Requires downloaded model")

        # Initialize service (downloads model if needed)
        await service.initialize()

        # Create a realistic document
        text = (
            """
        The Industrial Revolution was a period of major industrialization
        and innovation during the late 1700s and early 1800s. The Industrial
        Revolution began in Great Britain and quickly spread throughout the
        world. The American Industrial Revolution, commonly referred to as
        the Second Industrial Revolution, started sometime between 1820 and
        1870. This time period saw the mechanization of agriculture and
        textile manufacturing and a revolution in power, including steamships
        and railroads, that affected social, cultural, and economic conditions.
        """
            * 5
        )

        # Generate different summary types
        summaries = {}
        for summary_type in [SummaryType.TLDR, SummaryType.ABSTRACTIVE, SummaryType.BULLET_POINTS]:
            summary = await service.summarize(
                text=text,
                summary_type=summary_type,
                max_words=100 if summary_type != SummaryType.TLDR else 50,
            )
            summaries[summary_type] = summary

        # Verify all summaries
        for summary_type, summary in summaries.items():
            assert summary.text
            assert summary.summary_type == summary_type
            assert summary.compression_ratio > 1
            assert summary.model
            assert summary.language

        # TL;DR should be shortest
        assert (
            summaries[SummaryType.TLDR].summary_length
            < summaries[SummaryType.ABSTRACTIVE].summary_length
        )
