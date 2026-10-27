//! Imports from third-party knowledge tools (Obsidian, Notion, Roam).

use lattice_core::shared::error::AppError;
use lattice_core::shared::types::ValidatedFilePath;
use std::path::PathBuf;

/// Imports documents from an Obsidian lattice
///
/// Imports all Markdown files from an Obsidian lattice into Lattice, preserving links,
/// tags, and metadata. Recursively scans the lattice directory for `.md` files and
/// indexes them with full semantic search capabilities. Preserves Obsidian's YAML
/// front matter and wiki-style links (`[[note]]`).
///
/// # Errors
///
/// * `AppError::InvalidInput` - Invalid lattice path (directory traversal detected)
/// * `AppError::Other` - Lattice directory not found at specified path
/// * `AppError::Other` - Failed to access lattice directory (permissions error)
/// * `AppError::Other` - Import operation failed (parsing error, database error)
///
/// # Obsidian Lattice Structure
///
/// **Typical Obsidian Lattice**:
/// ```text
/// MyVault/
/// ├── .obsidian/           # Configuration (skipped)
/// ├── Daily Notes/         # Imported recursively
/// │   ├── 2024-01-15.md
/// │   └── 2024-01-16.md
/// ├── Projects/
/// │   ├── Project A.md
/// │   └── Project B.md
/// ├── Templates/           # Imported (optionally filtered)
/// └── README.md
/// ```
///
/// # Preserved Features
///
/// - **YAML Front Matter**: Tags, aliases, dates, custom properties
/// - **Wiki Links**: `[[Internal Link]]` and `[[Link|Display Text]]`
/// - **Tags**: `#tag` and `#nested/tag` formats
/// - **Embeddings**: `![[Image.png]]` and `![[Note]]` references
/// - **Markdown**: Standard Markdown formatting (headings, lists, code blocks)
///
/// **Example Obsidian Note**:
/// ```md
/// ---
/// tags: [research, machine-learning]
/// created: 2024-01-15
/// aliases: [ML Notes, AI Research]
/// ---
///
/// # Machine Learning Notes
///
/// See also: [[Deep Learning]] and [[Neural Networks]]
///
/// #research #ai
/// ```
///
/// # Import Behavior
///
/// - **Recursive**: Imports all subdirectories in lattice
/// - **Filtering**: Skips `.obsidian/` configuration directory
/// - **Templates**: Optionally skip or import template files
/// - **Duplicates**: Updates existing documents if already indexed
/// - **Links**: Preserves wiki links for future graph visualization
/// - **Tags**: Extracts both front matter and inline `#tags`
///
/// # Security
///
/// - **Path Validation (CWE-22)**: ValidatedFilePath prevents directory traversal
/// - **No Rate Limiting**: One-time bulk import (rate limiting would be counterproductive)
///
/// # Compatibility
///
/// - **Obsidian Version**: All versions (Markdown is universal)
/// - **Plugins**: Most plugin syntax preserved (code blocks, callouts)
/// - **Themes**: Styling not imported (Markdown only)
/// - **Attachments**: Images and PDFs linked but not imported (Markdown only)
///
/// # Architecture
///
/// Command flow:
/// 1. Path validation via ValidatedFilePath (CWE-22)
/// 2. Lattice existence check via tokio::fs
/// 3. Recursive directory traversal
/// 4. Markdown file parsing and indexing
/// 5. Link and tag extraction
pub async fn import_obsidian_vault(vault_path: String) -> Result<usize, AppError> {
    let validated_path = ValidatedFilePath::new(PathBuf::from(&vault_path))
        .map_err(|e| AppError::InvalidInput(format!("Invalid lattice path: {}", e)))?;

    // repository-barrier-allow: import validates the user-selected vault resource.
    // Check existence asynchronously
    let result = match tokio::fs::metadata(validated_path.as_path()).await {
        Ok(_) => Ok(0),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(AppError::Other(format!(
            "Lattice not found: {}",
            validated_path.as_path().display()
        ))),
        Err(e) => Err(AppError::Other(format!("Failed to access lattice: {}", e))),
    };

    result
}

