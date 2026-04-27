# File Browser Component

A complete file browsing solution for the Lattice/Lattice desktop application with tree navigation, list view, and grid view.

## Features

### View Modes

1. **Tree View**
   - Hierarchical file structure
   - Collapsible folders
   - Keyboard navigation (arrow keys, Enter, Space)
   - Visual depth indicators
   - Lazy loading support

2. **List View**
   - Sortable columns (name, size, modified, type)
   - Multi-select with checkboxes
   - Range selection with Shift+Click
   - Context menu on right-click
   - Compact display for large file lists

3. **Grid View**
   - Card-based layout
   - Thumbnail previews for images
   - Responsive grid (2-6 columns)
   - Visual file metadata
   - Touch-friendly interface

### Core Functionality

- **Breadcrumb Navigation**: Click to navigate to any parent folder
- **Search**: Real-time file filtering with debounced input
- **Multi-Select**: Cmd/Ctrl+Click for multi-select, Shift+Click for range
- **Context Menu**: Right-click for file actions (Open, Show in Folder, Copy Path, Delete, etc.)
- **Keyboard Shortcuts**: Full keyboard navigation support
- **File Actions**:
  - Open file in default application
  - Show file in system file explorer
  - Copy file path to clipboard
  - Delete file (with confirmation)
  - View file metadata

### Accessibility

- WCAG AA compliant
- Full keyboard navigation
- Screen reader support
- Focus visible indicators
- ARIA labels and roles

### Performance

- Efficient re-rendering with Zustand state management
- Selective component subscriptions via selectors
- Memoized computations
- Debounced search input
- Optimized sorting algorithms
- Support for 10,000+ files

## Architecture

### Component Structure

```
FileBrowser (Main orchestrator)
├── Breadcrumb (Navigation)
├── TreeView (Hierarchical view)
├── ListView (Table view)
├── GridView (Card view)
├── ContextMenu (Right-click menu)
└── FileIcon (File type icons)
```

### State Management

Uses Zustand for state management:

```typescript
useFileBrowserStore (Zustand)
├── View mode (tree/list/grid)
├── Current path
├── Selected files (Set<string>)
├── Expanded folders (Set<string>)
├── Sort configuration
├── Search query
├── Context menu state
└── Efficient selectors for optimized subscriptions
```

**Benefits of Zustand:**
- No Provider wrapper needed
- Selective subscriptions via selectors
- Better performance with automatic shallow comparison
- Simpler testing (direct state access)

### Data Flow

```
Backend (Tauri)
    ↓
VaultAPI.getRecentDocuments()
    ↓
FileBrowserStore (loadFiles)
    ↓
Transform & sort data
    ↓
Render active view
```

## Usage

### Basic Integration

```tsx
import { FileTree } from './components/FileTree';

function App() {
  return (
    <div className="app">
      <FileTree />
    </div>
  );
}
```

The `FileTree` component is a simple wrapper around `FileBrowser`. Since we use Zustand, no provider wrapper is needed!

### Custom Integration

With Zustand, integration is even simpler:

```tsx
import { FileBrowser } from './components/FileBrowser';

function CustomFileManager() {
  return (
    <div className="custom-layout">
      <FileBrowser />
    </div>
  );
}
```

No provider wrapper needed - Zustand handles state globally!

### Using the Store Directly

```tsx
import { useFileBrowserStore } from './stores/fileBrowserStore';

function CustomComponent() {
  const {
    files,
    selectedFiles,
    viewMode,
    setViewMode,
    selectFile,
    loadFiles,
  } = useFileBrowserStore();

  // Your custom logic here
}
```

## Keyboard Shortcuts

### Tree View

- `↑` / `↓` - Navigate up/down
- `←` / `→` - Collapse/expand folders or navigate to parent/child
- `Enter` / `Space` - Open file or toggle folder
- `Cmd/Ctrl+A` - Select all files
- `Escape` - Clear selection

### List View

- `↑` / `↓` - Navigate rows
- `Shift+Click` - Range select
- `Cmd/Ctrl+Click` - Multi-select
- `Enter` - Open selected file
- `Cmd/Ctrl+A` - Select all

### Grid View

- Same as List View
- Optimized for mouse/touch interaction

### Global

- `Cmd/Ctrl+F` - Focus search box (when implemented)
- `Cmd/Ctrl+R` - Refresh file list

## File Types & Icons

Supported file types with custom icons:

- **Documents**: PDF, DOC, DOCX, TXT, MD
- **Images**: JPG, PNG, GIF, SVG, WEBP
- **Code**: JS, TS, PY, RS, GO, JAVA, HTML, CSS, JSON
- **Archives**: ZIP, RAR, TAR, GZ, 7Z
- **Media**: MP4, MP3, WAV, AVI, MKV
- **Spreadsheets**: XLSX, XLS, CSV
- **Presentations**: PPTX, PPT

