# File Browser Quick Start Guide

## 5-Minute Setup

### 1. The Component is Already Integrated!

The File Browser is automatically available in the "Files" tab. Just run the app:

```bash
cd vault/desktop
npm run dev
```

### 2. Navigate to Files

- Click "Files" in the sidebar, or
- Press `Cmd/Ctrl+2`

### 3. Choose Your View

Click the view mode buttons in the top-right:

- **Tree** (🌳) - Hierarchical folder structure
- **List** (📋) - Table with sortable columns
- **Grid** (▦) - Cards with thumbnails

## Common Tasks

### Search for Files

```
Type in the search box → Instant filtering
```

### Select Multiple Files

```
Cmd/Ctrl+Click → Add to selection
Shift+Click → Select range
Cmd/Ctrl+A → Select all
```

### Open Files

```
Double-click → Opens in default app
Right-click → Context menu with options
Enter key → Opens selected file
```

### Sort Files (List View)

```
Click column headers:
- Name
- Size
- Modified
- Type
```

### Navigate Folders (Tree View)

```
Click chevron → Expand/collapse folder
Arrow keys → Navigate up/down
Left/Right → Collapse/expand or move to parent/child
```

## Keyboard Shortcuts

| Action | Shortcut |
|--------|----------|
| Navigate up/down | ↑ / ↓ |
| Expand/collapse | → / ← |
| Open file | Enter |
| Select file | Space |
| Select all | Cmd/Ctrl+A |
| Clear selection | Escape |
| Multi-select | Cmd/Ctrl+Click |
| Range select | Shift+Click |

## Context Menu Actions

Right-click any file for:

- **Open** - Open in default application
- **Open in System Viewer** - Open with system app picker
- **Show in Folder** - Reveal in file explorer
- **Copy Path** - Copy full file path
- **Get Info** - View file metadata
- **Delete** - Remove from index (with confirmation)

## View Features

### Tree View
- Hierarchical file structure
- Collapsible folders
- Visual depth indicators
- Best for: Exploring project structure

### List View
- Sortable columns
- Checkboxes for selection
- Compact display
- Best for: Managing many files

### Grid View
- Card-based layout
- Image thumbnails
- File metadata
- Best for: Visual browsing

## Pro Tips

### Tip 1: Quick Navigation
Use the breadcrumb at the top to jump to any parent folder instantly.

### Tip 2: Bulk Actions
Select multiple files, then right-click on any selected file to apply actions to all.

### Tip 3: Search + Sort
Combine search with sorting to quickly find specific files.

### Tip 4: Keyboard First
Learn the keyboard shortcuts for blazing-fast navigation.

### Tip 5: View Modes
Switch views based on your task:
- Tree for structure
- List for sorting
- Grid for browsing

## Troubleshooting

### No files showing?
1. Go to Settings → Indexing
2. Add folders to index
3. Wait for indexing to complete
4. Return to Files tab

### Can't select files?
- Make sure you clicked inside the file list
- Try clicking directly on the file name
- Use keyboard shortcuts (Space key)

### Context menu not appearing?
- Right-click directly on a file row
- Make sure the file is visible (not scrolled off-screen)
- Try clicking again if it doesn't appear

### Search not working?
- Type at least 2 characters
- Wait 300ms for debounce
- Check that files are indexed

## Next Steps

1. **Add Folders**: Settings → Indexing → Add Folder
2. **Explore Views**: Try all three view modes
3. **Learn Shortcuts**: Practice keyboard navigation
4. **Read Full Docs**: See `README.md` for complete documentation

## Getting Help

- **Full Documentation**: `src/components/FileBrowser/README.md`
- **Implementation Details**: `FILE_BROWSER_IMPLEMENTATION.md`
- **Type Definitions**: `src/types/fileBrowser.ts`
- **Code Examples**: Check component source files

## Quick Reference

```typescript
// Use the store directly
import { useFileBrowserStore } from './stores/fileBrowserStore';

function MyComponent() {
  const { files, selectedFiles, viewMode } = useFileBrowserStore();
  // Your custom logic
}
```

That's it! You're ready to browse files like a pro! 🚀
