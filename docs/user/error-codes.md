# Error Codes Reference

Complete reference of error messages and codes in Recall/Vault.

## Table of Contents

- [Error Code Format](#error-code-format)
- [General Errors (1000-1999)](#general-errors-1000-1999)
- [File System Errors (2000-2999)](#file-system-errors-2000-2999)
- [Database Errors (3000-3999)](#database-errors-3000-3999)
- [Indexing Errors (4000-4999)](#indexing-errors-4000-4999)
- [Search Errors (5000-5999)](#search-errors-5000-5999)
- [Network Errors (6000-6999)](#network-errors-6000-6999)
- [Configuration Errors (7000-7999)](#configuration-errors-7000-7999)
- [Python Bridge Errors (8000-8999)](#python-bridge-errors-8000-8999)

---

## Error Code Format

Error messages follow this format:

```
[ERROR_TYPE] Error message description
```

**Error Severity:**
- **ERROR:** Critical failure requiring immediate attention
- **WARNING:** Non-critical issue that may affect functionality
- **INFO:** Informational message about recoverable condition

---

## General Errors (1000-1999)

### ERR_1000: Unknown Error

**Message:** "An unknown error occurred"

**Cause:** Unexpected error with no specific handler

**Solution:**
1. Restart the application
2. Check logs for details
3. Report bug if persists

---

### ERR_1001: Operation Timeout

**Message:** "Operation timed out after {seconds} seconds"

**Cause:** Operation took longer than maximum allowed time (default: 30s)

**Solution:**
1. Check system performance
2. Reduce operation scope
3. Increase timeout in Settings → Advanced → Timeouts
4. Try again when system less busy

---

### ERR_1002: Invalid Input

**Message:** "Invalid input: {details}"

**Cause:** User provided invalid data or parameters

**Solution:**
1. Check input format
2. Refer to documentation for valid values
3. Use suggested values if provided

---

### ERR_1003: Resource Not Available

**Message:** "Resource temporarily unavailable"

**Cause:** Requested resource is locked or in use

**Solution:**
1. Wait a moment and retry
2. Close other applications using the resource
3. Restart application if persists

---

## File System Errors (2000-2999)

### ERR_2000: File Not Found

**Message:** "File not found: {path}"

**Cause:** File doesn't exist at specified path

**Solution:**
1. Verify file path is correct
2. Check if file was moved or deleted
3. Refresh folder index
4. Update watch folder paths if folder moved

**Severity:** WARNING

---

### ERR_2001: Permission Denied

**Message:** "Permission denied: {path}. Please check file permissions."

**Cause:** Application doesn't have permission to access file/folder

**Solution:**
1. Check file/folder permissions
2. Grant read access to the application
3. Move file to accessible location
4. Run app with appropriate permissions

**Windows:**
```powershell
icacls "C:\path\to\file" /grant %USERNAME%:R
```

**Linux/macOS:**
```bash
chmod 644 /path/to/file  # for files
chmod 755 /path/to/folder  # for folders
```

**Severity:** ERROR

---

### ERR_2002: File Too Large

**Message:** "File is too large to index: {path} ({size} MB). Maximum size is 50 MB."

**Cause:** File exceeds maximum size limit (50MB)

**Solution:**
1. Split large file into smaller chunks
2. Exclude large files from indexing
3. Convert to compressed format if possible

**Note:** Size limit prevents out-of-memory errors and excessive processing time.

**Severity:** WARNING

---

### ERR_2003: Disk Space Insufficient

**Message:** "Insufficient disk space. Please free up disk space and try again."

**Cause:** Not enough free disk space for operation

**Solution:**
1. Free up disk space (at least 500MB recommended)
2. Delete old backups
3. Move database to drive with more space
4. Clean up temporary files

**Windows:**
```
Settings → System → Storage → Free up space now
```

**macOS:**
```
About This Mac → Storage → Manage
```

**Linux:**
```bash
df -h  # check disk usage
ncdu /  # find large directories
```

**Severity:** ERROR

---

### ERR_2004: Path Invalid

**Message:** "Invalid path: {path}"

**Cause:** Path contains invalid characters or format

**Solution:**
1. Check path syntax
2. Remove special characters
3. Use absolute path instead of relative
4. Check for path injection attempts (security)

**Severity:** ERROR

---

### ERR_2005: File Locked

**Message:** "File is locked by another process: {path}"

**Cause:** File is open in another application

**Solution:**
1. Close file in other applications
2. Wait for other process to release file
3. Restart computer if can't identify locking process

**Windows - Find locking process:**
```
Resource Monitor → CPU tab → Associated Handles → Search filename
```

**Linux - Find locking process:**
```bash
lsof /path/to/file
```

**Severity:** WARNING

---

### ERR_2006: Cannot Open Directory

**Message:** "Cannot open directories"

**Cause:** Attempted to open directory instead of file

**Solution:**
1. Open specific file instead
2. Use folder browser if wanting to browse contents
3. Select file within directory

**Severity:** WARNING

---

## Database Errors (3000-3999)

### ERR_3000: Database Error

**Message:** "Database error: {details}. Try restarting the application."

**Cause:** General database operation failure

**Solution:**
1. Restart application
2. Check database file isn't corrupted
3. Restore from backup if needed
4. See [Database Corruption](troubleshooting.md#database-corruption)

**Severity:** ERROR

---

### ERR_3001: Database Corrupted

**Message:** "Database is corrupted. Please restore from backup."

**Cause:** Database file integrity check failed

**Solution:**
1. **Restore from automatic backup:**
   - Close application
   - Navigate to database directory
   - Copy `vault.db.backup-[date]` to `vault.db`
   - Restart application

2. **Rebuild database:**
   - Settings → Advanced → Rebuild Database
   - Re-index all content (may take hours)

3. **Start fresh:**
   - Close application
   - Rename `vault.db` to `vault.db.old`
   - Restart (creates new database)
   - Re-add watch folders

**Database locations:**
- Windows: `%LOCALAPPDATA%\Recall\Vault\vault.db`
- macOS: `~/Library/Application Support/Recall/Vault/vault.db`
- Linux: `~/.local/share/recall-vault/vault.db`

**Severity:** ERROR

---

### ERR_3002: Database Locked

**Message:** "Database is locked by another process"

**Cause:** Another instance accessing database or file lock not released

**Solution:**
1. Close all application instances
2. Wait 30 seconds
3. Restart application
4. Check for zombie processes

**Windows:**
```
Task Manager → Details → Find "Recall Vault" → End Task
```

**Linux/macOS:**
```bash
ps aux | grep recall
kill [PID]  # if found
```

**Severity:** ERROR

---

### ERR_3003: Query Failed

**Message:** "Database query failed: {details}"

**Cause:** SQL query execution error

**Solution:**
1. Retry operation
2. Restart application
3. Check database integrity
4. Report bug if persists with specific query

**Severity:** ERROR

---

### ERR_3004: Schema Mismatch

**Message:** "Database schema version mismatch. Migration required."

**Cause:** Database from different app version

**Solution:**
1. Update to latest app version
2. Automatic migration should run
3. Backup database before migration
4. Restore from backup if migration fails

**Severity:** ERROR

---

## Indexing Errors (4000-4999)

### ERR_4000: Indexing Failed

**Message:** "Failed to index file: {path}"

**Cause:** General indexing failure

**Solution:**
1. Check error details in indexing activity log
2. Verify file is accessible
3. Check file isn't corrupted
4. Try manual re-index

**Severity:** WARNING

---

### ERR_4001: Unsupported File Type

**Message:** "Unsupported file type: {type}. Convert to a supported format (PDF, DOCX, TXT, MD, HTML)."

**Cause:** File type not supported for indexing

**Supported formats:**
- Text: `.txt`, `.md`
- Documents: `.pdf`, `.docx`
- Web: `.html`

**Solution:**
1. Convert file to supported format
2. Use PDF for universal compatibility
3. Extract text manually and save as .txt
4. Request format support in feature request

**Severity:** INFO

---

### ERR_4002: Extraction Failed

**Message:** "Content extraction failed: {reason}"

**Cause:** Unable to extract text from document

**Common reasons:**
- Corrupted PDF
- Password-protected document
- Unsupported PDF features
- Malformed DOCX structure

**Solution:**
1. Try opening file in native application
2. Export/save as new file
3. Convert to different format
4. Check file isn't password-protected

**Severity:** WARNING

---

### ERR_4003: Queue Full

**Message:** "Indexing queue is full. Wait for current operations to complete."

**Cause:** Too many files queued for indexing (limit: 10,000)

**Solution:**
1. Wait for queue to process
2. Pause adding new folders
3. Reduce number of concurrent indexing operations
4. Increase queue size in advanced settings (risk: high memory)

**Severity:** WARNING

---

### ERR_4004: Embedding Generation Failed

**Message:** "Failed to generate embeddings: {reason}"

**Cause:** AI embedding creation error

**Common reasons:**
- Python bridge not running
- Embedding model not downloaded
- Out of memory
- Text too long

**Solution:**
1. Restart application (restarts Python bridge)
2. Check embedding model downloaded: Settings → Models
3. Reduce text chunk size
4. Check logs for Python errors

**Severity:** ERROR

---

### ERR_4005: Chunking Failed

**Message:** "Failed to chunk document: {path}"

**Cause:** Error splitting document into searchable chunks

**Solution:**
1. Check document isn't corrupted
2. Try re-saving document
3. Convert to plain text format
4. Check document isn't unusually formatted

**Severity:** WARNING

---

### ERR_4006: Metadata Extraction Failed

**Message:** "Failed to extract metadata from: {path}"

**Cause:** Cannot read document properties

**Solution:**
1. File will be indexed without metadata
2. Check file permissions
3. Verify file format is valid
4. No action required (non-critical)

**Severity:** INFO

---

## Search Errors (5000-5999)

### ERR_5000: Search Failed

**Message:** "Search operation failed: {reason}"

**Cause:** General search error

**Solution:**
1. Retry search
2. Simplify search query
3. Clear search cache: Settings → Advanced → Clear Cache
4. Restart application

**Severity:** ERROR

---

### ERR_5001: Invalid Search Query

**Message:** "Invalid search query: {details}"

**Cause:** Malformed search syntax

**Solution:**
1. Check query syntax
2. Remove special characters
3. Use simpler query
4. Refer to search syntax guide

**Valid syntax:**
- Basic: `search terms`
- Phrase: `"exact phrase"`
- Filter: `type:pdf after:2024-01-01`
- Combine: `"project plan" type:docx tag:work`

**Severity:** WARNING

---

### ERR_5002: Search Timeout

**Message:** "Search timed out. Try a more specific query."

**Cause:** Search exceeded maximum time (30s default)

**Solution:**
1. Use more specific search terms
2. Add filters to narrow results
3. Increase timeout: Settings → Advanced → Search Timeout
4. Optimize database: Settings → Advanced → Rebuild Index

**Severity:** WARNING

---

### ERR_5003: Index Unavailable

**Message:** "Search index is unavailable. Rebuild required."

**Cause:** Search index corrupted or missing

**Solution:**
1. Settings → Advanced → Rebuild Search Index
2. Wait for rebuild to complete (may take time)
3. Search will be available after rebuild

**Severity:** ERROR

---

### ERR_5004: Too Many Results

**Message:** "Query returned too many results. Please refine your search."

**Cause:** Search matched more than maximum results (default: 10,000)

**Solution:**
1. Use more specific search terms
2. Add date filters
3. Add type filters
4. Use phrase search for exact matches

**Severity:** INFO

---

## Network Errors (6000-6999)

### ERR_6000: Network Error

**Message:** "Network error: {details}. Please check your connection."

**Cause:** Network connectivity issue

**Solution:**
1. Check internet connection
2. Verify firewall settings
3. Disable VPN temporarily
4. Restart router

**Note:** Network only needed for:
- Update checks
- Model downloads
- Future cloud sync features

**Severity:** WARNING

---

### ERR_6001: Connection Refused

**Message:** "Connection failed. Please check if required services are running."

**Cause:** Cannot connect to internal service

**Common reasons:**
- Python bridge not started
- Port already in use
- Firewall blocking

**Solution:**
1. Restart application
2. Check firewall allows local connections
3. Ensure no port conflicts
4. Check antivirus not blocking

**Severity:** ERROR

---

### ERR_6002: Connection Timeout

**Message:** "Connection timed out. Please try again."

**Cause:** Service not responding in time

**Solution:**
1. Check system load
2. Retry operation
3. Restart application
4. Check no background updates running

**Severity:** WARNING

---

### ERR_6003: Download Failed

**Message:** "Download failed: {resource}"

**Cause:** Cannot download required resource (e.g., embedding model)

**Solution:**
1. Check internet connection
2. Check firewall/proxy settings
3. Try again later
4. Manual download if available

**Severity:** ERROR

---

## Configuration Errors (7000-7999)

### ERR_7000: Invalid Configuration

**Message:** "Configuration error: {setting}. Please check your settings."

**Cause:** Invalid value in configuration file

**Solution:**
1. Settings → Restore Defaults
2. Or manually edit config file
3. Remove invalid entries
4. Restart application

**Config locations:**
- Windows: `%LOCALAPPDATA%\Recall\Vault\config.json`
- macOS: `~/Library/Application Support/Recall/Vault/config.json`
- Linux: `~/.config/recall-vault/config.json`

**Severity:** ERROR

---

### ERR_7001: Config File Corrupted

**Message:** "Configuration file is corrupted. Restoring defaults."

**Cause:** Cannot parse config file

**Solution:**
1. Application will create new config with defaults
2. Re-apply your settings manually
3. Check backup config if available

**Severity:** WARNING

---

### ERR_7002: Config Read Failed

**Message:** "Cannot read configuration file"

**Cause:** Permission or file system error

**Solution:**
1. Check file permissions
2. Ensure config directory exists
3. Check disk not full
4. Try running app with appropriate permissions

**Severity:** ERROR

---

### ERR_7003: Config Write Failed

**Message:** "Cannot save configuration. Settings not persisted."

**Cause:** Cannot write to config file

**Solution:**
1. Check file permissions
2. Ensure disk not full
3. Check antivirus not blocking
4. Verify config file not read-only

**Severity:** ERROR

---

## Python Bridge Errors (8000-8999)

### ERR_8000: Python Bridge Unavailable

**Message:** "AI features unavailable. Python backend may not be running."

**Cause:** Python subprocess not started or crashed

**Impact:**
- Semantic search unavailable
- Embedding generation fails
- Indexing may be degraded

**Solution:**
1. Restart application (auto-restarts bridge)
2. Check logs for Python errors
3. Verify Python dependencies installed
4. Report bug if persists

**Severity:** ERROR

---

### ERR_8001: Python Bridge Timeout

**Message:** "Request timed out after {seconds} seconds"

**Cause:** Python operation took too long

**Solution:**
1. Reduce batch size
2. Increase timeout in settings
3. Check system resources
4. Restart application

**Severity:** WARNING

---

### ERR_8002: Python Error

**Message:** "Python error (code {code}): {message}"

**Cause:** Error in Python subprocess

**Solution:**
1. Check error message for details
2. Restart application
3. Update to latest version
4. Report bug with error details

**Severity:** ERROR

---

### ERR_8003: Circuit Breaker Open

**Message:** "Circuit breaker is open - too many failures"

**Cause:** Repeated failures triggered circuit breaker (protection mechanism)

**Solution:**
1. Wait 60 seconds for circuit breaker to reset
2. Restart application
3. Check logs for underlying error
4. Fix underlying issue before retry

**Note:** Circuit breaker prevents cascading failures by temporarily stopping operations after repeated failures.

**Severity:** ERROR

---

### ERR_8004: Model Not Found

**Message:** "Embedding model not found. Download the required model from Settings."

**Cause:** AI model not downloaded or corrupted

**Solution:**
1. Settings → Models
2. Download required model (one-time, ~500MB)
3. Wait for download to complete
4. Restart application

**Model location:**
- Windows: `%LOCALAPPDATA%\Recall\Vault\models`
- macOS: `~/Library/Application Support/Recall/Vault/models`
- Linux: `~/.local/share/recall-vault/models`

**Severity:** ERROR

---

### ERR_8005: Ollama Unavailable

**Message:** "LLM unavailable. Please start Ollama (run 'ollama serve' in terminal)."

**Cause:** Ollama service not running (future feature)

**Solution:**
1. Install Ollama from https://ollama.ai
2. Start Ollama service:
   ```bash
   ollama serve
   ```
3. Verify running: `ollama list`
4. Restart Recall/Vault

**Note:** This is for future LLM features.

**Severity:** WARNING

---

## Common Error Patterns

### "Permission denied" Errors

**Pattern:** Various operations fail with permission errors

**Common Causes:**
1. Running from protected directory
2. Antivirus blocking access
3. File/folder permissions
4. Admin rights needed

**Solution:**
1. Move database to user directory
2. Add antivirus exclusions
3. Check file permissions
4. Run with appropriate rights (avoid admin if possible)

---

### "Timeout" Errors

**Pattern:** Operations timing out

**Common Causes:**
1. System overloaded
2. Large files/datasets
3. Slow disk (HDD vs SSD)
4. Background processes

**Solution:**
1. Increase timeout values
2. Reduce operation scope
3. Upgrade to SSD
4. Close background apps

---

### Database Errors

**Pattern:** Various database operations failing

**Common Causes:**
1. Database corruption
2. Multiple instances running
3. Disk issues
4. Out of space

**Solution:**
1. Restore from backup
2. Close duplicate instances
3. Check disk health
4. Free up space

---

### Python/AI Errors

**Pattern:** Embedding or AI features failing

**Common Causes:**
1. Python bridge crashed
2. Model not downloaded
3. Out of memory
4. Incompatible versions

**Solution:**
1. Restart application
2. Download models
3. Increase RAM or reduce batch size
4. Update to latest version

---

## Error Reporting

When reporting errors, please include:

1. **Error Code/Message:** Exact error text
2. **Steps to Reproduce:** What you did before error
3. **System Info:**
   - OS and version
   - App version (Help → About)
   - Available RAM
   - Disk space
4. **Logs:** From appropriate log directory
5. **Screenshots:** If UI-related error

**Log locations:**
- Windows: `%LOCALAPPDATA%\Recall\Vault\logs`
- macOS: `~/Library/Logs/Recall/Vault`
- Linux: `~/.local/share/recall-vault/logs`

---

## Additional Resources

- **[Troubleshooting Guide](troubleshooting.md)** - Detailed solutions
- **[FAQ](faq.md)** - Common questions
- **GitHub Issues** - Report bugs
- **Community Forum** - Get help

---

**Error not listed?**

1. Check [Troubleshooting Guide](troubleshooting.md)
2. Search existing GitHub issues
3. Create new issue with details
4. Include error message and logs
