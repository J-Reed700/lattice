from unittest.mock import AsyncMock, MagicMock, patch

from PIL import Image
import pytest

from src.modules.content_extractor.extractors.image import extract_image
from src.modules.content_extractor.extractors.ocr_service import (
    OcrResult,
    OcrService,
    get_ocr_service,
)
from src.modules.content_extractor.types import (
    CorruptedFileError,
    ExtractedContent,
    ExtractionError,
    OcrConfig,
    OcrEngine,
)


@pytest.mark.unit()
class TestOcrService:
    @pytest.mark.asyncio()
    async def test_ocr_service_disabled(self, sample_image_file):
        config = OcrConfig(enabled=False)
        service = OcrService(config)
        result = await service.extract_text(str(sample_image_file))

        assert result.text == ""
        assert result.confidence == 0.0
        assert result.has_text is False
        assert result.engine_used == "disabled"
        assert result.word_count == 0

    @pytest.mark.asyncio()
    async def test_ocr_service_text_detection_skip(self, sample_photo_file):
        config = OcrConfig(enabled=True, detect_text_first=True)
        service = OcrService(config)

        with patch.object(service, "_has_text_content", return_value=False):
            result = await service.extract_text(str(sample_photo_file))

        assert result.text == ""
        assert result.has_text is False
        assert result.engine_used == "detection"

    @pytest.mark.asyncio()
    async def test_ocr_service_tesseract_engine(self, sample_screenshot_file):
        config = OcrConfig(enabled=True, engine=OcrEngine.TESSERACT)
        service = OcrService(config)

        mock_result = OcrResult(
            text="Sample Text",
            confidence=0.85,
            has_text=True,
            engine_used="tesseract",
            word_count=2,
            language="eng",
        )

        with patch.object(service, "_extract_with_tesseract", return_value=mock_result):
            result = await service.extract_text(str(sample_screenshot_file))

        assert result.text == "Sample Text"
        assert result.confidence == 0.85
        assert result.has_text is True
        assert result.engine_used == "tesseract"

    @pytest.mark.asyncio()
    async def test_ocr_service_vlm_engine(self, sample_screenshot_file):
        config = OcrConfig(enabled=True, engine=OcrEngine.VLM)
        service = OcrService(config)

        mock_result = OcrResult(
            text="Document Text from VLM",
            confidence=0.92,
            has_text=True,
            engine_used="vlm",
            word_count=4,
            language=None,
        )

        with patch.object(service, "_extract_with_vlm", return_value=mock_result):
            result = await service.extract_text(str(sample_screenshot_file))

        assert "Document Text" in result.text
        assert result.confidence >= 0.5
        assert result.engine_used == "vlm"

    @pytest.mark.asyncio()
    async def test_ocr_service_vlm_fallback_to_tesseract(self, sample_screenshot_file):
        config = OcrConfig(enabled=True, engine=OcrEngine.VLM)
        service = OcrService(config)

        mock_tesseract_result = OcrResult(
            text="Fallback Text",
            confidence=0.7,
            has_text=True,
            engine_used="tesseract",
            word_count=2,
            language="eng",
        )

        with patch.object(service, "_extract_with_vlm", side_effect=ExtractionError("VLM failed")):
            with patch.object(
                service, "_extract_with_tesseract", return_value=mock_tesseract_result
            ):
                result = await service.extract_text(str(sample_screenshot_file))

        assert result.text == "Fallback Text"
        assert result.engine_used == "tesseract"

    def test_preprocess_image_rgb_conversion(self):
        config = OcrConfig()
        service = OcrService(config)

        img = Image.new("RGBA", (100, 100))
        processed = service._preprocess_image(img)

        assert processed.mode == "RGB"

    def test_preprocess_image_resize(self):
        config = OcrConfig(max_image_size=500)
        service = OcrService(config)

        img = Image.new("RGB", (1000, 1000))
        processed = service._preprocess_image(img)

        assert processed.width <= 500
        assert processed.height <= 500

    def test_has_text_content_photo(self):
        config = OcrConfig()
        service = OcrService(config)

        img = Image.new("RGB", (800, 600))
        for x in range(0, 800, 10):
            for y in range(0, 600, 10):
                img.putpixel((x, y), (x % 256, y % 256, (x + y) % 256))

        result = service._has_text_content(img)
        assert isinstance(result, bool)

    def test_has_text_content_document(self):
        config = OcrConfig()
        service = OcrService(config)

        img = Image.new("L", (2000, 800), color=255)
        result = service._has_text_content(img)

        assert result is True

    def test_get_ocr_service_singleton(self):
        config1 = OcrConfig(enabled=True)
        service1 = get_ocr_service(config1)
        service2 = get_ocr_service()

        assert service1 is service2


