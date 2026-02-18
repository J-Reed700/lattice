#!/usr/bin/env python3
"""
JSON-RPC server for content extraction.

This server provides content extraction capabilities for the Vault indexing service.
It communicates via JSON-RPC over stdin/stdout.
"""

import json
import logging
from pathlib import Path
import sys
from typing import Any

# Add the modules to the path
sys.path.insert(0, str(Path(__file__).parent))

from modules.content_extractor import extract_content_from_file
from modules.content_extractor.types import ExtractionResult

# Configure logging to stderr so it doesn't interfere with JSON-RPC
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
    stream=sys.stderr,
)

logger = logging.getLogger(__name__)


class JsonRpcError:
    """JSON-RPC error codes."""

    PARSE_ERROR = -32700
    INVALID_REQUEST = -32600
    METHOD_NOT_FOUND = -32601
    INVALID_PARAMS = -32602
    INTERNAL_ERROR = -32603


class ContentExtractionServer:
    """JSON-RPC server for content extraction."""

    def __init__(self):
        self.methods = {
            "extract_pdf": self.extract_pdf,
            "extract_docx": self.extract_docx,
            "extract_text": self.extract_text,
            "ping": self.ping,
        }

    def extract_pdf(self, params: dict[str, Any]) -> dict[str, Any]:
        """Extract content from PDF file."""
        file_path = params.get("file_path")
        if not file_path:
            raise ValueError("Missing 'file_path' parameter")

        result = self._extract_content(file_path, "pdf")
        return self._format_result(result)

    def extract_docx(self, params: dict[str, Any]) -> dict[str, Any]:
        """Extract content from DOCX file."""
        file_path = params.get("file_path")
        if not file_path:
            raise ValueError("Missing 'file_path' parameter")

        result = self._extract_content(file_path, "docx")
        return self._format_result(result)

    def extract_text(self, params: dict[str, Any]) -> dict[str, Any]:
        """Extract content from text file."""
        file_path = params.get("file_path")
        if not file_path:
            raise ValueError("Missing 'file_path' parameter")

        result = self._extract_content(file_path, "text")
        return self._format_result(result)

    def ping(self, params: dict[str, Any]) -> dict[str, Any]:
        """Health check endpoint."""
        return {"status": "ok", "message": "pong"}

    def _extract_content(self, file_path: str, file_type: str) -> ExtractionResult:
        """Extract content using the content extractor module."""
        try:
            path = Path(file_path)
            if not path.exists():
                raise FileNotFoundError(f"File not found: {file_path}")

            result = extract_content_from_file(path)
            return result
        except Exception as e:
            logger.error(f"Error extracting content from {file_path}: {e}")
            raise

    def _format_result(self, result: ExtractionResult) -> dict[str, Any]:
        """Format extraction result for JSON-RPC response."""
        return {
            "text": result.text,
            "mime_type": result.mime_type,
            "page_count": getattr(result, "page_count", None),
            "language": getattr(result, "language", None),
        }

    def handle_request(self, request_data: dict[str, Any]) -> dict[str, Any]:
        """Handle a JSON-RPC request."""
        try:
            # Validate JSON-RPC version
            if request_data.get("jsonrpc") != "2.0":
                return self._error_response(
                    None, JsonRpcError.INVALID_REQUEST, "Invalid JSON-RPC version"
                )

            request_id = request_data.get("id")
            method_name = request_data.get("method")
            params = request_data.get("params", {})

            # Check if method exists
            if method_name not in self.methods:
                return self._error_response(
                    request_id, JsonRpcError.METHOD_NOT_FOUND, f"Method not found: {method_name}"
                )

            # Call the method
            method = self.methods[method_name]
            result = method(params)

            return self._success_response(request_id, result)

        except ValueError as e:
            return self._error_response(request_id, JsonRpcError.INVALID_PARAMS, str(e))
        except Exception as e:
            logger.error(f"Error handling request: {e}", exc_info=True)
            return self._error_response(
                request_id, JsonRpcError.INTERNAL_ERROR, f"Internal error: {e!s}"
            )

    def _success_response(self, request_id: Any, result: Any) -> dict[str, Any]:
        """Create a successful JSON-RPC response."""
        return {"jsonrpc": "2.0", "id": request_id, "result": result}

    def _error_response(self, request_id: Any, code: int, message: str) -> dict[str, Any]:
        """Create an error JSON-RPC response."""
        return {"jsonrpc": "2.0", "id": request_id, "error": {"code": code, "message": message}}

    def run(self):
        """Run the JSON-RPC server (stdin/stdout)."""
        logger.info("Content extraction RPC server started")

        try:
            for line in sys.stdin:
                line = line.strip()
                if not line:
                    continue

                try:
                    request_data = json.loads(line)
                    response = self.handle_request(request_data)
                except json.JSONDecodeError as e:
                    response = self._error_response(
                        None, JsonRpcError.PARSE_ERROR, f"Parse error: {e!s}"
                    )

                # Send response
                response_json = json.dumps(response)
                sys.stdout.write(response_json + "\n")
                sys.stdout.flush()

        except KeyboardInterrupt:
            logger.info("Server interrupted by user")
        except Exception as e:
            logger.error(f"Server error: {e}", exc_info=True)
            sys.exit(1)


def main():
    """Main entry point."""
    server = ContentExtractionServer()
    server.run()


if __name__ == "__main__":
    main()
