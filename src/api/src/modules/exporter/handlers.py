"""Export format handlers.

Implements handlers for each export format: JSON, CSV, Markdown, and ZIP.
Each handler is responsible for generating exports in its specific format.
"""

from abc import ABC, abstractmethod
import csv
import gzip
import json
from pathlib import Path
import shutil
from typing import TextIO
import zipfile

from .types import ExportMetadata, FileExportData


class BaseExportHandler(ABC):
    """Base class for export handlers.

    All format handlers inherit from this class and implement the
    generate() method for their specific format.
    """

    def __init__(self, output_path: str, compress: bool = True):
        """Initialize handler.

        Args:
            output_path: Path where export file should be written
            compress: Whether to compress the output
        """
        self.output_path = Path(output_path)
        self.compress = compress
        self.output_path.parent.mkdir(parents=True, exist_ok=True)

    @abstractmethod
    async def generate(self, files: list[FileExportData], metadata: ExportMetadata) -> str:
        """Generate export file.

        Args:
            files: List of file data to export
            metadata: Export metadata

        Returns:
            Path to generated export file
        """

    def _get_output_path_with_extension(self, base_extension: str) -> Path:
        """Get output path with appropriate extension.

        Args:
            base_extension: Base file extension (e.g., 'json', 'csv')

        Returns:
            Path with compression extension added if needed
        """
        if self.compress and base_extension != "zip":
            return self.output_path.with_suffix(f".{base_extension}.gz")
        return self.output_path.with_suffix(f".{base_extension}")


class JSONExportHandler(BaseExportHandler):
    """JSON export handler.

    Exports data as structured JSON with complete metadata and content.
    Supports optional compression with gzip.

    Example output structure:
    {
        "metadata": {
            "exported_at": "2024-01-01T00:00:00Z",
            "total_files": 150,
            ...
        },
        "files": [
            {
                "id": "...",
                "path": "...",
                "content": "...",
                ...
            }
        ]
    }
    """

    async def generate(self, files: list[FileExportData], metadata: ExportMetadata) -> str:
        """Generate JSON export.

        Args:
            files: List of file data to export
            metadata: Export metadata

        Returns:
            Path to generated JSON file

        Example:
            >>> handler = JSONExportHandler("/tmp/export.json")
            >>> path = await handler.generate(files, metadata)
        """
        output_path = self._get_output_path_with_extension("json")

        export_data = {
            "metadata": metadata.model_dump(mode="json"),
            "files": [f.model_dump(mode="json") for f in files],
        }

        if self.compress:
            with gzip.open(output_path, "wt", encoding="utf-8") as f:
                json.dump(export_data, f, indent=2, default=str)
        else:
            with open(output_path, "w", encoding="utf-8") as f:
                json.dump(export_data, f, indent=2, default=str)

        return str(output_path)


class CSVExportHandler(BaseExportHandler):
    """CSV export handler.

    Exports data as CSV for spreadsheet analysis.
    Flattens nested data structures for tabular format.

    Columns:
    - id, path, filename, extension, mime_type
    - size_bytes, hash_sha256, modified_at, indexed_at
    - text_content, word_count, char_count, language
    - tags (comma-separated)
    """

    async def generate(self, files: list[FileExportData], metadata: ExportMetadata) -> str:
        """Generate CSV export.

        Args:
            files: List of file data to export
            metadata: Export metadata

        Returns:
            Path to generated CSV file

        Example:
            >>> handler = CSVExportHandler("/tmp/export.csv", compress=True)
            >>> path = await handler.generate(files, metadata)
        """
        output_path = self._get_output_path_with_extension("csv")

        fieldnames = [
            "id",
            "path",
            "filename",
            "extension",
            "mime_type",
            "size_bytes",
            "hash_sha256",
            "modified_at",
            "indexed_at",
            "text_content",
            "word_count",
            "char_count",
            "language",
            "tags",
        ]

        if self.compress:
            with gzip.open(output_path, "wt", encoding="utf-8", newline="") as f:
                self._write_csv(f, fieldnames, files)
        else:
            with open(output_path, "w", encoding="utf-8", newline="") as f:
                self._write_csv(f, fieldnames, files)

        return str(output_path)

    def _write_csv(self, f: TextIO, fieldnames: list[str], files: list[FileExportData]):
        """Write CSV data to file handle.

        Args:
            f: File handle to write to
            fieldnames: CSV column names
            files: File data to write
        """
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()

        for file_data in files:
            row = {
                "id": str(file_data.id),
                "path": file_data.path,
                "filename": file_data.filename,
                "extension": file_data.extension,
                "mime_type": file_data.mime_type,
                "size_bytes": file_data.size_bytes,
                "hash_sha256": file_data.hash_sha256,
                "modified_at": file_data.modified_at.isoformat(),
                "indexed_at": file_data.indexed_at.isoformat(),
                "text_content": file_data.text_content or "",
                "word_count": file_data.word_count or 0,
                "char_count": file_data.char_count or 0,
                "language": file_data.language or "",
                "tags": ",".join(file_data.tags),
            }
            writer.writerow(row)


