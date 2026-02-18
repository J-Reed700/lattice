import asyncio
from dataclasses import dataclass

from PIL import Image

from ..types import ExtractionError, OcrConfig, OcrEngine


@dataclass
class OcrResult:
    """Result from OCR extraction.

    Attributes:
        text: Extracted text content
        confidence: Confidence score (0.0-1.0)
        has_text: Whether text was detected in the image
        engine_used: Which OCR engine was used
        word_count: Number of words extracted
        language: Detected language code

    Example:
        >>> result = OcrResult(
        ...     text="Hello World",
        ...     confidence=0.95,
        ...     has_text=True,
        ...     engine_used="vlm",
        ...     word_count=2,
        ...     language="eng"
        ... )
    """

    text: str
    confidence: float
    has_text: bool
    engine_used: str
    word_count: int
    language: str | None = None


class OcrService:
    """OCR service supporting multiple engines (VLM and Tesseract).

    Provides text extraction from images using Vision Language Models
    or traditional OCR engines with automatic fallback.

    Attributes:
        config: OCR configuration
        _vlm_model: Cached VLM model instance
        _vlm_processor: Cached VLM processor instance

    Example:
        >>> config = OcrConfig(enabled=True, engine=OcrEngine.VLM)
        >>> service = OcrService(config)
        >>> result = await service.extract_text("/path/to/image.jpg")
        >>> print(result.text)
    """

    def __init__(self, config: OcrConfig):
        self.config = config
        self._vlm_model: object | None = None
        self._vlm_processor: object | None = None
        self._inference_count = 0
        self._cleanup_interval = 50

    def __del__(self):
        """Ensure cleanup on object destruction."""
        self.cleanup()

    def cleanup(self) -> None:
        """Cleanup VLM model and free memory.

        Deletes cached model and processor instances and clears CUDA cache
        if available. Should be called when shutting down the service.
        """
        import gc
        import logging

        logger = logging.getLogger(__name__)

        if self._vlm_model is not None:
            logger.info("Cleaning up VLM model and freeing memory...")
            del self._vlm_model
            self._vlm_model = None
        if self._vlm_processor is not None:
            del self._vlm_processor
            self._vlm_processor = None

        gc.collect()

        try:
            import torch

            if torch.cuda.is_available():
                torch.cuda.empty_cache()
                torch.cuda.synchronize()
                logger.info("CUDA cache cleared")
        except ImportError:
            pass

        logger.info("VLM cleanup complete")

    async def extract_text(self, image_path: str) -> OcrResult:
        """Extract text from an image file.

        Args:
            image_path: Path to the image file

        Returns:
            OcrResult with extracted text and metadata

        Raises:
            ExtractionError: If OCR extraction fails
        """
        if not self.config.enabled or self.config.engine == OcrEngine.DISABLED:
            return OcrResult(
                text="", confidence=0.0, has_text=False, engine_used="disabled", word_count=0
            )

        try:
            with Image.open(image_path) as img:
                img = self._preprocess_image(img)

                if self.config.detect_text_first and not self._has_text_content(img):
                    return OcrResult(
                        text="",
                        confidence=1.0,
                        has_text=False,
                        engine_used="detection",
                        word_count=0,
                    )

                if self.config.engine == OcrEngine.VLM:
                    try:
                        result = await self._extract_with_vlm(img, image_path)
                        if result.confidence >= self.config.min_confidence:
                            return result
                    except Exception as vlm_error:
                        import logging

                        logger = logging.getLogger(__name__)
                        logger.warning(f"VLM failed: {vlm_error}, falling back to Tesseract")
                        return await self._extract_with_tesseract(img, image_path)

                elif self.config.engine == OcrEngine.TESSERACT:
                    return await self._extract_with_tesseract(img, image_path)

                return OcrResult(
                    text="", confidence=0.0, has_text=False, engine_used="none", word_count=0
                )

        except Exception as e:
            raise ExtractionError(f"OCR extraction failed: {e!s}", file_path=image_path) from e

    def _preprocess_image(self, img: Image.Image) -> Image.Image:
        """Preprocess image for OCR (resize, normalize).

        Args:
            img: PIL Image object

        Returns:
            Preprocessed PIL Image
        """
        if img.mode not in ("RGB", "L"):
            img = img.convert("RGB")

        max_size = self.config.max_image_size
        if img.width > max_size or img.height > max_size:
            img.thumbnail((max_size, max_size), Image.Resampling.LANCZOS)

        return img

    def _has_text_content(self, img: Image.Image) -> bool:
        """Detect if image likely contains text content.

        Uses simple heuristics to avoid OCR on pure photos/graphics.

        Args:
            img: PIL Image object

        Returns:
            True if image likely contains text
        """
        width, height = img.size
        aspect_ratio = width / height

        if aspect_ratio > 5 or aspect_ratio < 0.2:
            return True

        gray = img.convert("L")
        pixels = list(gray.getdata())
        unique_colors = len(set(pixels))

        if unique_colors < 50:
            return False

        return True

    async def _extract_with_vlm(self, img: Image.Image, image_path: str) -> OcrResult:
        """Extract text using Vision Language Model (Florence-2).

        Args:
            img: PIL Image object
            image_path: Path to image file (for error reporting)

        Returns:
            OcrResult with extracted text

        Raises:
            ExtractionError: If VLM extraction fails
        """
        try:
            if self._vlm_model is None:
                await self._load_vlm_model()

            loop = asyncio.get_event_loop()
            result = await loop.run_in_executor(None, self._run_vlm_inference, img)

            return result

        except ImportError as e:
            raise ExtractionError(
                f"VLM dependencies not installed: {e!s}. "
                "Install with: pip install transformers torch",
                file_path=image_path,
            ) from e
        except Exception as e:
            raise ExtractionError(f"VLM extraction failed: {e!s}", file_path=image_path) from e

    async def _load_vlm_model(self) -> None:
        """Load VLM model and processor.

        Raises:
            ImportError: If transformers library not available
        """
        try:
            from transformers import AutoModelForCausalLM, AutoProcessor
        except ImportError as e:
            raise ImportError(
                "transformers library not installed. Install with: pip install transformers torch"
            ) from e

        model_name = self.config.model_name

        loop = asyncio.get_event_loop()
        self._vlm_processor = await loop.run_in_executor(
            None, AutoProcessor.from_pretrained, model_name, False
        )

        self._vlm_model = await loop.run_in_executor(
            None,
            lambda: AutoModelForCausalLM.from_pretrained(
                model_name, trust_remote_code=False
            ).eval(),
        )

    def _run_vlm_inference(self, img: Image.Image) -> OcrResult:
        """Run VLM inference on image.

        Args:
            img: PIL Image object

        Returns:
            OcrResult with extracted text
        """
        import gc

        import torch

        task_prompt = "<OCR>"

        inputs = self._vlm_processor(text=task_prompt, images=img, return_tensors="pt")

        try:
            with torch.no_grad():
                generated_ids = self._vlm_model.generate(
                    input_ids=inputs["input_ids"],
                    pixel_values=inputs["pixel_values"],
                    max_new_tokens=1024,
                    num_beams=3,
                    do_sample=False,
                )

            generated_text = self._vlm_processor.batch_decode(
                generated_ids, skip_special_tokens=False
            )[0]

            parsed_answer = self._vlm_processor.post_process_generation(
                generated_text, task=task_prompt, image_size=(img.width, img.height)
            )

            text = parsed_answer.get(task_prompt, "")

            word_count = len(text.split())
            confidence = 0.9 if word_count > 0 else 0.0

            return OcrResult(
                text=text,
                confidence=confidence,
                has_text=word_count > 0,
                engine_used="vlm",
                word_count=word_count,
                language=None,
            )

        finally:
            del inputs
            del generated_ids

            self._inference_count += 1
            if self._inference_count % self._cleanup_interval == 0:
                gc.collect()
                if torch.cuda.is_available():
                    torch.cuda.empty_cache()

    async def _extract_with_tesseract(self, img: Image.Image, image_path: str) -> OcrResult:
        """Extract text using Tesseract OCR.

        Args:
            img: PIL Image object
            image_path: Path to image file (for error reporting)

        Returns:
            OcrResult with extracted text

        Raises:
            ExtractionError: If Tesseract extraction fails
        """
        try:
            import pytesseract
        except ImportError as e:
            raise ExtractionError(
                "pytesseract not installed. Install with: pip install pytesseract",
                file_path=image_path,
            ) from e

        try:
            lang = "+".join(self.config.languages)

            loop = asyncio.get_event_loop()
            data = await loop.run_in_executor(
                None,
                lambda: pytesseract.image_to_data(
                    img, lang=lang, output_type=pytesseract.Output.DICT
                ),
            )

            confidences = [
                int(conf) for conf in data["conf"] if conf != "-1" and str(conf).isdigit()
            ]
            avg_confidence = sum(confidences) / len(confidences) / 100.0 if confidences else 0.0

            text = await loop.run_in_executor(
                None, lambda: pytesseract.image_to_string(img, lang=lang)
            )
            text = text.strip()
            word_count = len(text.split())

            return OcrResult(
                text=text,
                confidence=avg_confidence,
                has_text=word_count > 0,
                engine_used="tesseract",
                word_count=word_count,
                language=self.config.languages[0] if self.config.languages else None,
            )

        except Exception as e:
            raise ExtractionError(
                f"Tesseract extraction failed: {e!s}", file_path=image_path
            ) from e


_ocr_service_instance: OcrService | None = None


def get_ocr_service(config: OcrConfig | None = None) -> OcrService:
    """Get or create OCR service singleton.

    Args:
        config: OCR configuration (uses default if None)

    Returns:
        OcrService instance
    """
    global _ocr_service_instance

    if config is None:
        config = OcrConfig()

    if _ocr_service_instance is None:
        _ocr_service_instance = OcrService(config)

    return _ocr_service_instance


def cleanup_ocr_service() -> None:
    """Cleanup OCR service singleton and free memory.

    Should be called when shutting down the application to free
    VLM model resources and clear GPU memory.
    """
    global _ocr_service_instance

    if _ocr_service_instance is not None:
        _ocr_service_instance.cleanup()
        _ocr_service_instance = None
