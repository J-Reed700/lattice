import asyncio
from dataclasses import dataclass
import logging
from typing import Any

from PIL import Image

from ..types import ExtractionError, OcrConfig, OcrEngine, VlmModel

logger = logging.getLogger(__name__)


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
        metadata: Additional extraction metadata

    Example:
        >>> result = OcrResult(
        ...     text="Hello World",
        ...     confidence=0.95,
        ...     has_text=True,
        ...     engine_used="qwen2-vl",
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
    metadata: dict[str, Any] = None

    def __post_init__(self):
        if self.metadata is None:
            self.metadata = {}


class ModelConfig:
    """Model-specific configuration mapping."""

    MODEL_REGISTRY = {
        VlmModel.FLORENCE2_BASE: {
            "model_id": "microsoft/Florence-2-base",
            "processor_type": "florence2",
            "supports_quantization": False,
            "default_prompt": "<OCR>",
            "memory_footprint_mb": 500,
        },
        VlmModel.FLORENCE2_LARGE: {
            "model_id": "microsoft/Florence-2-large",
            "processor_type": "florence2",
            "supports_quantization": False,
            "default_prompt": "<OCR>",
            "memory_footprint_mb": 1500,
        },
        VlmModel.QWEN2_VL_2B: {
            "model_id": "Qwen/Qwen2-VL-2B-Instruct",
            "processor_type": "qwen2-vl",
            "supports_quantization": True,
            "default_prompt": "Extract all text from this image.",
            "memory_footprint_mb": 2000,
        },
        VlmModel.QWEN2_VL_7B: {
            "model_id": "Qwen/Qwen2-VL-7B-Instruct",
            "processor_type": "qwen2-vl",
            "supports_quantization": True,
            "default_prompt": "Extract all text from this image.",
            "memory_footprint_mb": 7000,
        },
    }

    @classmethod
    def get_model_config(cls, vlm_model: VlmModel) -> dict[str, Any]:
        """Get configuration for specified VLM model."""
        return cls.MODEL_REGISTRY.get(vlm_model, cls.MODEL_REGISTRY[VlmModel.QWEN2_VL_2B])


class OcrService:
    """OCR service supporting multiple VLM models and Tesseract.

    Provides text extraction from images using Vision Language Models
    (Florence-2, Qwen2.5-VL) or traditional OCR engines with automatic fallback.

    Attributes:
        config: OCR configuration
        _vlm_model: Cached VLM model instance
        _vlm_processor: Cached VLM processor instance
        _model_config: Current model configuration

    Example:
        >>> config = OcrConfig(enabled=True, engine=OcrEngine.VLM, vlm_model=VlmModel.QWEN2_VL_2B)
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
        self._model_config = ModelConfig.get_model_config(config.vlm_model)
        self._current_vlm_model = config.vlm_model

    def __del__(self):
        """Ensure cleanup on object destruction."""
        self.cleanup()

    def cleanup(self) -> None:
        """Cleanup VLM model and free memory.

        Deletes cached model and processor instances and clears CUDA cache
        if available. Should be called when shutting down the service.
        """
        import gc

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
        """Extract text using Vision Language Model.

        Args:
            img: PIL Image object
            image_path: Path to image file (for error reporting)

        Returns:
            OcrResult with extracted text

        Raises:
            ExtractionError: If VLM extraction fails
        """
        try:
            if self._vlm_model is None or self._current_vlm_model != self.config.vlm_model:
                await self._load_vlm_model()

            loop = asyncio.get_event_loop()
            result = await loop.run_in_executor(None, self._run_vlm_inference, img)

            return result

        except ImportError as e:
            raise ExtractionError(
                f"VLM dependencies not installed: {e!s}. "
                "Install with: pip install transformers torch qwen-vl-utils",
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

        self._model_config = ModelConfig.get_model_config(self.config.vlm_model)
        model_id = self._model_config["model_id"]
        processor_type = self._model_config["processor_type"]

        logger.info(f"Loading VLM model: {model_id}")
        logger.info(f"Quantization: {self.config.use_quantization}")

        loop = asyncio.get_event_loop()

        if processor_type == "qwen2-vl":
            try:
                from transformers import AutoProcessor, Qwen2VLForConditionalGeneration

                self._vlm_processor = await loop.run_in_executor(
                    None, lambda: AutoProcessor.from_pretrained(model_id, trust_remote_code=True)
                )

                if self.config.use_quantization and self._model_config["supports_quantization"]:
                    logger.info("Loading Qwen2-VL with 4-bit quantization...")
                    self._vlm_model = await loop.run_in_executor(
                        None, lambda: self._load_qwen_quantized(model_id)
                    )
                else:
                    logger.info("Loading Qwen2-VL without quantization...")
                    self._vlm_model = await loop.run_in_executor(
                        None,
                        lambda: Qwen2VLForConditionalGeneration.from_pretrained(
                            model_id, torch_dtype="auto", device_map="auto", trust_remote_code=True
                        ).eval(),
                    )
            except ImportError:
                logger.warning("Qwen2VL not available, falling back to AutoModel")
                self._vlm_processor = await loop.run_in_executor(
                    None, lambda: AutoProcessor.from_pretrained(model_id, trust_remote_code=True)
                )
                self._vlm_model = await loop.run_in_executor(
                    None,
                    lambda: AutoModelForCausalLM.from_pretrained(
                        model_id, torch_dtype="auto", device_map="auto", trust_remote_code=True
                    ).eval(),
                )

        else:
            self._vlm_processor = await loop.run_in_executor(
                None, AutoProcessor.from_pretrained, model_id, False
            )

            self._vlm_model = await loop.run_in_executor(
                None,
                lambda: AutoModelForCausalLM.from_pretrained(
                    model_id, trust_remote_code=False
                ).eval(),
            )

        self._current_vlm_model = self.config.vlm_model
        logger.info(f"VLM model loaded successfully: {model_id}")

    def _load_qwen_quantized(self, model_id: str):
        """Load Qwen2-VL with 4-bit quantization.

        Args:
            model_id: HuggingFace model identifier

        Returns:
            Quantized model instance
        """
        try:
            import torch
            from transformers import BitsAndBytesConfig, Qwen2VLForConditionalGeneration

            quantization_config = BitsAndBytesConfig(
                load_in_4bit=True,
                bnb_4bit_compute_dtype=torch.float16,
                bnb_4bit_use_double_quant=True,
                bnb_4bit_quant_type="nf4",
            )

            model = Qwen2VLForConditionalGeneration.from_pretrained(
                model_id,
                quantization_config=quantization_config,
                device_map="auto",
                trust_remote_code=True,
            ).eval()

            return model

        except ImportError as e:
            logger.warning(f"Quantization not available: {e}. Loading without quantization.")
            from transformers import Qwen2VLForConditionalGeneration

            return Qwen2VLForConditionalGeneration.from_pretrained(
                model_id, torch_dtype="auto", device_map="auto", trust_remote_code=True
            ).eval()

    def _run_vlm_inference(self, img: Image.Image) -> OcrResult:
        """Run VLM inference on image.

        Args:
            img: PIL Image object

        Returns:
            OcrResult with extracted text
        """

        processor_type = self._model_config["processor_type"]

        if processor_type == "qwen2-vl":
            return self._run_qwen_inference(img)
        return self._run_florence_inference(img)

    def _run_qwen_inference(self, img: Image.Image) -> OcrResult:
        """Run Qwen2-VL inference on image.

        Args:
            img: PIL Image object

        Returns:
            OcrResult with extracted text
        """
        import gc

        import torch

        prompt = "Extract all text from this image. Provide the text exactly as it appears, maintaining the original formatting and layout. If there is no text, respond with 'No text found'."

        messages = [
            {
                "role": "user",
                "content": [
                    {"type": "image", "image": img},
                    {"type": "text", "text": prompt},
                ],
            }
        ]

        try:
            text_input = self._vlm_processor.apply_chat_template(
                messages, tokenize=False, add_generation_prompt=True
            )

            inputs = self._vlm_processor(
                text=[text_input], images=[img], padding=True, return_tensors="pt"
            )

            device = next(self._vlm_model.parameters()).device
            inputs = inputs.to(device)

            with torch.no_grad():
                generated_ids = self._vlm_model.generate(
                    **inputs,
                    max_new_tokens=self.config.max_new_tokens,
                    temperature=self.config.temperature if self.config.temperature > 0 else None,
                    do_sample=self.config.temperature > 0,
                )

            generated_ids_trimmed = [
                out_ids[len(in_ids) :]
                for in_ids, out_ids in zip(inputs.input_ids, generated_ids, strict=False)
            ]

            output_text = self._vlm_processor.batch_decode(
                generated_ids_trimmed, skip_special_tokens=True, clean_up_tokenization_spaces=False
            )[0]

            text = output_text.strip()

            if text.lower() in ["no text found", "no text", ""]:
                text = ""
                confidence = 0.0
            else:
                confidence = 0.95

            word_count = len(text.split()) if text else 0

            return OcrResult(
                text=text,
                confidence=confidence,
                has_text=word_count > 0,
                engine_used="qwen2-vl",
                word_count=word_count,
                language=None,
                metadata={
                    "model": self._model_config["model_id"],
                    "quantized": self.config.use_quantization,
                },
            )

        finally:
            if "inputs" in locals():
                del inputs
            if "generated_ids" in locals():
                del generated_ids

            self._inference_count += 1
            if self._inference_count % self._cleanup_interval == 0:
                gc.collect()
                if torch.cuda.is_available():
                    torch.cuda.empty_cache()

    def _run_florence_inference(self, img: Image.Image) -> OcrResult:
        """Run Florence-2 inference on image.

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
                engine_used="florence2",
                word_count=word_count,
                language=None,
                metadata={"model": self._model_config["model_id"]},
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
