"""
Link Extractor - Parser

Parser for extracting wikilinks from markdown documents.
"""

import re
from typing import Any


class LinkParser:
    """Parser for extracting wikilinks from markdown documents."""

    WIKILINK_PATTERN = r"\[\[([^\]|#]+)(?:#([^\]|]+))?(?:\|([^\]]+))?\]\]"

    def parse_document(self, content: str, source_path: str) -> list[dict[str, Any]]:
        """Extract all wikilinks from document content."""
        links = []

        for match in re.finditer(self.WIKILINK_PATTERN, content):
            target = match.group(1).strip()
            header = match.group(2).strip() if match.group(2) else None
            display = match.group(3).strip() if match.group(3) else None

            line_number = content[: match.start()].count("\n") + 1

            start = max(0, match.start() - 50)
            end = min(len(content), match.end() + 50)
            context = content[start:end].replace("\n", " ")

            links.append(
                {
                    "target": target,
                    "display_text": display,
                    "header": header,
                    "line_number": line_number,
                    "context": context,
                }
            )

        return links

    def extract_title(self, content: str) -> str | None:
        """Extract document title from content."""
        if not content:
            return None

        lines = content.split("\n")

        if content.startswith("---"):
            for line in lines[1:]:
                if line.strip() == "---":
                    break
                if line.strip().startswith("title:"):
                    title = line.split(":", 1)[1].strip()
                    return title.strip('"').strip("'")

        for line in lines:
            line = line.strip()
            if line.startswith("# "):
                return line[2:].strip()

        if lines and lines[0].strip():
            first_line = lines[0].strip()
            if len(first_line) < 100 and not first_line.startswith("["):
                return first_line

        return None
