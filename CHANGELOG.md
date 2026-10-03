# Changelog

All notable changes to Lattice will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **Explorer** (`/explorer`, ⌘6): a folder from disk beside a chat
  - Pick a folder once; the tree, the viewer and the model's tools stay inside it until you close it
  - Back and forward through the files opened (‹ › above the file, ⌘[ and ⌘])
  - Hide and show the folder tree (button at the left of the scope bar, ⌘\)
  - A line link whose path isn't in the folder (often just the file name) finds the file: one match opens at the cited lines, several are listed to pick from. The file found or picked is remembered for that folder, so the same link opens straight away; files read stay cached and are re-read in the background after a minute
  - Folder settings: each folder has its own system prompt and space (General by default), from its ⋯ menu or the scope bar. Its threads live in that space, which sets the library they search; changing it moves them. Migration `20261003100000_explorer_folder_settings.sql`
  - Read-only code viewer with line selection, syntax highlighting for about 45 languages (C and C++, Markdown with highlighted code fences, CMake, shell and more) loaded on demand, code folding, bracket matching and ⌘F find; the Studio lab editor shares the same language registry and colours
  - Explorer threads read the folder (`list_directory`, `read_file`, `search_files`, `find_files`, `search_folder`), can still search the web and the library, and answer with `path:10-24` line references that open in the viewer
  - Per-folder semantic index under `<data_dir>/folder-index/`, outside `lattice.db`: passages in SQLite with FTS5 plus a vector file, dense and BM25 results fused by reciprocal rank, kept current by a file watcher
  - Index progress in passages, percent and time left, in the scope bar, above the chat composer and in the log
  - Paused folder indexes finish in the background once the open folder's index is done: the most recently opened first, one at a time, without a watcher; opening a folder pauses that until its own run is done
  - **Your folders** on the start screen: every opened folder with its index status, size, threads and last-opened time; pin, rename, delete index, and remove (optionally with the folder's threads). No index is deleted unless you ask
  - Migration `20261002100000_conversation_explorer_root.sql` binds a conversation to its folder through `conversations.explorer_root`; `20261002110000_explorer_folders.sql` keeps the folder list
- **Learning Studio** (`/studio`, ⌘7): programs built from your sources with curriculum, practice, sandboxed labs, assessment evidence, a canvas, portability packs and Recall
  - Lab edits save durably with revision conflicts and recovery, in a multi-language code editor
  - Studio navigation grouped into five tabs

### Changed
- The navigation rail can be collapsed to icons on a wide window (**Collapse** at its bottom); the choice is remembered
- **Study left the rail**: flashcard decks moved into Learning Studio; the `study` backend stays and powers Recall
  - Explorer took Study's ⌘6 slot
- Startup JavaScript is back under the 1,050,000-byte budget checked by `scripts/check-initial-js-budget.mjs`: icon, date and command-menu code now loads with the page that uses it
- Learning Studio's library text read moved behind a repository so the repository-barrier and layer-boundary checks pass

### Fixed
- **Chat answers that stopped mid-thought with raw `<tool_call>` text**
  - The last round's "answer now, no tools" instruction went in as a system message, which llama.cpp merges into the top of the prompt, so the model never read it and kept writing calls as text. It is now the last user message
  - A call written as text in the last round is cut from the answer, which ends with a note that the turn ran out of tool rounds and can be continued
  - Explorer turns get 10 tool rounds instead of 5, and a turn whose context window is full answers at once rather than spending rounds on results with no room left
- **CRITICAL**: Fixed FTS5 trigger bug causing duplicate search results
  - FTS5 now properly aggregates chunks into document-level entries (one entry per document)
  - Search results no longer contain duplicate document IDs
  - Hybrid search (BM25 + vector) now merges correctly without duplicates
  - BM25 scoring fixed (calculated per document, not per chunk)
  - Performance improved by 30x (eliminated index bloat)
  - Reduced memory usage by 30x

### Changed
- **BREAKING**: FTS5 schema changed from chunk-level to document-level
  - Existing databases will be automatically migrated on startup
  - FTS5 index will be rebuilt from existing chunks
  - No action required for end users

## [1.0.0] - 2025-01-XX

### 🎉 Initial Public Release

First stable release of Lattice - your local-first AI knowledge base!

### Added

#### Core Features
- **Note Management**
  - Create, edit, and organize markdown notes
  - Full markdown support with live preview
  - Syntax highlighting for code blocks
  - Support for tables, lists, and formatting
  - Auto-save functionality

- **Search & Discovery**
  - Fast full-text search (FTS5)
  - Semantic search using AI embeddings
  - Hybrid search combining both approaches
  - Fuzzy matching for typo tolerance
  - Search-as-you-type with live results
  - Contextual search with BM25 ranking

- **AI-Powered Features**
  - Automatic tag generation based on content
  - Semantic similarity detection
  - Smart content connections
  - Embedding-based search (~400MB model)
  - Local inference (all processing on-device)

- **Linking & Connections**
  - Bidirectional wikilinks ([[note-name]])
  - Automatic backlink detection
  - Mentions and references (@-mentions)
  - Link autocomplete
  - Visual backlink panel

- **Organization**
  - Hierarchical tag system
  - Tag autocomplete
  - Tag-based filtering
  - Color-coded tags
  - Tag management interface

- **Daily Notes**
  - Quick daily note creation
  - Template support
  - Date navigation
  - Quick capture functionality
  - Calendar view

#### User Interface
- **Modern Design**
  - Clean, minimalist interface
  - Dark mode and light mode
  - Responsive layout
  - Smooth animations
  - Accessible design (WCAG compliant)

- **Command Palette**
  - Quick actions (Cmd/Ctrl+K)
  - Fuzzy search for commands
  - Keyboard-first navigation
  - Recently used commands

- **Keyboard Shortcuts**
  - Complete keyboard navigation
  - Vim-style keybindings (optional)
  - Customizable shortcuts
  - Context-aware shortcuts
  - Quick reference guide (Cmd/Ctrl+?)

- **Components**
  - Note editor with toolbar
  - File tree browser
  - Search results panel
  - Tag manager
  - Settings panel
  - Model download progress

#### Technical Features
- **Performance**
  - Handles 10,000+ notes efficiently
  - Search results in <100ms
  - Lazy loading for large vaults
  - Optimized rendering
  - Efficient memory usage

- **File System**
  - File watcher for external changes
  - Auto-sync with file system
  - Support for external editors
  - Conflict detection
  - Safe concurrent editing

- **Data & Storage**
  - SQLite database with FTS5
  - Vector index for embeddings
  - Automatic backups
  - Export to multiple formats (MD, HTML, JSON)
  - Import from various sources

- **Privacy & Security**
  - All data stored locally
  - No cloud sync required
  - No telemetry or tracking
  - Offline-first design
  - Encrypted exports (optional)

#### Platform Support
- **Windows**
  - Windows 10 1809+ (64-bit)
  - MSI installer
  - Start Menu integration
  - File associations

- **macOS**
  - macOS 10.15+ (Catalina or newer)
  - Intel and Apple Silicon support
  - DMG installer
  - Dock integration
  - Spotlight integration

- **Linux**
  - Ubuntu 20.04+, Debian 11+, Fedora 34+
  - .deb packages
  - AppImage (universal)
  - Desktop file integration
  - System tray support

### Technical Stack
- **Frontend**: React 18, TypeScript, Vite
- **Backend**: Rust, Tauri 2.0
- **Database**: SQLite with FTS5
- **AI Models**: Sentence Transformers (ONNX)
- **Search**: Hybrid (BM25 + Vector)
- **UI Components**: Radix UI, TailwindCSS

### System Requirements
- **RAM**: 4GB minimum, 8GB recommended
- **Storage**: 2GB free (includes models)
- **Internet**: Required only for initial model download

### Known Limitations
- Initial model download (~400MB) required on first run
- Large documents (>50MB) may be slow to process
- Vector search limited to 10,000 chunks per query
- No cloud sync (by design - local-first)

### Migration Notes
- First release - no migration needed
- Creates lattice at `~/Documents/Lattice` by default
- Models stored in app data directory

### Security
- No external network calls after initial setup
- All processing done locally
- No user data collection
- No analytics or telemetry

### Documentation
- Installation guide (INSTALLATION.md)
- Build guide (BUILD_GUIDE.md)
- User manual (README.md)
- API documentation
- Architecture documentation

### Credits
Special thanks to:
- Tauri team for the excellent framework
- ONNX Runtime for efficient inference
- Sentence Transformers for embedding models
- All beta testers and early adopters

---

## [Unreleased]

### Planned for v1.1.0
- [ ] Cloud sync (optional, encrypted)
- [ ] Mobile apps (iOS, Android)
- [ ] Plugin system
- [ ] Custom themes
- [ ] Graph view
- [ ] Vim mode improvements
- [ ] More export formats
- [ ] Collaborative editing

---

## Version History

### How to Read This Changelog

- **Added**: New features
- **Changed**: Changes in existing functionality
- **Deprecated**: Soon-to-be removed features
- **Removed**: Removed features
- **Fixed**: Bug fixes
- **Security**: Security fixes

### Version Numbering

We follow Semantic Versioning:
- **MAJOR** (1.x.x): Incompatible API changes
- **MINOR** (x.1.x): New features, backwards compatible
- **PATCH** (x.x.1): Bug fixes, backwards compatible

### Support Policy

- **v1.x**: Supported until v2.0 release
- Security updates for latest version only
- Bug fixes in latest minor version

---

[1.0.0]: https://github.com/J-Reed700/Lattice/releases/tag/v1.0.0
[Unreleased]: https://github.com/J-Reed700/Lattice/compare/v1.0.0...HEAD
