from io import BytesIO
import logging

from PIL import Image

logger = logging.getLogger(__name__)


class ThumbnailGenerator:
    def __init__(self, size: tuple[int, int] = (512, 512), quality: int = 85):
        self.size = size
        self.quality = quality

    async def generate(self, file_path: str, mime_type: str) -> bytes | None:
        try:
            if mime_type.startswith("image/"):
                return await self._generate_image_thumbnail(file_path)
            if mime_type.startswith("video/"):
                return await self._generate_video_thumbnail(file_path)
            if mime_type == "application/pdf":
                return await self._generate_pdf_thumbnail(file_path)
            return None
        except Exception as e:
            logger.error(f"Failed to generate thumbnail for {file_path}: {e}")
            return None

    async def _generate_image_thumbnail(self, file_path: str) -> bytes:
        img = Image.open(file_path)

        if hasattr(img, "getexif"):
            exif = img.getexif()
            if exif is not None:
                orientation = exif.get(0x0112)
                if orientation == 3:
                    img = img.rotate(180, expand=True)
                elif orientation == 6:
                    img = img.rotate(270, expand=True)
                elif orientation == 8:
                    img = img.rotate(90, expand=True)

        img.thumbnail(self.size, Image.Resampling.LANCZOS)

        buffer = BytesIO()
        img.convert("RGB").save(buffer, format="JPEG", quality=self.quality, optimize=True)
        buffer.seek(0)

        return buffer.getvalue()

    async def _generate_video_thumbnail(self, file_path: str) -> bytes | None:
        try:
            import cv2

            cap = cv2.VideoCapture(file_path)
            ret, frame = cap.read()
            cap.release()

            if not ret:
                logger.warning(f"Could not read first frame from video: {file_path}")
                return None

            frame_rgb = cv2.cvtColor(frame, cv2.COLOR_BGR2RGB)
            img = Image.fromarray(frame_rgb)

            img.thumbnail(self.size, Image.Resampling.LANCZOS)

            buffer = BytesIO()
            img.convert("RGB").save(buffer, format="JPEG", quality=self.quality, optimize=True)
            buffer.seek(0)

            return buffer.getvalue()
        except ImportError:
            logger.warning("opencv-python not installed, skipping video thumbnail")
            return None
        except Exception as e:
            logger.error(f"Failed to generate video thumbnail: {e}")
            return None

    async def _generate_pdf_thumbnail(self, file_path: str) -> bytes | None:
        try:
            from pdf2image import convert_from_path

            images = convert_from_path(file_path, first_page=1, last_page=1)

            if not images:
                logger.warning(f"Could not extract first page from PDF: {file_path}")
                return None

            img = images[0]
            img.thumbnail(self.size, Image.Resampling.LANCZOS)

            buffer = BytesIO()
            img.convert("RGB").save(buffer, format="JPEG", quality=self.quality, optimize=True)
            buffer.seek(0)

            return buffer.getvalue()
        except ImportError:
            logger.warning("pdf2image not installed, skipping PDF thumbnail")
            return None
        except Exception as e:
            logger.error(f"Failed to generate PDF thumbnail: {e}")
            return None
