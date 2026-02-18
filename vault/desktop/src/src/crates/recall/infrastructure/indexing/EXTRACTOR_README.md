# Content Extractor - Implementation Documentation

## Overview

Production-ready PDF and DOCX content extraction for the Vault desktop application. This implementation replaces the Python bridge with pure Rust solutions for better performance, reliability, and self-containment.

## Implementation Details

### Chosen Crates

#### 1. **lopdf (v0.32)**
- **Why chosen**: MIT-licensed pure Rust implementation, replaces GPL-3.0 licensed pdf-extract
- **License**: MIT (compatible with commercial use)
- **Capabilities**:
  - Low-level PDF parsing with direct access to PDF structure
  - Accurate page counting via `doc.get_pages().len()`
  - Extracts text content from page streams
  - Handles PDF content streams and text operators (Tj, TJ, BT, ET)
- **Limitations**:
  - Cannot handle encrypted/password-protected PDFs
  - Complex multi-column layouts may have ordering issues
  - Scanned PDFs without OCR layer return empty text
  - Some complex PDF features (forms, annotations) not extracted
  - Requires manual text extraction from content streams (no high-level text API)

#### 2. **docx-rs (v0.4)**
- **Why chosen**: Pure Rust, handles modern DOCX format
- **Note**: We use manual ZIP parsing instead of docx-rs for better control
- **Capabilities**: Extracts text from paragraphs, preserves paragraph boundaries
- **Limitations**:
  - Tables extracted as continuous text (no structure preservation)
  - Headers/footers in word/header*.xml and word/footer*.xml not currently extracted
  - Complex formatting (track changes, comments) ignored
  - Embedded objects and images skipped

#### 3. **zip (v0.6)**
- Used to manually parse DOCX files (which are ZIP archives)
- Allows direct access to document.xml for text extraction

### Architecture

```
ContentExtractor
├── extract_from_file() - Main entry point (async)
├── extract_pdf() - PDF extraction (async wrapper)
│   └── extract_pdf_with_pages() - Actual PDF processing with page boundaries (blocking)
├── extract_docx() - DOCX extraction (async wrapper)
│   └── extract_docx_sync() - Actual DOCX processing (blocking)
└── Helper functions
    ├── extract_page_text() - Extract text from individual PDF page
    ├── extract_text_from_docx_xml() - XML parsing
    └── decode_xml_entities() - Entity decoding
```

### Key Design Decisions

1. **Blocking I/O in spawn_blocking**: PDF and DOCX extraction use blocking I/O, so we wrap them in `tokio::task::spawn_blocking` to avoid blocking the async runtime.

2. **Error Handling**: All errors are converted to `IndexingError::ContentExtraction` with descriptive messages. No panics on corrupt files.

3. **Manual XML Parsing for DOCX**: Instead of using a full XML parser, we do simple string processing to extract `<w:t>` tags. This is more efficient and handles most cases.

4. **Direct PDF Content Stream Parsing**: lopdf provides low-level access to PDF content streams. We manually parse text operators (BT/ET for text blocks, Tj/TJ for text strings) to extract text. This gives us precise control over page boundaries and text extraction.

5. **Accurate Page Counting**: lopdf provides accurate page counts via `doc.get_pages().len()`, eliminating the need for estimation.

## Error Handling

### PDF Errors
- **Encrypted PDFs**: Returns `ContentExtraction` error with message about encryption
- **Corrupt files**: Returns `ContentExtraction` error with parsing details
- **Empty/scanned PDFs**: Returns success with empty text

### DOCX Errors
- **Not a ZIP file**: Returns `ContentExtraction` error about invalid DOCX format
- **Missing document.xml**: Returns `ContentExtraction` error about missing required file
- **Corrupt XML**: Gracefully handles malformed XML, extracts what's possible

### General Errors
- **File not found**: Returns `FileRead` error with IO details
- **Permission denied**: Returns `FileRead` error with IO details
- **Unsupported format**: Returns `UnsupportedType` error

