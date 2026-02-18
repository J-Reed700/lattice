# Document Viewer Component

A comprehensive, production-ready document viewer for the Recall/Vault desktop application.

## Overview

The DocumentViewer provides in-app viewing of various file formats from search results, eliminating the critical UX gap where users could search but not view documents.

## Features

### Core Functionality
- **Multi-format Support**: PDF, text, code, images, and markdown
- **Navigation**: Browse between search results with keyboard shortcuts
- **Metadata Display**: File information, size, dates, and search relevance scores
- **Actions**: Open in external app, show in folder
- **Responsive**: Works at all screen sizes with dark mode support

### Format-Specific Viewers

#### PDF Viewer
- Page navigation (previous/next)
- Zoom in/out (50% to 300%)
- Rotation (90-degree increments)
- Text selection and copy
- Page counter display

#### Text Viewer
- Monospace font for readability
- Preserved whitespace and line breaks
- Scrollable content

#### Code Viewer
- Syntax highlighting for 50+ languages
- Line numbers
- Copy to clipboard
- Theme matching (light/dark)
- Language detection from file extension

#### Image Viewer
- Zoom controls (10% to 500%)
- Rotation (90-degree increments)
- Fit to screen toggle
- Reset view
- Smooth transitions

#### Markdown Viewer
- GitHub-flavored markdown support
- Tables, task lists, strikethrough
- Typography styles
- Dark mode support
- External link handling

## Usage

### Basic Usage

```tsx
import { DocumentViewer } from '@/components/DocumentViewer';

function MyComponent() {
  const [viewingDocument, setViewingDocument] = useState<SearchResult | null>(null);

  return (
    <>
      {viewingDocument && (
        <DocumentViewer
          result={viewingDocument}
          searchResults={allResults}
          onClose={() => setViewingDocument(null)}
          onNavigate={(result) => setViewingDocument(result)}
        />
      )}
    </>
  );
}
```

### Integration with Search

The DocumentViewer is integrated into SearchView. Users can:
1. Search for documents
2. Click a result to open the viewer
3. Navigate between results with arrow keys or UI buttons
4. Close with Escape key or close button

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Escape` | Close viewer |
| `Left Arrow` | Previous document |
| `Right Arrow` | Next document |
| `I` | Toggle info sidebar |

## Architecture

```
DocumentViewer/
├── DocumentViewer.tsx       # Main container
├── ViewerHeader.tsx         # Header with actions
├── ViewerContent.tsx        # Format router
├── ViewerSidebar.tsx        # Metadata display
├── index.ts                 # Exports
└── viewers/                 # Format-specific viewers
    ├── PdfViewer.tsx
    ├── TextViewer.tsx
    ├── CodeViewer.tsx
    ├── ImageViewer.tsx
    └── MarkdownViewer.tsx
```

## Component Props

### DocumentViewer

```typescript
interface DocumentViewerProps {
  /** The search result to display */
  result: SearchResult;
  /** All search results for navigation */
  searchResults?: SearchResult[];
  /** Callback when viewer is closed */
  onClose: () => void;
  /** Callback when navigating to a different document */
  onNavigate?: (result: SearchResult) => void;
}
```

### FileData

```typescript
interface FileData {
  path: string;
  fileName: string;
  fileType: string;
  content?: string;
  mimeType: string;
  sizeBytes: number;
  modifiedAt: string;
}
```

## Backend Requirements

The DocumentViewer requires these Tauri commands:

### `read_file_content`
Reads text file content as a string.

```rust
#[tauri::command]
pub async fn read_file_content(path: String) -> Result<String, String>
```

### `get_file_metadata`
Gets file size and modified date.

```rust
#[tauri::command]
pub async fn get_file_metadata(path: String) -> Result<FileMetadata, String>
```

### `open_file`
Opens file in default external application.

```rust
#[tauri::command]
pub async fn open_file(path: String) -> Result<(), String>
```

### `show_in_folder`
Shows file in system file explorer.

```rust
#[tauri::command]
pub async fn show_in_folder(path: String) -> Result<(), String>
```

## Supported File Types

### Text Files
`.txt`, `.log`

### Code Files
`.js`, `.ts`, `.tsx`, `.jsx`, `.py`, `.rs`, `.go`, `.java`, `.c`, `.cpp`, `.h`, `.css`, `.html`, `.json`, `.yaml`, `.yml`, `.toml`, `.xml`, `.sh`, `.bash`, `.sql`, `.graphql`

### Documents
`.md`, `.markdown`, `.pdf`

### Images
`.png`, `.jpg`, `.jpeg`, `.gif`, `.svg`, `.webp`

## Accessibility

- **WCAG AA Compliant**: Proper contrast ratios, semantic HTML
- **Keyboard Navigation**: All functions accessible via keyboard
- **Screen Reader Support**: ARIA labels and landmarks
- **Focus Management**: Proper focus trapping in modal

## Performance

- **Lazy Loading**: Format-specific viewers loaded on demand
- **Code Splitting**: Reduces initial bundle size
- **Optimized Rendering**: React.memo and useCallback for performance
- **60fps Animations**: GPU-accelerated transforms

## Error Handling

The viewer gracefully handles:
- File not found
- Read permission errors
- Unsupported file formats
- Large file sizes
- Network issues (for remote files)

## Styling

- Tailwind CSS for styling
- Dark mode support via `dark:` classes
- Responsive breakpoints
- Custom animations with `transition-*` utilities

## Dependencies

```json
{
  "react-pdf": "^7.5.1",
  "pdfjs-dist": "^4.0.0",
  "react-syntax-highlighter": "^15.5.0",
  "react-markdown": "^10.1.0",
  "remark-gfm": "^4.0.1",
  "rehype-raw": "^7.0.0",
  "lucide-react": "^0.553.0"
}
```

## Testing

To test the viewer:

1. **Run the app**: `npm run dev`
2. **Perform a search**: Enter query in search bar
3. **Click a result**: Viewer should open
4. **Test navigation**: Use arrow keys or buttons
5. **Test formats**: Try different file types
6. **Test actions**: Open external, show in folder
7. **Test keyboard**: Escape, arrow keys, I key

## Future Enhancements

### High Priority
- [ ] Search within document
- [ ] Document annotations
- [ ] Print support
- [ ] Full-screen mode

### Medium Priority
- [ ] Table of contents for long documents
- [ ] Related documents panel
- [ ] Tag editing in sidebar
- [ ] Version history

### Low Priority
- [ ] Document comparison
- [ ] Export annotations
- [ ] Custom themes
- [ ] Plugin system for custom viewers

## Troubleshooting

### PDF not loading
- Ensure `pdfjs-dist` worker is properly configured
- Check console for CORS errors
- Verify file path is correct

### Syntax highlighting not working
- Verify `react-syntax-highlighter` is installed
- Check language mapping in CodeViewer
- Ensure code content is loaded

### Images not displaying
- Verify `convertFileSrc` from Tauri is used
- Check file permissions
- Ensure image format is supported

### Performance issues
- Enable code splitting
- Limit search results shown
- Use virtualization for large lists

## Contributing

When adding new format support:

1. Create viewer in `viewers/` directory
2. Add format detection in `ViewerContent.tsx`
3. Update `README.md` with supported types
4. Add tests for new format
5. Update documentation

## License

Part of the Recall/Vault project. See main LICENSE file.
