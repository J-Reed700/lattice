# Error Messages and Codes

When something goes wrong, Lattice shows you a plain message. Behind most messages is a short code such as `DATABASE_ERROR`, which Lattice records in its log. This page explains what each code means and what to try.

## Table of Contents

- [Where errors appear](#where-errors-appear)
- [Finding the code for an error](#finding-the-code-for-an-error)
- [Files and import](#files-and-import)
- [Models, search and indexing](#models-search-and-indexing)
- [Database](#database)
- [Backups](#backups)
- [Input, settings and state](#input-settings-and-state)
- [Network](#network)
- [Everything else](#everything-else)
- [Messages without a code](#messages-without-a-code)

---

## Where errors appear

- **Notifications.** Most failures appear as a notification in the corner of the window with a short message, such as "Couldn't add this folder" followed by the reason.
- **Chat notices.** Problems with the chat model appear as a notice in the chat: "No chat model yet.", "Chat model is downloading…", "Warming up the model…" or "The chat model didn't load." These link to model settings.
- **Startup dialogs.** If Lattice can't start, it shows a dialog titled **Application Setup Failed** or **Application Initialization Failed**, then quits. See [Lattice won't start](troubleshooting.md#lattice-wont-start).
- **Settings > General > Logs.** A record of recent errors and warnings, kept on this device.

## Finding the code for an error

1. Open **Settings** (⌘, on macOS) and choose **Logs** under General.
2. Click the **Errors** count to show only errors, or search for words from the message.
3. Expand the entry. If the error came with a code, its details include a `"code"` field.

The Logs page keeps the newest 300 entries. **Export logs** saves them as a JSON file. Lattice removes common secrets (API keys, tokens, passwords) and replaces your home folder name with `/[USER]` before storing an entry. Read the file before you share it anyway.

---

## Files and import

| Code | What it means | What to try |
|------|---------------|-------------|
| `FILE_NOT_FOUND` | The file isn't where Lattice expected it. It was moved, renamed or deleted, or a drive was disconnected. | Reconnect the drive, or import the file again from its new location. |
| `FILE_TOO_LARGE` | The file is over the size limit. Imports accept files up to 50 MB. The in-app viewer opens files up to 10 MB. | Split the document, or export a smaller version (for example a PDF without embedded images). |
| `UNSUPPORTED_FILE_TYPE` | Lattice can't read this kind of file. | Convert it to a supported format. See [What file types can I import?](faq.md#what-file-types-can-i-import) |
| `FILE_READ_ERROR` | The file exists but couldn't be read. | Check that it opens in another app and isn't locked or still syncing. |
| `FILE_SYSTEM_ERROR` | A disk operation failed: the disk is full, a folder isn't writable, or something else went wrong at the OS level. | Free some disk space and check that your user account owns Lattice's data folder. |
| `PERMISSION_DENIED` | The operating system refused access to a file or folder. During a backup restore, it means the passphrase or recovery code was wrong. | Grant access to the folder, or choose one you own. For a restore, re-enter the passphrase or use the 24-word recovery code. |
| `EXTRACTION_ERROR` | Lattice couldn't pull text out of the file. The file may be damaged, encrypted or password-protected. | Open and re-save the file in its original app, or remove the password. |

**Scanned PDFs.** Lattice has no OCR. A page that is only an image is skipped and the rest of the PDF is still imported. If a PDF is all scans, its text won't be searchable.

## Models, search and indexing

| Code | What it means | What to try |
|------|---------------|-------------|
| `MODEL_NOT_LOADED` | The model this action needs isn't downloaded, or it failed to load. | Open **Settings > AI > Downloaded** and check which model has the Chat, Utility or Embedding role. Download one from **Settings > AI > Models** if needed. |
| `SERVICE_NOT_AVAILABLE` | A part of Lattice this action depends on isn't running. The usual cause is that the embedding model couldn't load, which blocks importing and indexing. | Check that an embedding model is downloaded and active, then retry. Restart Lattice if it persists. |
| `EMBEDDING_ERROR` | Turning text into search vectors failed. | Retry. If it keeps happening, restart Lattice. If it still fails, try a different embedding model. |
| `TOKENIZATION_ERROR` | The model's tokenizer couldn't process the text. | Retry. If one document always fails, look in Settings > General > Logs to see which one, and re-save or convert it. |
| `QUEUE_FULL` | Too much work is waiting, for example a very large import. | Wait for the current work to finish, then retry. |
| `RATE_LIMIT_EXCEEDED` | Too many requests arrived in a short time. This can come from Lattice's own limits (for example many searches in a row) or from Hugging Face during a model download. | Wait a minute and retry. |

## Database

| Code | What it means | What to try |
|------|---------------|-------------|
| `DATABASE_ERROR` | A read or write to Lattice's database failed. | Retry. If it repeats, quit and reopen Lattice. If it keeps happening, see [Database problems](troubleshooting.md#database-problems). |
| `DATABASE_CONNECTION_ERROR` | Lattice couldn't get a database connection in time, usually because the database is busy with a long operation. | Wait for imports or indexing to finish, then retry. |
| `MIGRATION_ERROR` | Lattice couldn't update the database to the version this build expects. | Lattice is pre-release and doesn't convert data from older builds. See [Lattice won't start](troubleshooting.md#lattice-wont-start). |
| `CONCURRENT_MODIFICATION` | Something else changed the same item while you were editing it. | Reload the item and make your change again. |
| `CONSTRAINT_VIOLATION` | The change would conflict with existing data, for example a duplicate that has to be unique. | Use a different name or value. |

## Backups

| Code | What it means | What to try |
|------|---------------|-------------|
| `BACKUP_CREATION_FAILED` | A backup couldn't be written. | Check free space and that the backup folder still exists and is writable. If the folder was on a removable drive, reconnect it. |
| `BACKUP_RESTORE_FAILED` | A restore didn't complete. | Read the message. "backup file has not been downloaded by …" means the file is still only in the cloud: open it in your sync app, wait for it to download, then retry. |
| `BACKUP_CORRUPTED` | The backup file is damaged or incomplete. | Restore from an older archive in the same folder. |

Two restore messages arrive with a different code:

- "wrong passphrase or recovery code" arrives as `PERMISSION_DENIED`. Re-enter the passphrase, or use the 24-word recovery code.
- "backup archive format version … is not supported by this version of Lattice" arrives as a generic error. The archive was made by a newer Lattice, so update Lattice and retry.

## Input, settings and state

| Code | What it means | What to try |
|------|---------------|-------------|
| `INVALID_INPUT` | A value was rejected: an empty field, a bad path or URL, or a file over the import limit ("File too large: … bytes"). | Check the value named in the message. URLs need `http://` or `https://`. |
| `VALIDATION_ERROR` | A value is outside the allowed range or format. | Correct the field named in the message. |
| `INVALID_CONFIG` | A stored setting is invalid. | Fix the setting named in the message. As a last resort, use **Reset all** at the bottom of the Settings sidebar. |
| `INVALID_STATE` | The action can't run right now, for example because another operation is still in progress. | Wait for the other operation to finish, then retry. |
| `NOT_FOUND` | The item no longer exists: a document, conversation, model or record. | Refresh the view. If you deleted the item, it's gone. |
| `SECURITY_VIOLATION` | A security check failed, or the system keychain refused a request (for example while saving an API key). | Unlock your keychain or credential store and retry. |

## Network

| Code | What it means | What to try |
|------|---------------|-------------|
| `NETWORK_ERROR` | A request to another server failed. This happens with model downloads, URL imports, web search, a remote model server or a cloud provider. | Check your connection. For a remote server, check its URL in **Settings > AI > Chat**. |
| `TIMEOUT` | An operation took too long. | Retry. For a slow remote model server, check that it's running and reachable. |

## Everything else

| Code | What it means | What to try |
|------|---------------|-------------|
| `PROCESSING_ERROR` | A step inside Lattice failed. | Retry. If it repeats, check Settings > General > Logs for the details. |
| `SERIALIZATION_ERROR`, `DESERIALIZATION_ERROR`, `PARSING_ERROR` | Lattice couldn't read or write data in the expected format, for example a damaged settings or tools file you imported. | Check the file you imported. Otherwise retry, then report it. |
| `INTERNAL_ERROR` | Something unexpected went wrong inside Lattice. | Retry. If it repeats, report it with exported logs. See [Reporting a problem](troubleshooting.md#reporting-a-problem). |
| `UNKNOWN` | The error came back as a plain message without a code. Many operations report errors this way. | Read the message itself. It usually says what failed. |

A few other codes are defined but not used by this version: `ALREADY_EXISTS`, `GONE`, `UNAUTHORIZED`, `FILE_WRITE_ERROR`, `SERVICE_INITIALIZATION_ERROR`, `MODEL_LOAD_ERROR` and `NOT_IMPLEMENTED`.

---

## Messages without a code

Some important messages are plain sentences:

- **"Lattice's bundled llama-server can't run on this machine: …"** The local model engine couldn't start. The message ends with what to do. That might be "Update your graphics driver to get a Vulkan runtime", a note that Lattice is using its compatibility build, or "Reinstall Lattice." See [Chat doesn't answer](troubleshooting.md#chat-doesnt-answer-or-the-model-wont-load).
- **"Failed to create database connection at …"** or **"Failed to initialize database schema."** These appear in the startup dialog. See [Lattice won't start](troubleshooting.md#lattice-wont-start).
- **"No downloaded utility model is available for use."** Document search still works with basic query planning. Set a utility model in **Settings > AI > Downloaded** to turn on AI query planning.

## See also

- [Troubleshooting](troubleshooting.md)
- [FAQ](faq.md)