## Testing

### Unit Tests

Run the test suite:
```bash
cd vault/desktop/src-tauri
cargo test extractor
```

Included tests:
- `test_supported_extensions` - File extension detection
- `test_mime_detection` - MIME type mapping
- `test_xml_entity_decoding` - XML entity handling
- `test_docx_xml_extraction` - DOCX XML parsing
- `test_text_file_extraction` - Plain text extraction

### Integration Testing

#### Testing with Real PDF Files

```rust
use vault::indexing::extraction::ContentExtractor;
use std::path::Path;

#[tokio::test]
async fn test_real_pdf() {
    let extractor = ContentExtractor::new();

    // Test with a research paper PDF
    let path = Path::new("tests/fixtures/sample.pdf");
    let result = extractor.extract_from_file(path).await;

    assert!(result.is_ok());
    let content = result.unwrap();

    // Verify text was extracted
    assert!(!content.text.is_empty());
    assert!(content.metadata.page_count.is_some());
    println!("Extracted {} words from {} pages",
             content.metadata.word_count,
             content.metadata.page_count.unwrap());
}
```

#### Testing with Real DOCX Files

```rust
#[tokio::test]
async fn test_real_docx() {
    let extractor = ContentExtractor::new();

    // Test with a Word document
    let path = Path::new("tests/fixtures/sample.docx");
    let result = extractor.extract_from_file(path).await;

    assert!(result.is_ok());
    let content = result.unwrap();

    // Verify text was extracted with paragraph breaks
    assert!(!content.text.is_empty());
    assert!(content.text.contains("\n")); // Paragraph boundaries preserved
    println!("Extracted {} words", content.metadata.word_count);
}
```

#### Testing Error Cases

```rust
#[tokio::test]
async fn test_encrypted_pdf() {
    let extractor = ContentExtractor::new();
    let path = Path::new("tests/fixtures/encrypted.pdf");
    let result = extractor.extract_from_file(path).await;

    // Should return error, not panic
    assert!(result.is_err());
}

#[tokio::test]
async fn test_corrupt_docx() {
    let extractor = ContentExtractor::new();
    let path = Path::new("tests/fixtures/corrupt.docx");
    let result = extractor.extract_from_file(path).await;

    // Should return error, not panic
    assert!(result.is_err());
}
```

### Manual Testing

1. **Create test files directory**:
```bash
mkdir -p vault/desktop/src-tauri/tests/fixtures
```

2. **Add test files**:
   - `sample.pdf` - A simple PDF with text (e.g., research paper)
   - `sample.docx` - A Word document with multiple paragraphs
   - `encrypted.pdf` - A password-protected PDF
   - `scanned.pdf` - A scanned document (no text layer)

3. **Run extraction manually**:
```bash
cd vault/desktop
# Start the app and index a folder containing these files
# Check the logs for extraction results
```

## Performance Characteristics

### PDF Extraction
- **Time Complexity**: O(n) where n is file size
- **Memory**: Loads entire file into memory (may be 2-3x file size during processing)
- **Typical Speed**: ~1-5 seconds for 10-page document
- **Recommendation**: Set timeout to 30 seconds for large files

### DOCX Extraction
- **Time Complexity**: O(n) where n is XML size
- **Memory**: Loads entire document.xml into memory
- **Typical Speed**: <1 second for most documents
- **Recommendation**: Timeout of 10 seconds is sufficient

