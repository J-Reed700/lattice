# Frequently Asked Questions (FAQ)

Quick answers to common questions about Recall/Vault.

## Table of Contents

- [General Questions](#general-questions)
- [Privacy & Security](#privacy--security)
- [Features & Capabilities](#features--capabilities)
- [Performance & Limits](#performance--limits)
- [Comparisons](#comparisons)
- [Licensing & Usage](#licensing--usage)

---

## General Questions

### What is Recall/Vault?

Recall/Vault is a local-first personal knowledge management system that indexes your documents, files, and notes for instant semantic search. It runs entirely on your computer, keeping your data private and accessible offline.

### How does it work?

1. **Watch Folders:** You configure folders to monitor
2. **Automatic Indexing:** Files are automatically indexed when added or changed
3. **Semantic Search:** Content is analyzed using AI embeddings for intelligent search
4. **Instant Results:** Search across all indexed content in milliseconds

### What file types are supported?

Currently supported formats:
- **Text:** `.txt`, `.md` (Markdown)
- **Documents:** `.pdf`, `.docx`
- **Web:** `.html`

Planned support:
- Images (with OCR): `.jpg`, `.png`
- Presentations: `.pptx`
- Spreadsheets: `.xlsx`
- Code files: `.js`, `.py`, `.rs`, etc.

### Is it free?

Yes, Recall/Vault is open-source software released under the MIT license. You can use it for free, including for commercial purposes.

### Does it work offline?

Yes, completely! All indexing and search happens locally on your computer. No internet connection required.

### What platforms are supported?

- **Windows:** Windows 10 and later
- **macOS:** macOS 10.15 (Catalina) and later
- **Linux:** Modern distributions with glibc 2.31+

### How much disk space does it need?

- **Application:** ~100MB
- **Database:** Varies by indexed content
  - Approximately 10-20% of original file sizes
  - 10GB of documents = ~1-2GB database
- **Minimum:** 500MB free space
- **Recommended:** 5GB+ for typical usage

### How much RAM does it need?

- **Minimum:** 4GB system RAM
- **Recommended:** 8GB+ for large document collections
- **Typical usage:** 200-500MB when idle, 500MB-1GB when indexing

---

## Privacy & Security

### Is my data private?

**Yes, completely.** Everything runs locally on your computer:
- No cloud services required
- No data sent to external servers
- No telemetry or tracking
- No account or registration needed

### Where is my data stored?

All data is stored locally:

**Windows:**
- Database: `%LOCALAPPDATA%\Recall\Vault\vault.db`
- Config: `%LOCALAPPDATA%\Recall\Vault\config.json`
- Logs: `%LOCALAPPDATA%\Recall\Vault\logs`

**macOS:**
- Database: `~/Library/Application Support/Recall/Vault/vault.db`
- Config: `~/Library/Application Support/Recall/Vault/config.json`
- Logs: `~/Library/Logs/Recall/Vault`

**Linux:**
- Database: `~/.local/share/recall-vault/vault.db`
- Config: `~/.config/recall-vault/config.json`
- Logs: `~/.local/share/recall-vault/logs`

### Can I encrypt my database?

Database encryption is planned for a future release. Currently:
- Database stored unencrypted on disk
- Relies on OS-level encryption (BitLocker, FileVault, LUKS)
- Store database on encrypted drive for protection

### Does it send any data to the internet?

No, with one exception:
- **Update checks:** Optional check for new versions (can be disabled)
- **Everything else:** Runs completely offline

### Is it safe to index sensitive documents?

Yes, because:
- All processing happens locally
- No cloud services involved
- No data leaves your computer
- You control all data

**Best practices:**
- Use full-disk encryption (BitLocker, FileVault, LUKS)
- Set strong OS password
- Regular backups to secure location
- Don't index on shared computers

### Can other users on my computer see my indexed data?

Database files use standard OS permissions:
- Only accessible by your user account (by default)
- Other admin users may have access (OS limitation)
- Use OS-level permissions to restrict access

### What permissions does the app need?

**Required:**
- Read access to folders you want to index
- Write access to database location
- Local network (for internal components only)

**NOT required:**
- Internet access (except optional update checks)
- System administrator privileges
- Camera, microphone, or other sensors

---

## Features & Capabilities

### What is semantic search?

Unlike traditional keyword search, semantic search understands **meaning**:
- "meeting notes from last week" finds relevant notes even without exact words
- "python tutorial" finds programming guides, how-tos, docs
- Handles synonyms, related concepts automatically

It uses AI embeddings to understand context and relationships between words.

### Can it search inside PDFs?

Yes! The app extracts text from PDFs and indexes the content. You can search:
- Text-based PDFs (created digitally)
- Scanned PDFs (OCR support planned)

**Limitations:**
- Password-protected PDFs not supported
- Max file size: 50MB
- Image-only PDFs need OCR (coming soon)

### Does it support OCR (Optical Character Recognition)?

Not yet, but it's planned. Future releases will:
- Extract text from images
- Process scanned documents
- Index screenshots with text

**Workaround:** Use online OCR to convert scanned PDFs to text-based PDFs.

### Can I search by date, file type, or other filters?

Yes! Supported filters:
- **Date:** `after:2024-01-01`, `before:2024-12-31`
- **File type:** `type:pdf`, `type:docx`, `type:txt`
- **Tags:** `tag:work`, `tag:personal` (requires tagging)
- **Folder:** `folder:/path/to/folder`

Combine filters: `type:pdf after:2024-01-01 tag:work`

### Can I organize documents with tags?

Yes! Tag system features:
- Manual tagging
- Search by tag
- Filter by tag
- Tag hierarchies (planned)

### Does it sync across devices?

Not currently. Cloud sync is planned for future releases.

**Workaround:**
- Store database on cloud drive (Dropbox, OneDrive)
- **WARNING:** Don't run app simultaneously on multiple devices
- Risk of database corruption with concurrent access

### Can I export my data?

Yes! Export options:
- **Search results:** CSV, JSON, Markdown
- **Full database:** SQLite backup
- **Settings:** JSON export

### Can I share search results?

You can export and share:
- List of matching documents
- File paths and metadata
- Snippets and excerpts

**Cannot share:**
- Full document content (privacy/copyright)
- Database itself (large, machine-specific)

### Does it support multiple languages?

Yes! Indexing and search work with:
- English (optimized)
- Most European languages
- Unicode text

**Limitations:**
- Right-to-left languages may have display issues
- Some non-Latin scripts less accurate
- Language-specific features (stemming) English-focused

---

## Performance & Limits

### How many files can it index?

Tested with:
- **100,000+ documents:** Works well with adequate RAM
- **1 million+ documents:** Possible but slower searches
- **No hard limit:** Depends on system resources

**Performance factors:**
- Available RAM
- Disk speed (SSD recommended)
- Document sizes
- Search complexity

### How long does initial indexing take?

Approximate times:
- **1,000 documents:** 5-10 minutes
- **10,000 documents:** 1-2 hours
- **100,000 documents:** 12-24 hours

**Factors:**
- File sizes
- File types (PDFs slower than TXT)
- System performance
- First-time vs. incremental

### What's the maximum file size?

**Hard limit:** 50MB per file

Files larger than 50MB are automatically skipped to prevent:
- Out of memory errors
- Excessive processing time
- Database bloat

**Workaround:** Split large files into smaller chunks.

### How fast is search?

Typical search times:
- **Small collections (1k-10k docs):** <100ms
- **Medium collections (10k-100k docs):** 100-500ms
- **Large collections (100k+ docs):** 500ms-2s

**Factors affecting speed:**
- Database size
- Query complexity
- Available RAM
- Background indexing

### Can I limit CPU/memory usage?

Yes! Settings → Performance:
- **Indexing threads:** Reduce for lower CPU usage (default: 4)
- **Batch size:** Smaller batches = less memory
- **Indexing schedule:** Index during idle time only
- **Background throttling:** Reduce priority when app not focused

### Why is indexing slow?

Common causes:
1. **Large files:** 50MB files take time to process
2. **Many files:** 100,000+ files need time
3. **Complex documents:** PDFs with images, complex formatting
4. **Limited resources:** Low RAM or CPU
5. **Background tasks:** Other apps competing for resources

See [Troubleshooting - Slow Indexing](troubleshooting.md#slow-indexing)

---

## Comparisons

### How is this different from Windows Search / Spotlight?

| Feature | Recall/Vault | Windows/Mac Search |
|---------|-------------|-------------------|
| **Semantic search** | Yes | No (keyword only) |
| **Full document indexing** | Yes | Partial |
| **Privacy** | 100% local | May use cloud |
| **File type support** | Text, PDF, DOCX | Limited |
| **Search accuracy** | AI-powered | Basic matching |
| **Customization** | Full control | Limited |

### How is this different from Evernote / Notion?

| Feature | Recall/Vault | Evernote/Notion |
|---------|-------------|-----------------|
| **Storage** | Local files | Cloud |
| **Privacy** | Complete | Depends on service |
| **Cost** | Free | Subscription |
| **Offline** | Full support | Limited |
| **File formats** | Any supported type | Import only |
| **Lock-in** | None (your files) | Proprietary format |

### How is this different from DevonThink?

| Feature | Recall/Vault | DevonThink |
|---------|-------------|------------|
| **Platform** | Win/Mac/Linux | Mac only |
| **Cost** | Free | Paid |
| **Technology** | AI embeddings | Traditional indexing |
| **Open source** | Yes | No |
| **Customization** | High | High |

### How is this different from Obsidian?

| Feature | Recall/Vault | Obsidian |
|---------|-------------|----------|
| **Purpose** | Search existing files | Create and link notes |
| **File watching** | Automatic | Manual |
| **Search type** | Semantic | Text-based |
| **Note-taking** | View only | Full editor |
| **Use case** | Index existing docs | Create knowledge base |

**Best approach:** Use both!
- Obsidian for active note-taking
- Recall/Vault to search all notes + other documents

---

## Licensing & Usage

### Can I use it commercially?

Yes! MIT license allows:
- Personal use
- Commercial use
- Modification
- Distribution

**No restrictions on:**
- Company size
- Revenue
- Number of users
- Deployment type

### Can I modify the source code?

Yes! It's open source (MIT license):
- View and modify source code
- Create custom versions
- Contribute improvements
- Fork the project

### Do I need to credit the authors?

Not required for use, but appreciated:
- MIT license only requires including license text in distributions
- Attribution not required for personal use
- Credit welcome in projects using the code

### Can I distribute my modified version?

Yes! You can:
- Distribute modified versions
- Use different name
- Charge for your version (if you want)

**Requirements:**
- Include original MIT license text
- Indicate what you changed (recommended)

### Is there enterprise support?

Community support only (currently):
- GitHub Issues
- Community forum
- Documentation

Enterprise support may be available in future:
- Priority bug fixes
- Custom features
- Training and onboarding
- SLA guarantees

### Can I sponsor the project?

Yes! Sponsorship helps:
- Faster development
- Better documentation
- More features
- Long-term sustainability

Check project page for sponsorship options.

---

## Technical Questions

### What database does it use?

**SQLite** with extensions:
- **FTS5:** Full-text search
- **pgvector-style:** Vector embeddings (custom implementation)
- **JSON support:** Metadata storage

### What AI models does it use?

**Embedding model:** sentence-transformers (local)
- Runs entirely on your machine
- No API calls
- No internet needed
- ~500MB model download (one-time)

### Does it require Python?

No! The app is self-contained:
- All dependencies bundled
- Python bridge for AI features (internal)
- No manual Python installation needed

### What's the technology stack?

- **Frontend:** React, TypeScript, Tailwind CSS
- **Backend:** Rust (Tauri framework)
- **Database:** SQLite
- **Search:** Custom hybrid search (keyword + semantic)
- **AI:** Local embedding models

### Can I use my own AI models?

Not currently, but planned:
- OpenAI API integration
- Local LLM support (Ollama)
- Custom model endpoints

### Is there an API?

Not yet. Planned features:
- REST API for integration
- CLI for automation
- Plugin system

---

## Troubleshooting

### Where can I find help?

1. **[Troubleshooting Guide](troubleshooting.md)** - Step-by-step solutions
2. **[Error Codes](error-codes.md)** - Error reference
3. **GitHub Issues** - Report bugs
4. **Community Forum** - Ask questions

### How do I report a bug?

See [When to Report a Bug](troubleshooting.md#when-to-report-a-bug)

### How do I request a feature?

1. Check existing feature requests
2. Create GitHub issue with "Feature Request" label
3. Describe use case and benefit
4. Community votes on requests

### Where are the logs?

See [Log File Locations](troubleshooting.md#when-to-report-a-bug)

---

## Migration & Import

### Can I import from Evernote?

Not directly. Workaround:
1. Export Evernote notebooks as HTML/PDF
2. Save to folder
3. Add folder to Recall/Vault watch folders

### Can I import from Notion?

Yes! Similar process:
1. Export Notion workspace (Markdown format)
2. Save exported files to folder
3. Add folder to watch folders

### Can I import from DevonThink?

Yes! DevonThink stores files in database:
1. Export documents from DevonThink
2. Save to regular folder structure
3. Index with Recall/Vault

### Can I import from OneNote?

Not directly. OneNote uses proprietary format:
1. Export notebooks as PDF or Word
2. Save to folder
3. Add folder to Recall/Vault

---

## Future Features

### What features are planned?

**Near-term:**
- OCR for images and scanned documents
- Cloud sync across devices
- Mobile apps (iOS, Android)
- Browser extension for web clipping
- More file formats (XLSX, PPTX, code files)

**Long-term:**
- AI-powered summarization
- Automatic tagging and categorization
- Knowledge graph visualization
- Collaborative features
- Plugin system

### How can I influence the roadmap?

1. **Vote on feature requests** (GitHub discussions)
2. **Submit feature ideas** (GitHub issues)
3. **Contribute code** (Pull requests)
4. **Sponsor development** (Accelerate specific features)

### When will [feature X] be available?

Check project roadmap:
- GitHub milestones
- Project board
- Discussions forum

No fixed timelines - community-driven development.

---

## Still have questions?

- **[Troubleshooting Guide](troubleshooting.md)** - Common issues
- **[Error Codes](error-codes.md)** - Error reference
- **GitHub Discussions** - Community Q&A
- **GitHub Issues** - Bug reports and feature requests

Can't find your answer? Ask in community discussions!
