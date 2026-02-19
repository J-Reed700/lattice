# Documents API

## Overview

The Documents API provides endpoints for uploading and downloading documents in the Vault system. This API enforces strict validation on file types and sizes to ensure only supported document formats are processed.

## Endpoints

### Upload Document

**POST** `/api/v1/documents`

Upload a new document for indexing and storage.

#### Request

- **Content-Type**: `multipart/form-data`
- **Body Parameters**:
  - `file` (required): The document file to upload

#### Validation Rules

1. **File Type**: Only the following formats are accepted:
   - PDF (`.pdf`)
   - Plain Text (`.txt`)
   - Markdown (`.md`)
   - Microsoft Word (`.docx`)

2. **File Size**: Maximum 10MB per file

3. **Content**: File must not be empty

#### Response

**Status**: `201 Created`

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "filename": "report.pdf",
  "size_bytes": 1024000,
  "mime_type": "application/pdf",
  "upload_status": "completed",
  "created_at": "2024-01-15T10:30:00Z"
}
```

#### Error Responses

**400 Bad Request** - Validation Error
```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "Unsupported file type. Allowed: .pdf, .txt, .md, .docx",
    "details": {
      "allowed_extensions": [".pdf", ".txt", ".md", ".docx"]
    },
    "timestamp": "2024-01-15T10:30:00Z"
  }
}
```

**500 Internal Server Error** - Storage/Indexing Error
```json
{
  "error": {
    "code": "STORAGE_ERROR",
    "message": "Failed to index document: <error details>",
    "details": {},
    "timestamp": "2024-01-15T10:30:00Z"
  }
}
```

#### Example Usage

```bash
curl -X POST http://localhost:8000/api/v1/documents \
  -F "file=@/path/to/document.pdf"
```

```python
import requests

with open("document.pdf", "rb") as f:
    response = requests.post(
        "http://localhost:8000/api/v1/documents",
        files={"file": f}
    )

print(response.json())
```

---

### Download Document

**GET** `/api/v1/documents/{document_id}/download`

Download a previously uploaded document.

#### Parameters

- `document_id` (UUID, path parameter): The unique identifier of the document

#### Response

**Status**: `200 OK`

- **Content-Type**: Varies based on document type (e.g., `application/pdf`)
- **Headers**:
  - `Content-Disposition`: `attachment; filename="<original_filename>"`
- **Body**: Binary file content

#### Error Responses

**404 Not Found**
```json
{
  "error": {
    "code": "NOT_FOUND",
    "message": "Document <id> not found",
    "details": {},
    "timestamp": "2024-01-15T10:30:00Z"
  }
}
```

#### Example Usage

```bash
curl -o downloaded_file.pdf \
  http://localhost:8000/api/v1/documents/550e8400-e29b-41d4-a716-446655440000/download
```

```python
import requests

response = requests.get(
    "http://localhost:8000/api/v1/documents/550e8400-e29b-41d4-a716-446655440000/download"
)

with open("downloaded_file.pdf", "wb") as f:
    f.write(response.content)
```

---

## Processing Flow

When a document is uploaded:

1. **Validation**: File type, size, and content are validated
2. **Temporary Storage**: File is saved to a temporary location
3. **Indexing**: The indexing service processes the file:
   - Extracts text content
   - Generates embeddings for semantic search
   - Stores metadata in the database
4. **Response**: Returns document metadata
5. **Cleanup**: Temporary file is removed

## Supported MIME Types

- `application/pdf` - PDF documents
- `text/plain` - Plain text files
- `text/markdown` - Markdown files
- `application/vnd.openxmlformats-officedocument.wordprocessingml.document` - DOCX files

## Configuration

### File Size Limit

Default: 10MB (10,485,760 bytes)

To modify, update `MAX_FILE_SIZE` in `src/api/v1/documents.py`.

### Allowed File Types

To add or remove supported file types, update both:
- `ALLOWED_EXTENSIONS` set (file extensions)
- `ALLOWED_MIME_TYPES` set (MIME types)

## Error Handling

The API uses custom error classes from `src/api/errors.py`:

- `ValidationError`: Input validation failures (400)
- `NotFoundError`: Resource not found (404)
- `StorageError`: File storage/indexing failures (500)

All errors return a consistent JSON structure with:
- `code`: Machine-readable error code
- `message`: Human-readable error message
- `details`: Additional error context
- `timestamp`: ISO 8601 timestamp

## Integration with Existing Services

### Dependencies

- **Database Session**: `get_db()` - Provides async SQLAlchemy session
- **Indexing Service**: `get_indexing_service()` - Handles file processing and embedding generation
- **Storage Service**: Indirectly used by indexing service for cloud storage

### Related Endpoints

- **Files API** (`/api/v1/files`): General file management with more flexible upload options
- **Search API** (`/api/v1/search`): Search indexed document content

## Performance Considerations

- **Upload Size**: Files are fully read into memory for validation. Large files (approaching 10MB) may impact memory usage.
- **Indexing**: Processing happens synchronously. Document indexing time varies by:
  - File size
  - Content complexity
  - Embedding generation overhead

  Typical processing time: 1-5 seconds for most documents.

## Security

- **File Type Validation**: Enforces allowed extensions and MIME types
- **Size Limits**: Prevents resource exhaustion from large files
- **Path Traversal Protection**: Uses UUIDs for file identification
- **Temporary File Cleanup**: Ensures cleanup even on errors

## Future Enhancements

Potential improvements:
- Asynchronous background processing for large files
- Progress tracking for uploads
- Batch upload support
- File preview generation
- OCR support for scanned PDFs