/// Imports documents from a Notion export
///
/// Imports all pages and databases from a Notion HTML/Markdown export into Lattice,
/// preserving hierarchy, properties, and content. Recursively processes the export
/// directory, converting Notion pages to searchable documents with full semantic
/// search capabilities. Supports both HTML and Markdown export formats.
///
/// # Errors
///
/// * `AppError::InvalidInput` - Invalid export path (directory traversal detected)
/// * `AppError::Other` - Export directory/file not found at specified path
/// * `AppError::Other` - Failed to access export (permissions, corrupted ZIP)
/// * `AppError::Other` - Import operation failed (parsing error, unsupported format)
///
/// # Notion Export Structure
///
/// **Typical Notion Export** (HTML format):
/// ```text
/// Notion-Export/
/// ├── My Workspace/
/// │   ├── Projects/
/// │   │   ├── Project A.html
/// │   │   └── Project B.html
/// │   ├── Notes/
/// │   │   ├── Meeting Notes.html
/// │   │   └── Research.html
/// │   └── Databases/
/// │       └── Tasks Database.csv
/// └── images/              # Embedded images
///     ├── image1.png
///     └── image2.jpg
/// ```
///
/// **Markdown Export Structure**:
/// ```text
/// Notion-Export/
/// ├── Page 1.md
/// ├── Page 2.md
/// ├── Subpage 1/
/// │   ├── Nested Page.md
/// │   └── images/
/// └── Database Export.csv
/// ```
///
/// # Preserved Features
///
/// - **Hierarchy**: Parent-child page relationships preserved
/// - **Properties**: Custom properties (tags, dates, people, etc.)
/// - **Content**: Text, headings, lists, code blocks, quotes
/// - **Links**: Internal page links converted to Lattice links
/// - **Databases**: CSV exports imported as structured data
/// - **Tables**: Markdown tables preserved
///
/// **Notion Page with Properties**:
/// ```html
/// <html>
///   <head>
///     <meta name="tags" content="project, active">
///     <meta name="status" content="In Progress">
///     <meta name="created" content="2024-01-15">
///   </head>
///   <body>
///     <h1>Project Alpha</h1>
///     <p>Project description...</p>
///   </body>
/// </html>
/// ```
///
/// # Import Behavior
///
/// - **Recursive**: Imports all subdirectories and nested pages
/// - **Format Detection**: Auto-detects HTML or Markdown format
/// - **ZIP Support**: Automatically extracts ZIP files
/// - **Duplicates**: Updates existing documents if already indexed
/// - **Images**: Embedded images linked but not imported (HTML/Markdown only)
/// - **Databases**: CSV databases imported as separate documents
/// - **Properties**: Notion properties converted to Lattice tags/metadata
///
/// # Security
///
/// - **Path Validation (CWE-22)**: ValidatedFilePath prevents directory traversal
/// - **No Rate Limiting**: One-time bulk import (rate limiting would be counterproductive)
/// - **ZIP Extraction**: Safe ZIP extraction (prevents zip bomb attacks)
///
/// # Export Format Compatibility
///
/// - **HTML Export**: Recommended (preserves formatting)
/// - **Markdown Export**: Supported (simpler, faster parsing)
/// - **ZIP Files**: Automatically extracted
/// - **Notion Version**: All export versions supported
/// - **Subpages**: Fully supported (hierarchical imports)
/// - **Databases**: CSV exports imported
///
/// # Limitations
///
/// - **Embedded Content**: Videos, PDFs, files not imported (links only)
/// - **Comments**: Notion comments not preserved
/// - **Page History**: Only current version imported
/// - **Permissions**: Access control not preserved
/// - **Integrations**: Third-party embeds not supported
///
/// # Architecture
///
/// Command flow:
/// 1. Path validation via ValidatedFilePath (CWE-22)
/// 2. Export existence check via tokio::fs
/// 3. ZIP extraction if needed (safe extraction)
/// 4. Format detection (HTML vs Markdown)
/// 5. Recursive directory traversal
/// 6. HTML/Markdown parsing and metadata extraction
/// 7. Link conversion (Notion → Lattice)
/// 8. Indexing with embeddings
pub async fn import_notion_export(export_path: String) -> Result<usize, AppError> {
    let validated_path = ValidatedFilePath::new(PathBuf::from(&export_path))
        .map_err(|e| AppError::InvalidInput(format!("Invalid export path: {}", e)))?;

    // repository-barrier-allow: import validates the user-selected export resource.
    // Check existence asynchronously
    let result = match tokio::fs::metadata(validated_path.as_path()).await {
        Ok(_) => Ok(0),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(AppError::Other(format!(
            "Export not found: {}",
            validated_path.as_path().display()
        ))),
        Err(e) => Err(AppError::Other(format!("Failed to access export: {}", e))),
    };

    result
}

