# Troubleshooting Guide

This guide helps you diagnose and resolve common issues with Recall/Vault.

## Table of Contents

- [Installation Issues](#installation-issues)
- [Performance Issues](#performance-issues)
- [Functionality Issues](#functionality-issues)
- [Data Issues](#data-issues)
- [When to Report a Bug](#when-to-report-a-bug)

---

## Installation Issues

### App Won't Install

**Symptoms:**
- Installation fails with error messages
- App won't launch after installation
- Missing dependencies errors

**Solutions:**

1. **Check System Requirements**
   - **Windows:** Windows 10 or later
   - **macOS:** macOS 10.15 (Catalina) or later
   - **Linux:** Modern distribution with glibc 2.31+
   - At least 4GB RAM, 500MB free disk space

2. **Install Missing Dependencies**

   For **Windows:**
   - Install Microsoft Visual C++ Redistributable
   - Update Windows to latest version

   For **macOS:**
   - Install Xcode Command Line Tools: `xcode-select --install`

   For **Linux:**
   - Install required libraries:
     ```bash
     sudo apt-get update
     sudo apt-get install libwebkit2gtk-4.0-dev libssl-dev
     ```

3. **Run Installer as Administrator** (Windows)
   - Right-click installer → "Run as administrator"

4. **Check Disk Space**
   - Ensure at least 500MB free space for installation
   - Additional space needed for database and indexed content

### Permission Errors

**Symptoms:**
- "Permission denied" during installation
- Cannot access application files
- Installation directory errors

**Solutions:**

1. **Install to User Directory**
   - Choose installation directory you own
   - Avoid `C:\Program Files` on Windows (requires admin rights)
   - Use `~/Applications` on macOS or `~/.local/bin` on Linux

2. **Check File Permissions**
   ```bash
   # Linux/macOS
   chmod +x /path/to/recall-vault
   ```

3. **Run as Administrator** (Windows only, if necessary)
   - Right-click app → Properties → Compatibility
   - Check "Run this program as an administrator"

### Antivirus/Security Software Blocking

**Symptoms:**
- Installation blocked by antivirus
- App won't start due to security software
- Files quarantined

**Solutions:**

1. **Add Exclusion to Antivirus**
   - Add Recall/Vault installation folder to exclusions
   - Add database directory to exclusions
   - Common paths:
     - Windows: `%LOCALAPPDATA%\Recall\Vault`
     - macOS: `~/Library/Application Support/Recall/Vault`
     - Linux: `~/.local/share/recall-vault`

2. **Whitelist the Application**
   - Allow network access for the app (if using cloud features)
   - Allow local server on port 5173 (development) or 8080 (production)

3. **Verify App Signature** (macOS)
   ```bash
   codesign -dv --verbose=4 /Applications/Recall\ Vault.app
   ```

### Platform-Specific Issues

#### Windows

**Issue: SmartScreen Warning**

Solution:
1. Click "More info"
2. Click "Run anyway"
3. Or download from verified source to avoid warning

**Issue: .NET Framework Missing**

Solution:
- Install .NET 6.0 or later from Microsoft

#### macOS

**Issue: "App is damaged and can't be opened"**

Solution:
```bash
sudo xattr -rd com.apple.quarantine /Applications/Recall\ Vault.app
```

**Issue: Gatekeeper Blocking**

Solution:
1. System Preferences → Security & Privacy
2. Click "Open Anyway" for Recall/Vault

#### Linux

**Issue: AppImage Won't Run**

Solution:
```bash
chmod +x Recall-Vault.AppImage
# If FUSE is not available:
./Recall-Vault.AppImage --appimage-extract-and-run
```

**Issue: Missing Libraries**

Solution:
```bash
# Debian/Ubuntu
sudo apt-get install libwebkit2gtk-4.0-37 libgtk-3-0

# Fedora
sudo dnf install webkit2gtk3 gtk3

# Arch
sudo pacman -S webkit2gtk gtk3
```

---

## Performance Issues

### Slow Indexing

**Symptoms:**
- Files taking a long time to index
- Indexing queue growing
- High CPU usage during indexing

**Diagnostic Steps:**

1. **Check File Sizes**
   - Files over 50MB are automatically skipped
   - Large PDFs or DOCX files take longer to process

2. **Check Number of Files**
   - Indexing thousands of files takes time
   - First-time indexing is slower than incremental updates

3. **Monitor System Resources**
   - Check CPU usage in Task Manager/Activity Monitor
   - Ensure adequate free RAM (4GB+ recommended)

**Solutions:**

1. **Reduce Indexing Scope**
   - Exclude large media folders (videos, images)
   - Exclude build/dependency folders (node_modules, .git)
   - Use selective folder watching instead of entire drives

2. **Adjust Indexing Settings**
   - Reduce concurrent indexing operations (default: 4)
   - Increase batch size for better throughput
   - Schedule indexing during idle time

3. **Split Large Files**
   - Files over 50MB won't be indexed
   - Split large documents into smaller chunks

4. **Optimize Database**
   - Clear old indexed content if no longer needed
   - Rebuild search index periodically

### High Memory Usage

**Symptoms:**
- App using excessive RAM (>2GB)
- System slowdown when app is running
- Out of memory errors

**Diagnostic Steps:**

1. **Check Database Size**
   - Navigate to app data directory
   - Check size of `vault.db` file
   - Large databases (>5GB) require more memory

2. **Check Indexed Content**
   - View number of indexed documents
   - Count of embeddings in vector store

**Solutions:**

1. **Clear Unused Data**
   - Remove indexed folders no longer needed
   - Clear old search history
   - Delete orphaned embeddings

2. **Reduce Vector Dimensions** (Advanced)
   - Use smaller embedding model
   - Trade-off: slightly reduced search accuracy

3. **Limit Concurrent Operations**
   - Reduce number of simultaneous indexing operations
   - Close other memory-intensive applications

4. **Restart Application**
   - Memory leaks may occur over extended use
   - Restart clears memory and resets connections

### Slow Search Results

**Symptoms:**
- Search taking >3 seconds to return results
- UI freezing during search
- Search timeouts

**Diagnostic Steps:**

1. **Check Database Size**
   - Large databases (>100,000 documents) slower to search
   - Check database file size

2. **Check Query Complexity**
   - Long search queries take more time
   - Complex filters slow down search

3. **Monitor System Load**
   - High CPU/memory usage affects search speed
   - Background indexing competes for resources

**Solutions:**

1. **Optimize Search Query**
   - Use more specific search terms
   - Use filters to narrow results
   - Avoid very generic terms

2. **Rebuild Search Index**
   - Settings → Advanced → Rebuild Search Index
   - This recreates FTS5 indexes for better performance

3. **Pause Indexing During Search**
   - Stop indexing operations temporarily
   - Search performance improves when indexing is idle

4. **Increase Search Timeout**
   - Settings → Advanced → Search Timeout
   - Increase from default 30 seconds

### UI Lag or Freezing

**Symptoms:**
- UI becomes unresponsive
- Clicks not registering
- Scrolling is choppy

**Solutions:**

1. **Reduce Results Display**
   - Limit search results to 50-100 items
   - Use pagination instead of infinite scroll

2. **Clear Browser Cache**
   - Settings → Advanced → Clear Cache
   - Restart application

3. **Disable Animations**
   - Settings → Appearance → Reduce Motion
   - Improves performance on slower systems

4. **Check GPU Acceleration**
   - Enable hardware acceleration if available
   - Update graphics drivers

---

## Functionality Issues

### Files Not Indexing

**Symptoms:**
- Added files don't appear in search
- Watch folder not detecting new files
- Specific file types being ignored

**Diagnostic Steps:**

1. **Check File Type Support**
   - Supported: `.txt`, `.md`, `.pdf`, `.docx`, `.html`
   - Unsupported files are silently skipped

2. **Check File Size**
   - Files over 50MB are automatically skipped
   - See error in indexing activity log

3. **Check Watch Folder Status**
   - Settings → Watch Folders
   - Ensure folder is enabled and path is correct

4. **Check File Permissions**
   - Ensure app has read access to files
   - Check file isn't locked by another application

**Solutions:**

1. **Verify File Format**
   - Convert unsupported files to supported formats
   - Use PDF for images that need OCR (future feature)

2. **Manual Reindex**
   - Right-click folder → "Re-index Folder"
   - Force re-scan of all files

3. **Check Indexing Activity Log**
   - View → Indexing Activity
   - Look for errors or skipped files

4. **Grant File Access Permissions**
   ```bash
   # Linux/macOS - check file permissions
   ls -la /path/to/file
   chmod 644 /path/to/file  # if needed
   ```

### Search Not Working

**Symptoms:**
- Search returns no results
- Search returns irrelevant results
- Search syntax errors

**Diagnostic Steps:**

1. **Verify Content is Indexed**
   - Check document count in status bar
   - Ensure files have been indexed

2. **Test Simple Query**
   - Try single word search
   - Verify search system is working

3. **Check Search Mode**
   - Semantic search vs. keyword search
   - Try switching modes

**Solutions:**

1. **Wait for Indexing to Complete**
   - New files need to be indexed before searchable
   - Check indexing queue status

2. **Use Correct Search Syntax**
   - Basic: just type words
   - Phrase: use quotes "exact phrase"
   - Filters: `tag:work`, `type:pdf`, `after:2024-01-01`

3. **Rebuild Search Index**
   - Settings → Advanced → Rebuild Search Index
   - This may take several minutes

4. **Clear Search Cache**
   - Settings → Advanced → Clear Cache
   - Restart application

### Watch Folders Not Detecting Changes

**Symptoms:**
- New files not automatically indexed
- File changes not reflected
- Deleted files still in search

**Diagnostic Steps:**

1. **Check Watch Folder Settings**
   - Settings → Watch Folders
   - Verify folder is enabled
   - Check recursive setting

2. **Check System File Watcher Limits** (Linux)
   ```bash
   cat /proc/sys/fs/inotify/max_user_watches
   ```

3. **Test Manual Indexing**
   - If manual works but auto doesn't, it's a watcher issue

**Solutions:**

1. **Increase File Watcher Limit** (Linux)
   ```bash
   echo fs.inotify.max_user_watches=524288 | sudo tee -a /etc/sysctl.conf
   sudo sysctl -p
   ```

2. **Restart File Watcher**
   - Settings → Watch Folders
   - Disable and re-enable folder
   - Or restart application

3. **Check Folder Permissions**
   - Ensure app can read folder and subfolders
   - Check for network drive issues (may not support watching)

4. **Use Manual Refresh**
   - Right-click folder → "Refresh Now"
   - Schedule periodic manual scans

### OCR Failing

**Note:** OCR is a planned feature not yet implemented.

When available:

**Symptoms:**
- Images not being text-extracted
- Scanned PDFs not searchable
- OCR errors in logs

**Solutions:**

1. **Check Tesseract Installation**
   - OCR requires Tesseract OCR engine
   - Install from: https://github.com/tesseract-ocr/tesseract

2. **Verify Language Packs**
   - Install language packs for non-English text
   - Set OCR language in Settings

3. **Image Quality Issues**
   - Use high-quality scans (300 DPI minimum)
   - Ensure good contrast and lighting

### Export Errors

**Symptoms:**
- Cannot export search results
- Export file corrupted
- Export takes too long

**Solutions:**

1. **Reduce Export Size**
   - Limit number of results
   - Export in batches

2. **Choose Different Format**
   - Try CSV instead of JSON
   - Try plain text if structured formats fail

3. **Check Disk Space**
   - Ensure adequate space for export file
   - Large exports may need several GB

4. **Check File Permissions**
   - Ensure write access to export directory
   - Try exporting to user home directory

---

## Data Issues

### Database Corruption

**Symptoms:**
- "Database is corrupted" error
- App crashes on startup
- Search returning no results
- Data integrity errors

**Diagnostic Steps:**

1. **Check Database File**
   - Location (Windows): `%LOCALAPPDATA%\Recall\Vault\vault.db`
   - Location (macOS): `~/Library/Application Support/Recall/Vault/vault.db`
   - Location (Linux): `~/.local/share/recall-vault/vault.db`

2. **Verify File Integrity**
   ```bash
   # If sqlite3 is installed
   sqlite3 vault.db "PRAGMA integrity_check;"
   ```

**Solutions:**

1. **Restore from Automatic Backup**
   - Backups located in same directory as database
   - Files named: `vault.db.backup-YYYY-MM-DD-HHMMSS`
   - Steps:
     1. Close application
     2. Rename `vault.db` to `vault.db.corrupt`
     3. Copy most recent backup to `vault.db`
     4. Restart application

2. **Rebuild Database**
   - Settings → Advanced → Rebuild Database
   - **WARNING:** This re-indexes all files (may take hours)
   - Settings and watch folders preserved

3. **Manual Recovery** (Advanced)
   ```bash
   # Export what data is recoverable
   sqlite3 vault.db ".dump" > recovered.sql
   # Create new database
   mv vault.db vault.db.corrupt
   sqlite3 vault.db < recovered.sql
   ```

4. **Start Fresh** (Last Resort)
   - Close application
   - Move `vault.db` to backup location
   - Restart application (new database created)
   - Re-add watch folders

### Missing Files

**Symptoms:**
- Previously indexed files not appearing
- Search results show files that no longer exist
- Document count decreased unexpectedly

**Solutions:**

1. **Refresh Index**
   - Right-click watch folder → "Refresh Now"
   - This removes missing files and adds new ones

2. **Check File Location**
   - Files may have been moved or renamed
   - Update watch folder path if folder moved

3. **Verify Files Weren't Deleted**
   - Check Recycle Bin / Trash
   - Check if accidentally deleted

4. **Check Filters**
   - Search filters may be hiding results
   - Clear all filters and search again

### Lost Search Results

**Symptoms:**
- Search that previously worked returns nothing
- Results different from before
- Missing recent documents

**Solutions:**

1. **Wait for Indexing**
   - Recent files may still be indexing
   - Check indexing queue

2. **Clear Search Cache**
   - Settings → Advanced → Clear Cache
   - May resolve stale results

3. **Rebuild Search Index**
   - Settings → Advanced → Rebuild Search Index
   - Recreates full-text search indexes

4. **Check Document Status**
   - View → All Documents
   - Verify documents are still indexed

### Settings Not Saving

**Symptoms:**
- Settings revert after restart
- Configuration changes not persisting
- Default settings keep appearing

**Solutions:**

1. **Check Write Permissions**
   - Config file location:
     - Windows: `%LOCALAPPDATA%\Recall\Vault\config.json`
     - macOS: `~/Library/Application Support/Recall/Vault/config.json`
     - Linux: `~/.config/recall-vault/config.json`
   - Ensure app can write to this location

2. **Close Multiple Instances**
   - Only one instance should run
   - Multiple instances may overwrite settings

3. **Repair Config File**
   - Close application
   - Delete or rename `config.json`
   - Restart (new config created with defaults)
   - Reapply settings

4. **Check File Locks**
   - Ensure config file isn't locked by antivirus
   - Check if file is read-only

---

## When to Report a Bug

Please report bugs when:

1. **The application crashes consistently** with the same steps
2. **Data loss or corruption** occurs
3. **Security issues** are discovered
4. **The problem persists** after trying all troubleshooting steps
5. **You encounter an error code** not listed in the documentation

### How to Report

1. **Gather Information:**
   - Operating system and version
   - Application version (Help → About)
   - Steps to reproduce the issue
   - Error messages or screenshots
   - Log files (if available)

2. **Create Issue:**
   - Visit: https://github.com/your-org/recall-vault/issues
   - Use bug report template
   - Include all gathered information
   - Be as specific as possible

3. **Log File Locations:**
   - Windows: `%LOCALAPPDATA%\Recall\Vault\logs`
   - macOS: `~/Library/Logs/Recall/Vault`
   - Linux: `~/.local/share/recall-vault/logs`

### Before Reporting

- [ ] Check [FAQ](faq.md) for common questions
- [ ] Check [Error Codes](error-codes.md) for specific errors
- [ ] Search existing issues for duplicates
- [ ] Try latest version of the app
- [ ] Include reproduction steps

---

## Additional Resources

- [FAQ](faq.md) - Frequently asked questions
- [Error Codes](error-codes.md) - Complete error code reference
- [User Guide](../README.md) - Complete user documentation
- [Community Forum](https://github.com/your-org/recall-vault/discussions) - Ask questions and share tips

---

**Still having issues?**

Join our community discussions or contact support with detailed information about your problem.