class MarkdownExportHandler(BaseExportHandler):
    """Markdown export handler.

    Exports data as human-readable Markdown files organized by folder structure.
    Creates an index file and individual markdown files for each document.

    Structure:
        export/
        ├── INDEX.md              # Table of contents
        ├── metadata.json         # Export metadata
        └── documents/
            ├── folder1/
            │   └── document1.md
            └── folder2/
                └── document2.md
    """

    async def generate(self, files: list[FileExportData], metadata: ExportMetadata) -> str:
        """Generate Markdown export.

        Args:
            files: List of file data to export
            metadata: Export metadata

        Returns:
            Path to export directory (or zip if compressed)

        Example:
            >>> handler = MarkdownExportHandler("/tmp/export", compress=False)
            >>> path = await handler.generate(files, metadata)
        """
        # Create export directory
        export_dir = self.output_path.with_suffix("")
        export_dir.mkdir(parents=True, exist_ok=True)

        docs_dir = export_dir / "documents"
        docs_dir.mkdir(exist_ok=True)

        # Write metadata
        metadata_path = export_dir / "metadata.json"
        with open(metadata_path, "w", encoding="utf-8") as f:
            json.dump(metadata.model_dump(mode="json"), f, indent=2, default=str)

        # Generate index
        await self._generate_index(export_dir, files, metadata)

        # Generate individual markdown files
        for file_data in files:
            await self._generate_file_markdown(docs_dir, file_data)

        # Compress if requested
        if self.compress:
            zip_path = self.output_path.with_suffix(".zip")
            with zipfile.ZipFile(zip_path, "w", zipfile.ZIP_DEFLATED) as zipf:
                for file_path in export_dir.rglob("*"):
                    if file_path.is_file():
                        arcname = file_path.relative_to(export_dir)
                        zipf.write(file_path, arcname)

            # Clean up directory
            shutil.rmtree(export_dir)
            return str(zip_path)

        return str(export_dir)

    async def _generate_index(
        self, export_dir: Path, files: list[FileExportData], metadata: ExportMetadata
    ):
        """Generate INDEX.md file.

        Args:
            export_dir: Export directory
            files: List of files
            metadata: Export metadata
        """
        index_path = export_dir / "INDEX.md"

        with open(index_path, "w", encoding="utf-8") as f:
            f.write("# Vault Export\n\n")
            f.write(f"**Exported:** {metadata.exported_at.isoformat()}\n\n")
            f.write(f"**Total Files:** {metadata.total_files}\n\n")
            f.write(f"**Total Size:** {self._format_bytes(metadata.total_size_bytes)}\n\n")

            if metadata.filters_applied:
                f.write("## Filters Applied\n\n")
                for key, value in metadata.filters_applied.items():
                    f.write(f"- **{key}:** {value}\n")
                f.write("\n")

            f.write("## Files\n\n")

            # Group by folder
            folders: dict = {}
            for file_data in files:
                path_parts = Path(file_data.path).parts
                folder = str(Path(*path_parts[:-1])) if len(path_parts) > 1 else "root"

                if folder not in folders:
                    folders[folder] = []
                folders[folder].append(file_data)

            # Write folder sections
            for folder, folder_files in sorted(folders.items()):
                f.write(f"### {folder}\n\n")
                for file_data in sorted(folder_files, key=lambda x: x.filename):
                    rel_path = f"documents/{self._sanitize_filename(file_data.path)}.md"
                    f.write(f"- [{file_data.filename}]({rel_path})")
                    if file_data.word_count:
                        f.write(f" ({file_data.word_count} words)")
                    f.write("\n")
                f.write("\n")

    async def _generate_file_markdown(self, docs_dir: Path, file_data: FileExportData):
        """Generate markdown file for a single document.

        Args:
            docs_dir: Documents directory
            file_data: File data to export
        """
        # Create sanitized filename
        safe_filename = self._sanitize_filename(file_data.path)
        file_path = docs_dir / f"{safe_filename}.md"
        file_path.parent.mkdir(parents=True, exist_ok=True)

        with open(file_path, "w", encoding="utf-8") as f:
            f.write(f"# {file_data.filename}\n\n")

            # Metadata section
            f.write("## Metadata\n\n")
            f.write(f"- **Path:** `{file_data.path}`\n")
            f.write(f"- **Type:** {file_data.mime_type}\n")
            f.write(f"- **Size:** {self._format_bytes(file_data.size_bytes)}\n")
            f.write(f"- **Modified:** {file_data.modified_at.isoformat()}\n")
            f.write(f"- **Indexed:** {file_data.indexed_at.isoformat()}\n")

            if file_data.language:
                f.write(f"- **Language:** {file_data.language}\n")

            if file_data.tags:
                f.write(f"- **Tags:** {', '.join(f'`{tag}`' for tag in file_data.tags)}\n")

            f.write("\n")

            # Content section
            if file_data.text_content:
                f.write("## Content\n\n")
                f.write(file_data.text_content)
                f.write("\n")

    def _sanitize_filename(self, path: str) -> str:
        """Sanitize filename for safe filesystem use.

        Args:
            path: Original file path

        Returns:
            Sanitized filename
        """
        # Replace path separators and special characters
        safe = path.replace("/", "_").replace("\\", "_")
        safe = safe.replace(":", "_").replace("*", "_")
        safe = safe.replace("?", "_").replace('"', "_")
        safe = safe.replace("<", "_").replace(">", "_")
        safe = safe.replace("|", "_")
        return safe[:200]  # Limit length

    def _format_bytes(self, bytes_val: int) -> str:
        """Format bytes as human-readable string.

        Args:
            bytes_val: Number of bytes

        Returns:
            Formatted string (e.g., "1.5 MB")
        """
        for unit in ["B", "KB", "MB", "GB", "TB"]:
            if bytes_val < 1024.0:
                return f"{bytes_val:.1f} {unit}"
            bytes_val /= 1024.0
        return f"{bytes_val:.1f} PB"


