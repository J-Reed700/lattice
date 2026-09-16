# Vault User Manual

This comprehensive guide covers everything you need to know to use Vault effectively.

## Table of Contents

- [Overview](#overview)
- [Understanding the Interface](#understanding-the-interface)
- [File Management](#file-management)
- [Search Functionality](#search-functionality)
- [Watch Folders and Auto-Indexing](#watch-folders-and-auto-indexing)
- [Settings and Configuration](#settings-and-configuration)
- [Export and Backup](#export-and-backup)
- [Storage Management](#storage-management)
- [Tips and Best Practices](#tips-and-best-practices)

## Overview

Vault is your personal knowledge recall system that makes all your files searchable using AI-powered semantic search. Unlike traditional file search that only matches keywords, Vault understands the meaning and context of your queries.

### Key Concepts

**Indexing**
The process of analyzing your files and creating a searchable database. Vault extracts text, generates AI embeddings, and stores metadata about each file.

**Embeddings**
Numerical representations of text that capture meaning. Vault uses these to find files that are conceptually similar to your search query, even if they don't contain the exact words.

**Semantic Search**
Search by meaning rather than exact keywords. For example, searching for "vacation photos" will find images even if they're labeled "holiday pictures."

**Watch Folders**
Folders that Vault monitors for changes. When you add, modify, or delete files, Vault automatically updates the index.

## Understanding the Interface

Vault's interface is designed to be clean and intuitive. Here's a tour of the main components:

**Screenshot placeholder:** *Full Vault window with labeled interface elements*

### Main Window Components

**1. Search Bar (Top Center)**
- Primary way to search your indexed files
- Auto-complete suggestions appear as you type
- Press Enter to search or Escape to clear

**2. Search Mode Selector (Top Left)**
- Switch between Semantic, Keyword, and Hybrid search
- Each mode has different strengths (see [Search Functionality](#search-functionality))

**3. Settings Button (Top Right)**
- Gear icon opens the settings panel
- Keyboard shortcut: `Ctrl+,` (Windows/Linux) or `Cmd+,` (macOS)

**4. Results Area (Center)**
- Displays search results in a scrollable list
- Shows file preview, name, location, and relevance score
- Click any result to see details or open the file

**5. Filter Panel (Right Sidebar)**
- Filter results by file type, date range, size, and tags
- Collapsible to maximize results space

**6. Status Bar (Bottom)**
- Shows indexing progress
- Displays number of indexed files
- Shows storage usage

### Keyboard Shortcuts

Master these shortcuts for faster workflow:

| Shortcut | Action |
|----------|--------|
| `Ctrl/Cmd + K` | Focus search bar |
| `Ctrl/Cmd + ,` | Open settings |
| `Ctrl/Cmd + F` | Toggle filter panel |
| `Ctrl/Cmd + N` | Index new folder |
| `Ctrl/Cmd + R` | Refresh index |
| `Ctrl/Cmd + E` | Export results |
| `Ctrl/Cmd + B` | Create backup |
| `Escape` | Clear search or close dialog |
| `Enter` | Open selected file |
| `Ctrl/Cmd + Enter` | Open in system explorer |
| `↑/↓` | Navigate results |
| `Tab` | Cycle through result actions |

**Screenshot placeholder:** *Keyboard shortcuts reference card*

## File Management

Vault helps you organize, find, and manage your files efficiently.

### Adding Files to the Index

There are several ways to add files to Vault:

**Method 1: Add a Folder**
1. Open Settings > Indexing
2. Click "+ Add Folder"
3. Select the folder to index
4. Choose indexing options:
   - **Recursive:** Include all subfolders
   - **Auto-index:** Automatically update when files change
5. Click "Start Indexing"

**Method 2: Drag and Drop**
1. Drag a folder from your file explorer
2. Drop it onto the Vault window
3. Confirm indexing options
4. Indexing starts automatically

**Method 3: Context Menu (Windows/macOS)**
1. Right-click a folder in your file explorer
2. Select "Index with Vault" from the context menu
3. Vault opens and starts indexing

**Screenshot placeholder:** *Add Folder dialog showing folder selection and options*

### Supported File Types

Vault can index and search these file formats:

**Documents**
- Text files (`.txt`, `.md`, `.log`)
- Microsoft Office (`.docx`, `.xlsx`, `.pptx`)
- PDF documents (`.pdf`)
- Rich text (`.rtf`)
- OpenDocument (`.odt`, `.ods`, `.odp`)

**Images**
- Common formats (`.jpg`, `.png`, `.gif`, `.bmp`, `.webp`)
- RAW formats (`.cr2`, `.nef`, `.arw`)
- Metadata and EXIF data extraction

**Code and Development**
- Source code (`.js`, `.py`, `.java`, `.cpp`, `.rs`, etc.)
- Configuration files (`.json`, `.yaml`, `.toml`, `.xml`)
- Markup languages (`.html`, `.css`, `.svg`)

**Other Formats**
- Archives (`.zip`, `.tar`, `.gz`) - file list only
- Email files (`.eml`, `.msg`)
- Ebooks (`.epub`, `.mobi`)

**Screenshot placeholder:** *File types settings showing checkboxes for supported formats*

### Viewing and Opening Files

**View File Details**
1. Click a search result
2. Details panel shows:
   - Full file path
   - File size and type
   - Creation and modification dates
   - Preview of content
   - Related files (if any)

**Open File**
- Click "Open" button to open in default application
- Or double-click the result
- Or press Enter when result is selected

**Show in Folder**
- Click "Show in Folder" to reveal in file explorer
- Or press `Ctrl/Cmd + Enter`

**Quick Preview**
- Hover over a result for 1 second to see quick preview
- Works for text files and images
- Press `Space` to toggle full preview

**Screenshot placeholder:** *File details panel showing metadata and preview*

### File Operations

**Copy File Path**
1. Right-click a search result
2. Select "Copy Path"
3. Path is copied to clipboard

**Copy Content**
1. Right-click a search result
2. Select "Copy Content"
3. File's text content is copied to clipboard

**Tag Files**
1. Select one or more results
2. Click "Add Tag" button
3. Create new tags or select existing ones
4. Tags appear on files and are searchable

**Remove from Index**
1. Right-click a search result
2. Select "Remove from Index"
3. Confirm removal
4. File is no longer searchable (but not deleted from disk)

**Screenshot placeholder:** *Right-click context menu showing file operations*

## Search Functionality

Vault offers three powerful search modes to help you find exactly what you need.

### Search Modes

**Semantic Search (Recommended)**

Searches by meaning and context, not just exact words.

**Best for:**
- Natural language queries
- Finding conceptually similar content
- When you don't know exact keywords

**Examples:**
```
"document about marketing strategy"
"photos from summer vacation"
"meeting notes with project deadlines"
"code for user authentication"
```

**How it works:**
1. Vault converts your query into an AI embedding
2. Compares it to embeddings of all indexed files
3. Returns files with similar meaning

**Screenshot placeholder:** *Semantic search results showing relevant files without exact keyword matches*

**Keyword Search**

Searches for exact word matches with boolean operators.

**Best for:**
- Finding specific terms or phrases
- Precise technical searches
- When you know exact keywords

**Examples:**
```
"annual report"                    (exact phrase in quotes)
marketing AND strategy             (both words must appear)
vacation OR holiday                (either word)
project NOT archived               (exclude a word)
filename:budget                    (search in filename only)
type:pdf                          (filter by file type)
```

**Boolean Operators:**
- `AND` - Both terms must appear
- `OR` - Either term can appear
- `NOT` - Exclude term
- `"quotes"` - Exact phrase match
- `*` - Wildcard character

**Screenshot placeholder:** *Keyword search with boolean operators in search bar*

**Hybrid Search**

Combines semantic and keyword search for best results.

**Best for:**
- Complex queries
- When you want comprehensive results
- Balancing precision and recall

**How it works:**
1. Runs both semantic and keyword searches
2. Combines results using smart ranking
3. Deduplicates and sorts by relevance

**Configuration:**
- Settings > Search > Hybrid Search Weight
- Adjust slider to favor semantic (left) or keyword (right) results
- Default: 50/50 balance

**Screenshot placeholder:** *Hybrid search settings with weight adjustment slider*

### Search Filters

Narrow your results using powerful filters:

**File Type Filter**
- Select one or more file types (Documents, Images, Code, etc.)
- Only results matching selected types appear
- Click "All Types" to reset

**Date Range Filter**
- **Modified:** When file was last changed
- **Created:** When file was created
- **Indexed:** When file was added to Vault
- Presets: Today, This Week, This Month, This Year, Custom
- Custom range: Pick start and end dates

**Size Filter**
- Tiny: < 10 KB
- Small: 10 KB - 100 KB
- Medium: 100 KB - 1 MB
- Large: 1 MB - 10 MB
- Huge: > 10 MB
- Custom: Specify exact range

**Location Filter**
- Filter by parent folder
- Useful when you know general location
- Shows folder tree of indexed locations

**Tag Filter**
- Filter by tags you've created
- Multiple tags = show files with ANY selected tag
- "All tags" mode = show files with ALL selected tags

**Screenshot placeholder:** *Filter sidebar showing all filter options*

### Search Tips and Tricks

**Use Natural Language**
```
Good: "photos of birthday party last summer"
Better than: "birthday party summer photos"
```

**Be Specific When Needed**
```
Too broad: "document"
Better: "project proposal for Smith account"
```

**Combine Filters**
```
Query: "marketing"
+ Type: PDF
+ Date: Last 3 months
= Recent marketing PDFs
```

**Use Tags for Organization**
```
Tag files as: Important, Work, Personal, Archive
Then filter by tags to quickly find categories
```

**Search File Names vs Content**
```
filename:report      (searches only filenames)
content:analysis     (searches only file content)
```

**Save Common Searches**
1. Perform a search with filters
2. Click "Save Search" button
3. Give it a name
4. Access from "Saved Searches" dropdown

**Screenshot placeholder:** *Saved searches dropdown showing frequently used searches*

## Watch Folders and Auto-Indexing

Watch folders automatically keep your index up-to-date as files change.

### Setting Up Watch Folders

**Add a Watch Folder**
1. Settings > Indexing > Watch Folders
2. Click "+ Add Watch Folder"
3. Select folder to monitor
4. Configure options:
   - **Recursive:** Monitor subfolders
   - **Auto-index new files:** Automatically index new files
   - **Update on changes:** Re-index modified files
   - **Remove deleted files:** Remove deleted files from index

**Screenshot placeholder:** *Watch folder configuration dialog*

### Watch Folder Behavior

**When files are added:**
- Automatically indexed within 10 seconds
- Notification appears when indexing completes
- File immediately becomes searchable

**When files are modified:**
- Detected within 10 seconds
- Re-indexed to update content
- Search index updated with new content

**When files are deleted:**
- Automatically removed from index
- No longer appears in search results
- Database space reclaimed

**When files are moved:**
- Treated as delete + add
- Path updated in database
- Maintains tags and metadata

### Managing Watch Folders

**View All Watch Folders**
- Settings > Indexing > Watch Folders
- Shows list of all monitored folders
- Display includes:
  - Folder path
  - Number of indexed files
  - Last scan time
  - Status (active/paused)

**Pause/Resume Watching**
- Click pause icon next to folder
- Pausing stops file monitoring
- Resume to restart monitoring

**Remove Watch Folder**
- Click trash icon next to folder
- Confirms before removing
- Options:
  - **Keep indexed files:** Files stay in index
  - **Remove indexed files:** Files removed from index

**Rescan Folder**
- Click refresh icon next to folder
- Forces full re-scan
- Useful after manual file changes

**Screenshot placeholder:** *Watch folders list showing multiple folders with status indicators*

### Watch Folder Performance

**Performance Impact:**
- Minimal CPU usage during idle
- Brief CPU spike when changes detected
- Memory usage: ~10 MB per 1,000 watched files

**Optimization Tips:**
- Exclude temporary folders (Downloads, Temp)
- Use exclude patterns for node_modules, .git, etc.
- Limit to folders that actually change
- Consider manual indexing for static archives

### Exclude Patterns

Prevent certain files from being indexed:

**Common Patterns:**
```
*.tmp                    (temporary files)
*.log                    (log files)
node_modules             (Node.js dependencies)
.git                     (Git repository data)
.DS_Store                (macOS system files)
Thumbs.db                (Windows thumbnails)
~*                       (Microsoft Office temp files)
```

**Pattern Syntax:**
- `*` matches any characters
- `?` matches single character
- Use folder names without slashes
- Case-insensitive on Windows/macOS

**Configure Exclude Patterns:**
1. Settings > Indexing > Exclude Patterns
2. Click "+ Add Pattern"
3. Enter pattern
4. Click "Test Pattern" to see what it matches
5. Save

**Screenshot placeholder:** *Exclude patterns settings with pattern tester*

## Settings and Configuration

Customize Vault to work exactly how you want.

### General Settings

**Appearance**
- **Theme:** Light, Dark, or System
- **Font size:** 12-18pt (default: 14pt)
- **Animations:** Enable/disable UI animations
- **Compact mode:** Reduce spacing for more results

**Startup**
- **Launch on system startup:** Start Vault when computer boots
- **Start minimized:** Launch to system tray
- **Check for updates:** Automatically check for new versions

**Language**
- Select interface language
- Currently supported: English (more coming soon)

**Screenshot placeholder:** *General settings panel showing appearance and startup options*

### Indexing Settings

**Performance**
- **Batch size:** Files processed simultaneously (16-64, default: 32)
- **Thread count:** CPU threads for indexing (1-16, auto-detect recommended)
- **Index priority:** Background, Normal, or High
- **Max file size:** Skip files larger than specified size (default: 100 MB)

**Content Extraction**
- **Extract text from images (OCR):** Enable/disable OCR (requires additional setup)
- **Process metadata:** Extract EXIF, ID3, and other metadata
- **Generate thumbnails:** Create image previews (uses storage)
- **Deep content analysis:** More thorough but slower indexing

**File Types**
- Check/uncheck file types to index
- Add custom file extensions
- Configure type-specific settings

**Screenshot placeholder:** *Indexing settings showing performance sliders and file type checkboxes*

### Search Settings

**Relevance**
- **Similarity threshold:** Minimum score for results (0-100, default: 60)
- **Max results:** Maximum results to return (10-1000, default: 50)
- **Results per page:** Results shown before pagination (10-100, default: 25)

**Hybrid Search**
- **Enable hybrid search:** Combine semantic and keyword
- **Semantic weight:** Importance of semantic results (0-100%, default: 50%)
- **Keyword weight:** Importance of keyword results (0-100%, default: 50%)

**Reranking**
- **Enable reranking:** Re-sort results for better relevance (slower)
- **Reranking model:** Choose reranking algorithm

**Cache**
- **Enable search cache:** Remember recent searches for speed
- **Cache size:** Number of searches to remember (10-1000, default: 100)
- **Cache TTL:** How long to keep cached results (1-60 min, default: 15)

**Screenshot placeholder:** *Search settings with relevance sliders*

### Privacy and Security

**Data Privacy**
- **Location:** All data stored locally (no cloud)
- **Analytics:** Vault collects zero telemetry or usage data
- **Updates:** Update checks only (no data sent)

**File Access**
- **Read-only mode:** Vault never modifies your original files
- **Sandboxed paths:** Vault only accesses configured folders
- **Permission verification:** Confirms access before indexing

**Database Security**
- **Encryption:** Enable database encryption (requires password)
- **Password protection:** Lock Vault with password
- **Auto-lock:** Lock after inactivity period

**Screenshot placeholder:** *Security settings showing encryption and password options*

### Advanced Settings

**Database**
- **Database location:** Path to vault.db file
- **Database size:** Current size of index
- **Optimize database:** Reclaim space and improve performance
- **Verify database:** Check for corruption
- **Reset database:** Delete all indexed data (cannot be undone!)

**Models**
- **Model location:** Path to AI models
- **Embedding model:** Choose text embedding model
- **Re-download models:** Force re-download if corrupted
- **Model cache:** Clear model cache to free space

**Logging**
- **Enable logging:** Write application logs
- **Log level:** Error, Warning, Info, Debug, Trace
- **Log location:** Path to log files
- **Max log size:** Rotate logs after size limit

**Experimental**
- **Beta features:** Enable experimental functionality
- **Developer mode:** Show debug information
- **Verbose logging:** Detailed logging for troubleshooting

**Screenshot placeholder:** *Advanced settings showing database and model options*

## Export and Backup

Protect your data and share your knowledge.

### Creating Backups

**Automatic Backups**
1. Settings > Backup > Auto Backup
2. Enable automatic backups
3. Configure schedule:
   - Daily, Weekly, or Monthly
   - Specific time of day
   - Retention: How many backups to keep
4. Choose backup location
5. Enable compression to save space

**Manual Backup**
1. Click "Backup" button in toolbar
2. Or Settings > Backup > Create Backup Now
3. Choose backup location
4. Select what to backup:
   - **Database only:** Index data (fast, small)
   - **Database + models:** Include AI models
   - **Full backup:** Database + models + config
5. Click "Create Backup"

**Screenshot placeholder:** *Backup creation dialog with options*

**Backup Contents:**
- `vault.db` - Your complete file index
- `config.json` - All settings and preferences
- `models/` - AI model files (optional)
- `backups.json` - Backup metadata

### Restoring from Backup

**Restore Process:**
1. Settings > Backup > Restore Backup
2. Click "Browse" and select backup file
3. Review backup information:
   - Creation date
   - Number of indexed files
   - Database size
   - Vault version
4. Click "Restore"
5. Vault restarts with restored data

**Warning:** Restoring replaces all current data. Create a backup first!

**Screenshot placeholder:** *Restore backup dialog showing backup information*

### Exporting Data

Export your index in various formats for use in other applications.

**Export Formats:**

**Markdown Export**
- One .md file per indexed document
- Preserves text content and metadata
- Useful for notes apps (Obsidian, Notion)
- Maintains folder structure

**JSON Export**
- Complete database export
- Includes all metadata and embeddings
- Useful for programmatic access
- Can be re-imported later

**CSV Export**
- Spreadsheet-friendly format
- File list with metadata
- Good for inventory or analysis
- No content, just metadata

**HTML Export**
- Static web page for each file
- Browsable in any web browser
- Includes search functionality
- Self-contained archive

**Screenshot placeholder:** *Export dialog showing format options*

**Export Process:**
1. Settings > Export > Export Data
2. Choose export format
3. Select what to export:
   - All indexed files
   - Current search results
   - Specific folders
   - Tagged files
4. Choose destination folder
5. Click "Export"
6. Progress bar shows export status

### Importing Data

**Import from Other Apps:**

**Obsidian Vault**
- Imports markdown files and attachments
- Preserves folder structure
- Converts wiki links to tags

**Notion Export**
- Imports exported Notion pages
- Handles nested pages
- Converts databases to tags

**Roam Research**
- Imports JSON export
- Converts block references
- Creates tags from page links

**Screenshot placeholder:** *Import dialog showing source application options*

**Import Process:**
1. Settings > Import > Import Data
2. Select source application
3. Browse to export file/folder
4. Configure import options:
   - Preserve tags
   - Convert links
   - Create folders
5. Click "Import"
6. Review imported items

## Storage Management

Monitor and optimize your storage usage.

### Storage Dashboard

View storage statistics:

**Database Size**
- Total database size
- Documents metadata: X MB
- Text chunks: X MB
- Embeddings: X MB (largest component)
- Images and thumbnails: X MB

**Indexed Content**
- Total files indexed: X,XXX
- Total text content: X GB
- Average file size: X KB
- Largest file: filename (X MB)

**Breakdown by Type**
- Documents: XX%
- Images: XX%
- Code: XX%
- Other: XX%

**Screenshot placeholder:** *Storage dashboard showing size breakdown with charts*

### Optimizing Storage

**Reduce Database Size:**

1. **Remove Unused Files**
   - Review indexed folders
   - Remove folders you no longer need
   - Files are removed from index

2. **Exclude Large Files**
   - Settings > Indexing > Max File Size
   - Set limit (e.g., 50 MB)
   - Large files won't be indexed

3. **Disable Thumbnails**
   - Settings > Indexing > Generate Thumbnails
   - Uncheck to save space
   - Reduces image storage by 30-50%

4. **Optimize Database**
   - Settings > Advanced > Optimize Database
   - Reclaims unused space
   - Rebuilds indexes for efficiency
   - Run monthly for best performance

5. **Clean Cache**
   - Settings > Advanced > Clear Cache
   - Removes search cache
   - Removes temporary files
   - Safe to do anytime

**Screenshot placeholder:** *Optimize database dialog showing space to be reclaimed*

### Storage Limits

**Recommended Limits:**
- **Small library:** < 10,000 files, < 1 GB database
- **Medium library:** 10,000 - 100,000 files, 1-10 GB database
- **Large library:** 100,000 - 1,000,000 files, 10-100 GB database

**Performance Impact:**
- Up to 100,000 files: No noticeable slowdown
- 100,000 - 500,000 files: Slightly longer searches (100-500ms)
- 500,000+ files: Consider multiple Vault instances

**Disk Space Requirements:**
- Average: 50-100 KB per document (including embeddings)
- Text documents: 10-30 KB
- Images with thumbnails: 100-200 KB
- Large PDFs: 200-500 KB

## Tips and Best Practices

### Organizing Your Files

**Use Descriptive Filenames**
```
Good: 2024-Q3-Marketing-Report.pdf
Bad:  report.pdf
```
Even with semantic search, good filenames help.

**Create a Folder Structure**
```
Documents/
  Work/
    Projects/
    Meetings/
  Personal/
    Finance/
    Health/
```
Organized folders make filtering easier.

**Tag Strategically**
- Use tags for cross-cutting categories
- Examples: Important, Todo, Archive, Reference
- Don't over-tag (3-5 tags per file maximum)

**Screenshot placeholder:** *Well-organized file tree with logical folder structure*

### Optimizing Search

**Start Broad, Then Narrow**
1. Begin with general query
2. Review results
3. Add filters to narrow down
4. Refine query if needed

**Use the Right Search Mode**
- **Semantic:** For conceptual searches
- **Keyword:** For exact terms
- **Hybrid:** When unsure

**Learn from Results**
- Notice what works
- Adjust query based on results
- Save successful searches

**Regular Maintenance**
- Review and remove outdated files
- Update tags periodically
- Clean up duplicate files

### Performance Tips

**Faster Indexing**
- Close other applications
- Use SSD instead of HDD
- Increase batch size (Settings > Indexing)
- Disable deep content analysis for speed

**Faster Searches**
- Enable search cache
- Use filters to reduce result set
- Lower max results setting
- Close filter panel when not needed

**Reduce Memory Usage**
- Limit number of watch folders
- Disable thumbnail generation
- Lower results per page
- Close Vault when not in use

**Screenshot placeholder:** *Performance settings optimized for speed*

### Privacy Best Practices

**Sensitive Information**
- Don't index folders with passwords or private keys
- Use exclude patterns for sensitive files
- Consider encrypting database for extra security

**Shared Computers**
- Enable password protection
- Set auto-lock timeout
- Create backups to encrypted drive

**Before Sharing**
- Check what's indexed
- Review search history (if feature enabled)
- Clear cache before sharing computer

### Common Workflows

**Morning Routine**
1. Launch Vault
2. Check indexing status
3. Review files added yesterday
4. Tag important items

**Research Workflow**
1. Search for topic
2. Open relevant files
3. Tag as "Research - [Project Name]"
4. Export results as reference list

**Cleanup Workflow**
1. Search for old files (Date: > 1 year ago)
2. Review results
3. Remove or archive unneeded files
4. Tag keepers appropriately

**Backup Workflow**
1. Weekly: Create manual backup
2. Monthly: Verify backup integrity
3. Quarterly: Clean old backups
4. Yearly: Export important data

**Screenshot placeholder:** *Workflow diagram showing common usage patterns*

## Frequently Asked Questions

**Q: How many files can Vault handle?**
A: Vault can index millions of files, but performance is best with under 100,000 files per instance.

**Q: Does Vault work offline?**
A: Yes, completely! Vault works entirely offline after initial model download.

**Q: Does Vault modify my files?**
A: No, Vault is read-only. It never changes your original files.

**Q: Where is my data stored?**
A: All data is stored locally in your system's app data folder. See [First Launch](#first-launch) for exact locations.

**Q: Can I use Vault on multiple computers?**
A: Yes, but you need separate installations. Sync features are planned for future versions.

**Q: Why is indexing slow?**
A: Indexing is CPU-intensive because it generates AI embeddings. Larger files take longer. This is normal.

**Q: Can I search inside ZIP files?**
A: Vault indexes the file list in archives but not the content of compressed files.

**Q: How do I uninstall Vault?**
A: Use your system's uninstaller. To remove all data, also delete the app data folder.

**Q: Is my data private?**
A: Yes. Vault collects zero telemetry and never sends data anywhere. Everything stays on your computer.

**Q: Can I index network drives?**
A: Yes, but it's slower. Local drives are recommended for best performance.

---

## Getting More Help

**Resources:**
- [Getting Started Guide](getting-started.md) - Installation and setup
- [Advanced Features](advanced-features.md) - Power user features
- [GitHub Issues](https://github.com/yourusername/vault/issues) - Bug reports and feature requests
- [Discussions](https://github.com/yourusername/vault/discussions) - Community support

**Contributing:**
- Report bugs and request features on GitHub
- Share your workflows and tips
- Help improve documentation

Happy searching with Vault!