@pytest.mark.unit()
class TestImageExtractionWithOcr:
    def test_extract_image_without_ocr(self, sample_image_file):
        config = OcrConfig(enabled=False)
        content = extract_image(str(sample_image_file), config)

        assert isinstance(content, ExtractedContent)
        assert content.text == ""
        assert content.mime_type.startswith("image/")
        assert "width" in content.metadata
        assert "height" in content.metadata

    def test_extract_image_with_ocr_enabled(self, sample_screenshot_file):
        config = OcrConfig(enabled=True, engine=OcrEngine.TESSERACT)

        mock_result = OcrResult(
            text="Screenshot Text Content",
            confidence=0.88,
            has_text=True,
            engine_used="tesseract",
            word_count=3,
            language="eng",
        )

        with patch(
            "src.modules.content_extractor.extractors.image.get_ocr_service"
        ) as mock_get_service:
            mock_service = MagicMock()
            mock_service.extract_text = AsyncMock(return_value=mock_result)
            mock_get_service.return_value = mock_service

            content = extract_image(str(sample_screenshot_file), config)

        assert content.text == "Screenshot Text Content"
        assert "ocr" in content.metadata
        assert content.metadata["ocr"]["has_text"] is True
        assert content.metadata["ocr"]["confidence"] == 0.88
        assert content.metadata["ocr"]["engine"] == "tesseract"
        assert content.metadata["ocr"]["word_count"] == 3

    def test_extract_image_ocr_failure_graceful_fallback(self, sample_image_file):
        config = OcrConfig(enabled=True)

        with patch(
            "src.modules.content_extractor.extractors.image.get_ocr_service"
        ) as mock_get_service:
            mock_service = MagicMock()
            mock_service.extract_text = AsyncMock(
                side_effect=ExtractionError("OCR failed", file_path=str(sample_image_file))
            )
            mock_get_service.return_value = mock_service

            content = extract_image(str(sample_image_file), config)

        assert content.text == ""
        assert "ocr" in content.metadata
        assert content.metadata["ocr"]["engine"] == "failed"
        assert "error" in content.metadata["ocr"]

    def test_extract_image_preserves_metadata(self, sample_image_file):
        config = OcrConfig(enabled=True)

        with patch(
            "src.modules.content_extractor.extractors.image.get_ocr_service"
        ) as mock_get_service:
            mock_service = MagicMock()
            mock_result = OcrResult(
                text="Test",
                confidence=0.9,
                has_text=True,
                engine_used="vlm",
                word_count=1,
                language="eng",
            )
            mock_service.extract_text = AsyncMock(return_value=mock_result)
            mock_get_service.return_value = mock_service

            content = extract_image(str(sample_image_file), config)

        assert "width" in content.metadata
        assert "height" in content.metadata
        assert "format" in content.metadata
        assert "ocr" in content.metadata

    def test_extract_image_default_config(self, sample_image_file):
        with patch(
            "src.modules.content_extractor.extractors.image.get_ocr_service"
        ) as mock_get_service:
            mock_service = MagicMock()
            mock_result = OcrResult(
                text="Default Config Text",
                confidence=0.75,
                has_text=True,
                engine_used="vlm",
                word_count=3,
                language=None,
            )
            mock_service.extract_text = AsyncMock(return_value=mock_result)
            mock_get_service.return_value = mock_service

            content = extract_image(str(sample_image_file))

        assert content.text == "Default Config Text"

    def test_extract_image_corrupted_file(self, temp_dir):
        file_path = temp_dir / "corrupted.jpg"
        file_path.write_bytes(b"Not a valid image")

        with pytest.raises(CorruptedFileError):
            extract_image(str(file_path))


@pytest.mark.unit()
class TestOcrConfig:
    def test_ocr_config_defaults(self):
        config = OcrConfig()

        assert config.enabled is True
        assert config.engine == OcrEngine.VLM
        assert config.min_confidence == 0.5
        assert config.detect_text_first is True
        assert config.max_image_size == 2048
        assert config.model_name == "microsoft/Florence-2-base"
        assert config.languages == ["eng"]

    def test_ocr_config_custom_values(self):
        config = OcrConfig(
            enabled=False,
            engine=OcrEngine.TESSERACT,
            min_confidence=0.8,
            detect_text_first=False,
            max_image_size=1024,
            languages=["eng", "fra"],
        )

        assert config.enabled is False
        assert config.engine == OcrEngine.TESSERACT
        assert config.min_confidence == 0.8
        assert config.detect_text_first is False
        assert config.max_image_size == 1024
        assert config.languages == ["eng", "fra"]

    def test_ocr_engine_enum_values(self):
        assert OcrEngine.VLM == "vlm"
        assert OcrEngine.TESSERACT == "tesseract"
        assert OcrEngine.DISABLED == "disabled"


@pytest.mark.unit()
class TestOcrResult:
    def test_ocr_result_creation(self):
        result = OcrResult(
            text="Extracted text",
            confidence=0.95,
            has_text=True,
            engine_used="vlm",
            word_count=2,
            language="eng",
        )

        assert result.text == "Extracted text"
        assert result.confidence == 0.95
        assert result.has_text is True
        assert result.engine_used == "vlm"
        assert result.word_count == 2
        assert result.language == "eng"

    def test_ocr_result_optional_language(self):
        result = OcrResult(
            text="Text", confidence=0.9, has_text=True, engine_used="tesseract", word_count=1
        )

        assert result.language is None
