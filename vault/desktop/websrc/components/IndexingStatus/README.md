# IndexingStatus Component

Toast notifications for file indexing operations, implementing **Spec 6** from `ZEN_FILE_ARCHITECTURE_SOLUTION.md`.

## Purpose

Displays user-friendly status messages after file indexing operations, fixing **Issue #4** (no duplicate message) by showing clear feedback for:
- ✅ Successfully imported files
- ℹ️ Files already in library (duplicates)
- 🔄 Updated files with new content
- ❌ Errors during indexing

## Usage

```typescript
import { showIndexingToast } from '@/components/IndexingStatus';

// After indexing a file
const result = await VaultAPI.indexFile(filePath);

if (result.ok) {
  showIndexingToast({
    status: result.data.status,  // 'indexed', 'already_indexed', or 'updated'
    fileName: 'document.pdf',
    message: result.data.error || undefined
  });
}
```

## API

### `showIndexingToast(result: IndexingResult)`

Display a toast notification for a single file indexing result.

**Parameters:**
- `result.status`: One of:
  - `'indexed'` or `'imported'` - File successfully imported
  - `'already_indexed'` - File already exists in library
  - `'updated'` - File content changed and re-indexed
  - `'error'` - Indexing failed
- `result.fileName`: Name of the file (for display)
- `result.message`: Optional error message or additional context

**Example:**
```typescript
showIndexingToast({
  status: 'already_indexed',
  fileName: 'report.pdf'
});
// Shows: "ℹ️ report.pdf - This file is already in your library"
```

### `showBatchIndexingToast(totalFiles, successCount, duplicateCount, errorCount)`

Display a summary toast for batch indexing operations.

**Parameters:**
- `totalFiles`: Total number of files processed
- `successCount`: Number of successfully imported files
- `duplicateCount`: Number of duplicate files skipped
- `errorCount`: Number of files that failed

**Example:**
```typescript
showBatchIndexingToast(10, 7, 2, 1);
// Shows: "Batch import complete - 7 imported, 2 duplicates, 1 failed"
```

## Integration Points

Currently integrated into:
- `/components/Upload/UploadWithProgress.tsx` - File upload handlers
  - Single file selection
  - Multiple file selection
  - Drag and drop

## Toast Messages

| Status | Icon | Title | Message |
|--------|------|-------|---------|
| `indexed` / `imported` | ✅ | File name | "File imported successfully" |
| `already_indexed` | ℹ️ | File name | "This file is already in your library" |
| `updated` | 🔄 | File name | "File updated with new content" |
| `error` | ❌ | File name | Error message |

## Backend Integration

The component expects the backend to return `IndexFileResponseDto` with:
```rust
pub struct IndexFileResponseDto {
    pub document_id: String,
    pub chunks_created: usize,
    pub status: String,  // "indexed", "already_indexed", or "updated"
    pub error: Option<String>,
}
```

## Related Files

- `/types/api/files.ts` - `IndexFileResponse` type definition
- `/lib/api.ts` - `VaultAPI.indexFile()` wrapper
- `/stores/toastStore.ts` - Toast notification system
- `/utils/toast.ts` - Toast utility functions

## Architecture

Follows the **modular design philosophy**:
- Self-contained utility functions
- Clear contract with documented types
- No external dependencies beyond toast store
- Easy to test and maintain