class ZIPExportHandler(BaseExportHandler):
    """ZIP export handler.

    Creates a ZIP archive containing:
    - Original files (if requested)
    - metadata.json with export metadata
    - index.json with file listings
    - extracted_text/ directory with text content
    """

    async def generate(
        self,
        files: list[FileExportData],
        metadata: ExportMetadata,
        include_original_files: bool = True,
    ) -> str:
        """Generate ZIP export.

        Args:
            files: List of file data to export
            metadata: Export metadata
            include_original_files: Whether to include original files

        Returns:
            Path to generated ZIP file

        Example:
            >>> handler = ZIPExportHandler("/tmp/export.zip")
            >>> path = await handler.generate(files, metadata, include_original_files=True)
        """
        output_path = self._get_output_path_with_extension("zip")

        with zipfile.ZipFile(output_path, "w", zipfile.ZIP_DEFLATED) as zipf:
            # Add metadata
            metadata_json = json.dumps(metadata.model_dump(mode="json"), indent=2, default=str)
            zipf.writestr("metadata.json", metadata_json)

            # Add file index
            index_data = {
                "total_files": len(files),
                "files": [
                    {
                        "id": str(f.id),
                        "path": f.path,
                        "filename": f.filename,
                        "size_bytes": f.size_bytes,
                        "mime_type": f.mime_type,
                    }
                    for f in files
                ],
            }
            zipf.writestr("index.json", json.dumps(index_data, indent=2))

            # Add extracted text for each file
            for file_data in files:
                if file_data.text_content:
                    text_path = f"extracted_text/{file_data.id}.txt"
                    zipf.writestr(text_path, file_data.text_content)

                # Add original file if requested and exists
                if include_original_files:
                    original_path = Path(file_data.path)
                    if original_path.exists() and original_path.is_file():
                        try:
                            arcname = f"files/{file_data.filename}"
                            zipf.write(original_path, arcname)
                        except Exception:
                            # Skip files that can't be read
                            pass

        return str(output_path)