### Concurrency
Both extraction methods run in `spawn_blocking` thread pool, so they:
- Don't block the async runtime
- Can run in parallel (up to tokio's blocking thread pool size)
- Are safe to call from async contexts

## Known Limitations and Edge Cases

### PDF Limitations
1. **Encrypted/Protected PDFs**: Will fail with extraction error
2. **Scanned PDFs**: Without OCR layer, returns empty text (consider adding Tesseract integration for OCR)
3. **Complex Layouts**: Multi-column academic papers may have text order issues
4. **Forms and Fields**: Form field text not extracted
5. **Annotations/Comments**: Not extracted
6. **Images**: No OCR on embedded images

### DOCX Limitations
1. **Tables**: Text extracted but structure lost (all cells become one paragraph)
2. **Headers/Footers**: Currently not extracted (in word/header*.xml, word/footer*.xml)
3. **Track Changes**: Deleted/inserted text not handled specially
4. **Comments**: Not extracted
5. **Embedded Documents**: Not processed
6. **Drawing Objects**: Text in shapes/textboxes may be missed

### General Limitations
1. **File Size**: Very large files (>100MB) may cause memory issues
2. **Encoding**: Assumes UTF-8 compatible encoding
3. **Language Detection**: Not implemented (metadata.language always None)
4. **Metadata**: Limited metadata extraction (no author, creation date, etc.)

## Future Enhancements

### Priority 1 (High Impact)
- [ ] Add OCR support for scanned PDFs using Tesseract
- [ ] Extract DOCX headers and footers
- [ ] Better table structure preservation in DOCX
- [ ] Add file size limits and streaming for large files

### Priority 2 (Nice to Have)
- [ ] Extract PDF metadata (author, title, creation date)
- [ ] Extract DOCX comments and track changes
- [ ] Language detection using whatlang crate
- [ ] Better multi-column PDF handling
- [ ] Progress callbacks for large files

### Priority 3 (Advanced)
- [ ] Support for more formats (RTF, ODT, EPUB)
- [ ] Handle encrypted PDFs with password
- [ ] Extract images and run OCR on them
- [ ] Preserve more formatting info (bold, italic, headings)

## Migration Notes

### License Migration: pdf-extract (GPL-3.0) → lopdf (MIT)

**Date**: 2025-11-10
**Reason**: License compliance - GPL-3.0 is incompatible with proprietary/commercial use

#### Why the Change?

- **pdf-extract**: GPL-3.0 licensed (copyleft license requiring source disclosure)
- **lopdf**: MIT licensed (permissive license compatible with commercial use)
- The GPL-3.0 license would require the entire Recall application to be open-sourced under GPL-3.0, which is not compatible with commercial distribution

#### Implementation Differences

**pdf-extract approach**:
- High-level API: `extract_text(path)` returns text directly
- Automatic text ordering and layout detection
- Page count estimation via byte scanning

**lopdf approach**:
- Low-level API: Direct access to PDF objects and content streams
- Manual parsing of PDF content operators (BT, ET, Tj, TJ, T*, Td, TD, Tm)
- Accurate page counting via `doc.get_pages().len()`
- More control but requires more implementation code

#### Text Extraction Quality

**Similarities**:
- Both handle standard PDF text extraction
- Both fail on encrypted/password-protected PDFs
- Both return empty text for scanned PDFs without OCR layer
- Both have issues with complex multi-column layouts

**Differences**:
- **Page Boundaries**: lopdf provides accurate page-by-page extraction with real page numbers, pdf-extract estimated boundaries
- **Page Ranges**: lopdf tracks exact character positions per page in `page_ranges` vector
- **Text Operators**: lopdf recognizes more text positioning operators (T*, Td, TD, Tm) for better line break detection
- **Character Extraction**: lopdf handles both individual strings (Tj) and string arrays (TJ) more explicitly

#### Performance Characteristics

- **Speed**: lopdf is comparable to pdf-extract (both O(n) on file size)
- **Memory**: lopdf may use slightly more memory due to parsing entire PDF structure
- **Accuracy**: lopdf provides more accurate page counts and boundaries

#### Code Changes

**Removed**:
- `estimate_pdf_page_count()` function (line 210-213) - replaced by accurate counting
- Test `test_pdf_page_count_estimation` - no longer needed

**Added**:
- `extract_page_text(doc, page_num)` helper function for per-page extraction
- PDF content stream parsing logic (BT/ET blocks, Tj/TJ operators)
- Page range tracking in `extract_pdf_with_pages()`

**Modified**:
- `extract_pdf()` - wraps `extract_pdf_with_pages()` in `spawn_blocking` with timeout
- `extract_pdf_with_pages()` - returns accurate page boundaries
- Cargo.toml: `pdf-extract = "0.7"` → `lopdf = "0.32"`

### Changes from Python Bridge Version

**Before** (Python bridge):
- Required Python runtime
- External dependencies (PyPDF2, python-docx)
- JSON-RPC communication overhead
- Separate process management

**After** (Pure Rust):
- No external runtime needed
- Self-contained binary
- Direct function calls (faster)
- Simpler error handling

### API Compatibility

The public API remains the same:
```rust
pub async fn extract_from_file(&self, path: &Path) -> Result<ExtractedContent>
```

Return type `ExtractedContent` structure is unchanged:
```rust
pub struct ExtractedContent {
    pub text: String,
    pub mime_type: String,
    pub metadata: ContentMetadata,
    pub page_ranges: Vec<(usize, usize, usize)>, // (page_num, start_pos, end_pos)
}
```

### Breaking Changes

**None for end users**. The implementation is a drop-in replacement with improved accuracy:
- Page counts are now accurate instead of estimated
- Page boundaries in `page_ranges` are now precise
- Text extraction quality is equivalent or better

## Troubleshooting

### Issue: "PDF extraction failed: Invalid PDF structure"
**Cause**: Corrupt or invalid PDF file
**Solution**: Verify file is valid PDF, try opening in PDF reader

### Issue: "Failed to open DOCX as ZIP"
**Cause**: File is not a valid DOCX (old .doc format, or corrupt)
**Solution**: Verify file is DOCX format (not .doc), try opening in Word

### Issue: Empty text extracted from PDF
**Cause**: Scanned PDF without text layer, or encrypted PDF
**Solution**: Run OCR on the PDF first, or remove encryption

### Issue: DOCX text missing or incomplete
**Cause**: Text might be in headers/footers, tables, or text boxes
**Solution**: Currently unsupported, future enhancement needed

### Issue: Out of memory on large files
**Cause**: File size exceeds available memory
**Solution**: Add file size check, reject files >50MB

## Example Usage

### Basic Usage

```rust
use vault::indexing::extraction::ContentExtractor;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let extractor = ContentExtractor::new();

    // Extract from PDF
    let pdf_path = Path::new("document.pdf");
    let result = extractor.extract_from_file(pdf_path).await?;

    println!("Extracted {} words from PDF", result.metadata.word_count);
    println!("First 100 chars: {}", &result.text[..100.min(result.text.len())]);

    // Extract from DOCX
    let docx_path = Path::new("document.docx");
    let result = extractor.extract_from_file(docx_path).await?;

    println!("Extracted {} words from DOCX", result.metadata.word_count);

    Ok(())
}
```

### Error Handling

```rust
match extractor.extract_from_file(path).await {
    Ok(content) => {
        println!("Success: {} words", content.metadata.word_count);
    }
    Err(IndexingError::ContentExtraction { path, reason }) => {
        eprintln!("Failed to extract {}: {}", path, reason);
    }
    Err(IndexingError::UnsupportedType { path, mime_type }) => {
        eprintln!("Unsupported file type {} for {}", mime_type, path);
    }
    Err(e) => {
        eprintln!("Unexpected error: {}", e);
    }
}
```

### Batch Processing

```rust
use futures::future::join_all;

async fn process_documents(paths: Vec<PathBuf>) -> Vec<Result<ExtractedContent>> {
    let extractor = ContentExtractor::new();

    let futures = paths.iter().map(|path| {
        extractor.extract_from_file(path)
    });

    join_all(futures).await
}
```