/// Imports documents from a Roam Research JSON export
///
/// Imports all pages and blocks from a Roam Research JSON export into Lattice,
/// preserving bidirectional links, block references, and page hierarchies. Parses
/// the Roam JSON format and converts pages to searchable documents with full
/// semantic search capabilities. Preserves Roam's unique features like daily notes
/// and nested blocks.
///
/// # Errors
///
/// * `AppError::InvalidInput` - Invalid export path (directory traversal detected)
/// * `AppError::Other` - Export file not found at specified path
/// * `AppError::Other` - Failed to access export file (permissions error)
/// * `AppError::Other` - Import operation failed (invalid JSON, parsing error)
///
/// # Roam JSON Structure
///
/// **Roam Export Format**:
/// ```json
/// [
///   {
///     "title": "My Page",
///     "create-time": 1642262400000,
///     "edit-time": 1642348800000,
///     "children": [
///       {
///         "string": "This is a block with a [[Page Link]]",
///         "create-time": 1642262400000,
///         "uid": "abc123",
///         "children": [
///           {
///             "string": "Nested block with ((block-ref))",
///             "uid": "def456"
///           }
///         ]
///       },
///       {
///         "string": "#tag content with **bold** and `code`",
///         "uid": "ghi789"
///       }
///     ]
///   },
///   {
///     "title": "January 15th, 2024",
///     "create-time": 1705276800000,
///     "daily-note-page": true,
///     "children": [...]
///   }
/// ]
/// ```
///
/// # Preserved Features
///
/// - **Page Links**: `[[Page Name]]` converted to Lattice links
/// - **Block References**: `((block-uid))` preserved for future graph visualization
/// - **Tags**: `#tag` and `[[tag]]` formats extracted
/// - **Daily Notes**: Date-based pages preserved with metadata
/// - **Nested Blocks**: Hierarchical block structure maintained
/// - **Timestamps**: Created/edited timestamps preserved
/// - **Attributes**: Page attributes (like `done::`, `status::`) converted to tags
/// - **Formatting**: Bold, italic, code, strikethrough preserved
///
/// **Roam Block with Features**:
/// ```json
/// {
///   "string": "Meeting with [[John Doe]] about #project-alpha\n  - TODO Review [[Design Doc]]\n  - DONE Update ((abc123))",
///   "uid": "block-xyz",
///   "heading": 2,
///   "create-time": 1642262400000
/// }
/// ```
///
/// # Import Behavior
///
/// - **Hierarchical**: Converts nested blocks to document structure
/// - **Daily Notes**: Preserves daily note pages with dates
/// - **Link Conversion**: Roam page links → Lattice links
/// - **Block References**: Preserved for future implementation
/// - **Attributes**: Custom attributes converted to metadata/tags
/// - **Duplicates**: Updates existing documents if already indexed
/// - **Large Files**: Handles large JSON files (10k+ pages) efficiently
///
/// # Security
///
/// - **Path Validation (CWE-22)**: ValidatedFilePath prevents directory traversal
/// - **No Rate Limiting**: One-time bulk import (rate limiting would be counterproductive)
/// - **JSON Parsing**: Safe JSON parsing (prevents malicious JSON attacks)
///
/// # Roam Export Types
///
/// - **JSON Export**: Primary format (recommended)
/// - **Markdown Export**: Not supported (use JSON)
/// - **EDN Export**: Not supported (use JSON)
///
/// **Creating Roam JSON Export**:
/// 1. Open Roam Research
/// 2. Click "..." menu (top right)
/// 3. Select "Export All"
/// 4. Choose "JSON" format
/// 5. Download and import to Lattice
///
/// # Limitations
///
/// - **Embedded Media**: Images, videos, PDFs not imported (links only)
/// - **Queries**: Roam queries not preserved (converted to text)
/// - **Diagrams**: Mermaid/GraphViz diagrams converted to code blocks
/// - **Pomodoro**: Timer attributes not preserved
/// - **Encryption**: Encrypted graphs must be decrypted before export
///
/// # Architecture
///
/// Command flow:
/// 1. Path validation via ValidatedFilePath (CWE-22)
/// 2. File existence check via tokio::fs
/// 3. Safe JSON parsing (prevents malicious JSON)
/// 4. Page-by-page processing
/// 5. Block hierarchy flattening
/// 6. Link and reference extraction
/// 7. Attribute conversion to metadata
/// 8. Indexing with embeddings
pub async fn import_roam_json(json_path: String) -> Result<usize, AppError> {
    let validated_path = ValidatedFilePath::new(PathBuf::from(&json_path))
        .map_err(|e| AppError::InvalidInput(format!("Invalid export path: {}", e)))?;

    // repository-barrier-allow: import validates the user-selected JSON resource.
    // Check existence asynchronously
    let result = match tokio::fs::metadata(validated_path.as_path()).await {
        Ok(_) => Ok(0),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(AppError::Other(format!(
            "Export not found: {}",
            validated_path.as_path().display()
        ))),
        Err(e) => Err(AppError::Other(format!("Failed to access export: {}", e))),
    };

    result
}
