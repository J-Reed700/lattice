# Getting Started with Vault

Welcome! This guide will help you install Vault and get searching in just a few minutes.

## Table of Contents

- [System Requirements](#system-requirements)
- [Installation](#installation)
  - [Windows](#windows)
  - [macOS](#macos)
  - [Linux](#linux)
- [First Launch](#first-launch)
- [Quick Start Tutorial](#quick-start-tutorial)
- [Troubleshooting](#troubleshooting)

## System Requirements

Before installing Vault, make sure your system meets these requirements:

### Minimum Requirements
- **Operating System:** Windows 10+, macOS 10.15+, or Linux (Ubuntu 20.04+)
- **RAM:** 4 GB (8 GB recommended)
- **Storage:** 500 MB for application + space for your indexed files
- **Processor:** 64-bit processor (Intel or ARM)

### Recommended Requirements
- **RAM:** 8 GB or more
- **Storage:** SSD for better performance
- **Processor:** Multi-core processor for faster indexing

**Note:** Vault works entirely offline and does not require an internet connection after installation.

## Installation

Download the latest version of Vault for your operating system:

### Windows

1. **Download the Installer**
   - Visit the [Vault releases page](https://github.com/yourusername/vault/releases)
   - Download `Vault-Setup-x.x.x.exe`

2. **Run the Installer**
   - Double-click the downloaded file
   - If Windows SmartScreen appears, click "More info" then "Run anyway"
   - Follow the installation wizard
   - Choose installation location (default is recommended)

3. **Launch Vault**
   - Vault will appear in your Start Menu
   - Double-click the Vault icon to launch

**Screenshot placeholder:** *Windows installer wizard showing installation progress*

### macOS

1. **Download the Application**
   - Visit the [Vault releases page](https://github.com/yourusername/vault/releases)
   - Download `Vault-x.x.x.dmg`

2. **Install Vault**
   - Open the downloaded DMG file
   - Drag the Vault icon to your Applications folder
   - Eject the DMG

3. **First Launch**
   - Open Vault from Applications
   - If macOS says "Vault cannot be opened," right-click and select "Open"
   - Click "Open" in the security dialog

**Screenshot placeholder:** *macOS drag-to-install interface*

### Linux

#### Option 1: AppImage (Recommended)

1. **Download and Prepare**
   ```bash
   # Download the AppImage
   wget https://github.com/yourusername/vault/releases/download/v0.1.0/Vault-x.x.x.AppImage

   # Make it executable
   chmod +x Vault-x.x.x.AppImage
   ```

2. **Run Vault**
   ```bash
   ./Vault-x.x.x.AppImage
   ```

#### Option 2: Debian/Ubuntu (DEB package)

```bash
# Download the DEB file
wget https://github.com/yourusername/vault/releases/download/v0.1.0/vault_x.x.x_amd64.deb

# Install
sudo dpkg -i vault_x.x.x_amd64.deb

# Fix dependencies if needed
sudo apt-get install -f
```

#### Option 3: RPM-based (Fedora, CentOS, RHEL)

```bash
# Download the RPM file
wget https://github.com/yourusername/vault/releases/download/v0.1.0/vault-x.x.x.x86_64.rpm

# Install
sudo rpm -i vault-x.x.x.x86_64.rpm
```

**Screenshot placeholder:** *Linux desktop showing Vault application icon*

## First Launch

When you first open Vault, you'll see the welcome screen. Here's what to expect:

### Welcome Screen

The welcome screen introduces you to Vault and prepares the application for first use.

**What happens during first launch:**
1. **Database Creation** - Vault creates a local database to store your file index
2. **Model Download** - Downloads AI models for semantic search (about 100 MB)
3. **Configuration Setup** - Creates default settings

**Screenshot placeholder:** *Welcome screen with "Welcome to Vault" message and setup progress*

**Note:** The first launch may take 2-3 minutes to download models. This only happens once.

### Model Download Progress

You'll see a progress indicator while AI models are being downloaded:

- **Text Embedding Model** (~90 MB) - For semantic search
- **Configuration Files** (~10 MB) - For text processing

**Screenshot placeholder:** *Model download screen showing progress bars*

**Tip:** You can use Vault while models download, but AI-powered features will be unavailable until download completes.

### Database Location

Vault stores its database in your system's standard application data folder:

- **Windows:** `C:\Users\YourName\AppData\Roaming\com.vault.app\`
- **macOS:** `~/Library/Application Support/com.vault.app/`
- **Linux:** `~/.local/share/com.vault.app/`

This folder contains:
- `vault.db` - Your file index database
- `config.json` - Application settings
- `models/` - Downloaded AI models

## Quick Start Tutorial

Follow this 5-minute tutorial to index your first folder and perform your first search.

### Step 1: Add Your First Folder

1. **Open Settings**
   - Click the gear icon in the top-right corner
   - Or press `Ctrl+,` (Windows/Linux) or `Cmd+,` (macOS)

2. **Navigate to Indexing Tab**
   - Click "Indexing" in the left sidebar

3. **Add a Folder**
   - Click the "+ Add Folder" button
   - Browse to a folder you want to index (start with something small like Documents)
   - Select the folder and click "Select Folder"

4. **Configure Options**
   - **Recursive indexing:** Check to include subfolders (recommended)
   - **Auto-index:** Check to automatically update when files change

5. **Start Indexing**
   - Click "Start Indexing"
   - Watch the progress bar as Vault processes your files

**Screenshot placeholder:** *Settings window showing the "Add Folder" dialog with folder browser*

**Tip:** Start with a small folder (100-500 files) for your first try. You can add more folders later.

### Step 2: Wait for Indexing to Complete

The indexing panel shows real-time progress:

- **Files processed:** Number of files indexed
- **Files remaining:** Files left to process
- **Current file:** The file being processed right now
- **Estimated time:** How long until completion

**What's happening behind the scenes:**
1. Vault scans all files in the folder
2. Extracts text content from each file
3. Generates AI embeddings for semantic search
4. Stores everything in the local database

**Screenshot placeholder:** *Indexing progress panel showing files being processed with progress bar*

**Processing speed:** Expect about 50-100 files per minute depending on your computer's speed.

### Step 3: Perform Your First Search

Once indexing completes:

1. **Enter a Search Query**
   - Click the search bar at the top
   - Type what you're looking for (e.g., "project proposal from last month")
   - Press Enter or click the search icon

2. **View Results**
   - Results appear instantly, sorted by relevance
   - Each result shows:
     - File name and type
     - Matching content snippet
     - File location and date
     - Relevance score

3. **Open a File**
   - Click on any result to see more details
   - Click "Open" to open the file in its default application
   - Click "Show in Folder" to reveal it in your file explorer

**Screenshot placeholder:** *Search results showing multiple files with relevance scores and snippets*

### Step 4: Try Different Search Types

Vault supports multiple search modes:

**Semantic Search (Default)**
```
"document about marketing strategy"
"photos from my vacation"
"meeting notes with John"
```
Understands meaning and context, finds similar concepts.

**Keyword Search**
```
"exact phrase in quotes"
project AND proposal
marketing OR advertising
```
Finds exact word matches, supports boolean operators.

**Hybrid Search**
Combines both methods for best results. Enable in Settings > Search.

**Screenshot placeholder:** *Search bar with search mode dropdown showing semantic/keyword/hybrid options*

### Congratulations!

You've successfully:
- Installed Vault
- Indexed your first folder
- Performed semantic searches
- Opened files from search results

**Next steps:**
- Add more folders to expand your searchable content
- Explore advanced search features
- Set up auto-indexing for watch folders
- Customize settings to your preferences

## Troubleshooting

### Installation Issues

**Windows: "Windows Protected Your PC" warning**
- Click "More info"
- Click "Run anyway"
- This appears because Vault is not yet code-signed (coming soon)

**macOS: "Vault cannot be opened"**
- Right-click the Vault app
- Select "Open" from the menu
- Click "Open" in the dialog
- This only needs to be done once

**Linux: Missing dependencies**
```bash
# For Ubuntu/Debian
sudo apt-get install libwebkit2gtk-4.0-37 libgtk-3-0

# For Fedora
sudo dnf install webkit2gtk3 gtk3
```

### First Launch Issues

**Model download fails**
- **Check internet connection** - Models require a one-time download
- **Retry download** - Settings > Advanced > Re-download Models
- **Manual download** - Download models separately and place in models folder

**Database creation fails**
- **Check disk space** - Ensure you have at least 100 MB free
- **Check permissions** - Make sure you can write to the app data folder
- **Reset database** - Settings > Advanced > Reset Database (warning: deletes all indexed data)

### Indexing Issues

**Indexing is very slow**
- **Large files** - Big PDFs or images take longer to process
- **Disk speed** - SSD is much faster than HDD
- **System resources** - Close other applications to free up RAM
- **Reduce batch size** - Settings > Indexing > Batch Size (try 16 instead of 32)

**Some files not being indexed**
- **Unsupported format** - Check Settings > Indexing > File Types for supported formats
- **File permissions** - Ensure Vault can read the files
- **Excluded patterns** - Check Settings > Indexing > Excluded Patterns

**Indexing errors on specific files**
- **Corrupted files** - Some files may be damaged
- **Locked files** - Files in use by other applications
- **Very large files** - Files over 100 MB may time out (configurable)

### Search Issues

**No search results**
- **Wait for indexing** - Ensure indexing completed successfully
- **Check indexed folders** - Settings > Indexing to verify folders are added
- **Try different queries** - Use simpler or more specific terms
- **Database corruption** - Settings > Advanced > Verify Database

**Incorrect results**
- **Try hybrid search** - Combines semantic and keyword matching
- **Adjust similarity threshold** - Settings > Search > Similarity Threshold
- **Use keyword search** - For exact matches, switch to keyword mode

**Search is slow**
- **Large database** - With 100,000+ files, searches may take a few seconds
- **Optimize database** - Settings > Advanced > Optimize Database
- **Clear cache** - Settings > Advanced > Clear Search Cache

### Performance Issues

**High CPU usage**
- **During indexing** - This is normal; indexing is CPU-intensive
- **During idle** - Check if file watcher is detecting many changes
- **Constant high usage** - Check Settings > General > Auto-index (disable if not needed)

**High memory usage**
- **Large files** - Processing big PDFs requires more RAM
- **Many results** - Displaying thousands of results uses memory
- **Restart Vault** - Closes the app and clears memory

**Application not responding**
- **Wait** - Large operations may take time
- **Check task manager** - Verify Vault is actually running
- **Force quit** - Last resort: close and restart (may lose unsaved changes)

### Getting More Help

If you're still experiencing issues:

1. **Check the logs**
   - Settings > Advanced > Open Logs Folder
   - Look for error messages in the latest log file

2. **Search existing issues**
   - Visit [GitHub Issues](https://github.com/yourusername/vault/issues)
   - Search for similar problems

3. **Create a new issue**
   - Include your operating system and version
   - Describe what you were doing when the problem occurred
   - Attach relevant log files
   - Include screenshots if helpful

4. **Community support**
   - Join the [discussions forum](https://github.com/yourusername/vault/discussions)
   - Ask questions and share tips

## Next Steps

Now that you have Vault installed and working:

- **Read the [User Manual](user-manual.md)** - Learn about all features in depth
- **Explore [Advanced Features](advanced-features.md)** - Unlock power user capabilities
- **Customize your settings** - Make Vault work the way you want
- **Add more folders** - Expand your searchable knowledge base

Happy searching!

---

**Need help?** Check the [User Manual](user-manual.md) or visit our [support page](https://github.com/yourusername/vault/discussions).