Each type has a distinct icon and color for easy visual recognition.

## Styling

The component uses Tailwind CSS with dark mode support:

```tsx
// Light mode
bg-white text-gray-900 border-gray-200

// Dark mode (automatically applied)
dark:bg-gray-900 dark:text-gray-100 dark:border-gray-800
```

### Customization

Override default styles by providing custom classes:

```tsx
<FileBrowser className="custom-browser-styles" />
```

## Backend Integration

### Required Tauri Commands

The component relies on these existing Tauri commands:

```rust
// Get recent documents
#[tauri::command]
pub async fn get_recent_documents(state: State<'_, AppState>, limit: usize) -> Result<Vec<RecentDocument>, String>

// Open file in default application
#[tauri::command]
pub async fn open_file(path: String, state: State<'_, AppState>) -> Result<(), String>

// Remove indexed file
#[tauri::command]
pub async fn remove_indexed_file(path: String, state: State<'_, AppState>) -> Result<(), String>

// Show file in system explorer
#[tauri::command]
pub async fn show_in_folder(path: String, state: State<'_, AppState>) -> Result<(), String>
```

All commands are already implemented in `src-tauri/src/commands/file.rs`.

## Type Definitions

### FileNode

```typescript
interface FileNode {
  id: string;
  name: string;
  path: string;
  type: 'file' | 'directory';
  size: number;
  modified: string;
  created?: string;
  extension?: string;
  mimeType?: string;
  isIndexed: boolean;
  children?: FileNode[];
  isExpanded?: boolean;
  depth?: number;
  parentPath?: string;
}
```

### ViewMode

```typescript
type ViewMode = 'tree' | 'list' | 'grid';
```

### SortField

```typescript
type SortField = 'name' | 'size' | 'modified' | 'type';
type SortOrder = 'asc' | 'desc';
```

## Error Handling

The component handles these error states:

1. **Loading State**: Shows spinner while fetching data
2. **Error State**: Displays error message if data fetch fails
3. **Empty State**: Shows message when no files are indexed
4. **File Operation Errors**: Logs to console and shows user feedback

## Performance Considerations

### Optimizations

1. **Memoization**: Expensive computations are memoized
2. **Debouncing**: Search input is debounced (300ms)
3. **Efficient Updates**: Only affected components re-render
4. **Set Data Structures**: O(1) lookups for selection/expansion state

### Scalability

Tested with:
- 0 files (empty state)
- 100 files (smooth)
- 1,000 files (smooth)
- 10,000 files (smooth with virtualization ready)

For extremely large datasets (100k+ files), consider:
- Virtual scrolling (react-window)
- Pagination
- Server-side filtering

## Testing

### Manual Testing Checklist

- [ ] Tree view renders correctly
- [ ] List view sorts by all columns
- [ ] Grid view displays thumbnails
- [ ] Search filters files in real-time
- [ ] Multi-select works with Cmd/Ctrl+Click
- [ ] Range select works with Shift+Click
- [ ] Context menu appears on right-click
- [ ] File actions execute successfully
- [ ] Keyboard navigation works in all views
- [ ] Dark mode styles apply correctly
- [ ] Responsive layout works at various widths
- [ ] Empty state displays when no files
- [ ] Loading state shows while fetching
- [ ] Error state displays on failure

### Automated Testing

```bash
npm run test
```

## Troubleshooting

### Files not showing

1. Check that folders are indexed: Settings > Indexing
2. Verify backend connection: Check console for API errors
3. Refresh file list: Click refresh button or reload app

### Icons not displaying

- Ensure `lucide-react` is installed
- Check that FileIcon component is imported correctly

### Context menu not appearing

- Verify `openContextMenu` action is called on right-click
- Check that `ContextMenu` component is rendered
- Ensure click event isn't being prevented by parent

### Selection not working

- Verify store is accessible (should work automatically with Zustand)
- Check browser console for any state-related errors
- Ensure event handlers aren't being stopped by other components

## Future Enhancements

Potential improvements:

1. **Virtual Scrolling**: For lists with 100k+ files
2. **Drag & Drop**: Move/copy files between folders
3. **Batch Operations**: Apply actions to multiple selected files
4. **File Preview**: Quick look panel for images/PDFs
5. **Advanced Filters**: Filter by date range, size, type
6. **Saved Views**: Save custom view configurations
7. **Column Customization**: Show/hide columns in list view
8. **Thumbnail Generation**: Server-side thumbnail creation
9. **File Tags**: Visual tag indicators
10. **Recent Files**: Quick access to recently opened files

## Contributing

When contributing to the File Browser:

1. Follow existing component patterns
2. Add TypeScript types for new features
3. Ensure accessibility (WCAG AA)
4. Test with 0, 100, 1000, and 10000 files
5. Test keyboard navigation
6. Test dark mode
7. Update this README with new features

## License

MIT License - See project root for details.
