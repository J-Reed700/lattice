# Troubleshooting

How to fix common problems with Lattice. For what a specific error code means, see [Error Messages and Codes](error-codes.md).

## Table of Contents

- [Where Lattice keeps things](#where-lattice-keeps-things)
- [Installing and opening](#installing-and-opening)
- [Lattice won't start](#lattice-wont-start)
- [Model downloads fail](#model-downloads-fail)
- [Chat doesn't answer, or the model won't load](#chat-doesnt-answer-or-the-model-wont-load)
- [Chat is slow](#chat-is-slow)
- [Answers ignore my documents, or search finds nothing](#answers-ignore-my-documents-or-search-finds-nothing)
- [Import problems](#import-problems)
- [Explorer's folder index](#explorers-folder-index)
- [My notes don't appear as Markdown files](#my-notes-dont-appear-as-markdown-files)
- [Backup and restore problems](#backup-and-restore-problems)
- [Database problems](#database-problems)
- [Starting over](#starting-over)
- [Reporting a problem](#reporting-a-problem)

---

## Where Lattice keeps things

| What | macOS | Windows | Linux |
|------|-------|---------|-------|
| App data: the database (`lattice.db`), `settings.json`, search indexes, `backups/`, `exports/`, `folder-index/`, `crashes/` | `~/Library/Application Support/tech.lattice.app/` | `%APPDATA%\tech.lattice.app\` | `~/.local/share/tech.lattice.app/` |
| Downloaded models | `~/.cache/lattice/models/` | `%USERPROFILE%\.cache\lattice\models\` | `~/.cache/lattice/models/` |
| Copies of imported files | `~/.lattice/files/` | `%USERPROFILE%\.lattice\files\` | `~/.lattice/files/` |
| Log files (`lattice.log.<date>`, last 7 days) | `~/Library/Application Support/lattice/logs/` | `%LOCALAPPDATA%\lattice\logs\` | `~/.local/share/lattice/logs/` |
| Notes mirrored as Markdown (only if turned on) | `~/Lattice/` | `%USERPROFILE%\Lattice\` | `~/Lattice/` |

On macOS, `~/Library` is hidden in Finder. To get there, choose **Go > Go to Folder…** and paste the path.

When you import a file, Lattice copies it into its own library. Your original file is never changed or moved.

---

## Installing and opening

**Supported systems:** Windows 11 (x64), macOS 13.3 or later on Apple Silicon, and Ubuntu 24.04 LTS (x64). An Intel Mac build is still being qualified. When it's available it's a separate download, because the Apple Silicon build won't run on an Intel Mac. See [platform support](../development/platform-support.md).

**macOS says the app can't be opened.** In Finder, Control-click Lattice and choose **Open**. Or try to open it once, then go to **System Settings > Privacy & Security** and click **Open Anyway**.

**Windows SmartScreen warns about the installer.** Click **More info**, then **Run anyway**. Only do this if you got the installer from a source you trust.

**Opening Lattice again just shows the existing window.** That's expected. Only one copy of Lattice runs at a time, so a second launch brings the open window to the front.

---

## Lattice won't start

If Lattice can't start, it shows a dialog and quits.

**"Application Setup Failed"**: Lattice couldn't create its data folders.

- Free some disk space.
- Make sure your user account can write to the app data folder (see [the table above](#where-lattice-keeps-things)).
- Check whether antivirus software is blocking the folder.

**"Application Initialization Failed"** with "Failed to create database connection" or "Failed to initialize database schema":

1. Quit any other copy of Lattice that might be running, including one that's stuck. Check Activity Monitor or Task Manager.
2. Check that you have at least a few hundred MB of free disk space.
3. If you've been running an older build: Lattice is pre-release and doesn't convert databases from older builds. When the database layout changes in a way your existing data can't follow, the remedy is to start with a fresh database. See [Starting over](#starting-over).

---

## Model downloads fail

The first time Lattice opens, it suggests a chat model and an embedding model that fit your machine, and it shows the total download size first. Downloads come from Hugging Face.

- **Not enough space.** The setup dialog says "Not enough free disk" with the space needed. Models are stored in `~/.cache/lattice/models/`, so the space has to be free on the drive that holds your home folder.
- **The download stops or fails.** Check your connection and retry from **Settings > AI > Models**. If the error mentions a rate limit, Hugging Face is throttling downloads. Wait a minute and retry.
- **A model in the catalog needs a token.** Some models on Hugging Face are gated. Create a token on Hugging Face and paste it in the **Hugging Face** section of **Settings > AI > Models**.
- **You skipped setup.** Open **Settings > AI > Models**, download a chat model and an embedding model, then give each its role in **Settings > AI > Downloaded** (Set as Chat, Set as Embedding).

---

## Chat doesn't answer, or the model won't load

Start with the notice Lattice shows in the chat:

- **"No chat model yet."** Download a chat model in **Settings > AI > Models** and set it as Chat in **Settings > AI > Downloaded**.
- **"Chat model is downloading…"** Wait for the download to finish.
- **"Warming up the model…"** The local engine is loading the model. A large model can take a while on first use.
- **"The chat model didn't load."** Use **Open model settings** and try again. If it fails again, try a smaller model.

**Check the provider.** In **Settings > AI > Chat > Provider**:

- **Auto** tries the built-in engine, then a llama.cpp server, then Ollama.
- **Local only** uses only models running inside Lattice.
- **llama.cpp** or **Ollama** use a server you run yourself. Check the server URL, and use **Test llama.cpp connection** for a llama.cpp server.
- **OpenAI** or **Anthropic** need an API key, which Lattice stores in your system keychain.

**"Lattice's bundled llama-server can't run on this machine."** Lattice runs local models with a llama.cpp engine that ships with the app. The message ends with what to do:

- *"Update your graphics driver to get a Vulkan runtime"* (Windows and Linux). Until you do, Lattice runs models on the CPU, which works but is slower.
- *"…uses its compatibility build instead"*. Your CPU lacks an instruction set the faster build needs. Nothing to fix.
- *"Reinstall Lattice."* The engine file is damaged or is for the wrong kind of processor. Reinstall, and on a Mac check that you installed the build for your processor.

**The answer stops partway.** In **Settings > AI > Tuning**, **Stall timeout** is how many seconds a reply may go silent before Lattice treats it as dead. It doesn't cap total answer time. Raise it on slow hardware or with a slow remote server.

**The model runs out of memory, or falls back to the CPU.** Leave **Local model context window** in **Settings > AI > Tuning** on automatic. A window too large for your graphics memory pushes the model onto the CPU. Otherwise, choose a smaller model.

---

## Chat is slow

- **The model is running on the CPU.** This happens on Intel Macs and on Windows or Linux machines without a working Vulkan driver. Use a smaller chat model, or point Lattice at a faster machine running llama.cpp (**Settings > AI > Chat**, provider **llama.cpp**).
- **Deep research is on.** Deep research runs several rounds of searching and reading and is much slower than a normal answer. Turn it off with `/deep` or by removing its chip in the message box.
- **A big context.** Very long conversations and many attached documents make each turn slower. Use **Continue in new chat** from the conversation's menu to carry a summary into a fresh conversation, or `/compact` to fold older messages into a summary.

---

## Answers ignore my documents, or search finds nothing

- **Check the space.** A conversation in a space searches only the documents in that space. Move the document into the space, or ask from a conversation in the right space.
- **Make sure the document finished importing.** Recently added files need to finish indexing before they can be found. Check **Import > History** for failures.
- **Make sure an embedding model is active.** Searching by meaning needs one (**Settings > AI > Downloaded**, Set as Embedding). On the Search page, try **Keyword** mode to check whether the text is there at all.
- **Ask for your documents explicitly.** Type `/docs` or use the **Your documents** chip so every answer searches your library.
- **No utility model.** Without one, Lattice uses basic query planning. Setting a small model as Utility in **Settings > AI > Downloaded** improves how questions are turned into searches.

---

## Import problems

- **The file type isn't supported.** See [What file types can I import?](faq.md#what-file-types-can-i-import). Images aren't supported.
- **The file is over 50 MB.** Imports are limited to 50 MB per file. Split the document or save a smaller copy.
- **A scanned PDF has no searchable text.** Lattice has no OCR. Pages that are only images are skipped and the rest of the PDF is imported. Run the PDF through an OCR tool first.
- **A password-protected or damaged file fails.** Remove the password or re-save the file in its original app, then import again.
- **An audio file fails with "No transcription model is downloaded."** Audio is transcribed on your computer. Download a transcription model from **Settings > AI > Models**, then import again.
- **A web page won't import.** Some sites block automated readers or need you to sign in. Save the page as PDF or HTML and import the file instead.
- **Adding a folder to Settings > Indexing doesn't import anything.** In this version, the **Watched folders** list in **Settings > General > Indexing** doesn't scan the folders or pick up new files. To import a folder, use **Add folder** in the command palette (⌘K), or add the files on the **Files** tab of **Import** (⌘I).

---

## Explorer's folder index

When you open a folder in Explorer (⌘6), Lattice builds a search index of it so the folder's chat can find code and text by meaning. The index's status is shown beside the folder path.

- **No embedding model.** The index needs an active embedding model. Set one in **Settings > AI > Downloaded**. Explorer still works without it, and the chat finds things by text matching.
- **Not indexed or Too large.** Lattice won't index a drive's root folder, your whole home folder or its own data folder, and folders with more than 20,000 files aren't indexed either. Open a smaller sub-folder instead. The chat's text search still works across the whole folder.
- **Indexing is slow.** The pill shows the percentage and the time left. Indexing shares the GPU with the chat model, so it slows down while a reply is being written. Closing the folder pauses it; reopening resumes where it stopped.
- **The index failed.** Click the status and choose **Retry**. To start over for that folder, choose **Rebuild index**.
- **Files are missing from answers.** Files ignored by the folder's `.gitignore` and binary files aren't indexed or searched.

The index is stored in Lattice's app data (`folder-index/`), never in your folder. To free the space, use **Delete index** in the folder's **⋯** menu on Explorer's start screen; the next open builds it again.

---

## My notes don't appear as Markdown files

Writing notes out as Markdown files is off by default. Turn on **Mirror notes to disk** in **Settings > General > Vault**. The first time you turn it on, Lattice writes out your existing notes. The default folder is `~/Lattice`, and **Choose** picks a different one.

To have edits you make to those files in another app flow back into Lattice, turn on **Watch for external edits**. It takes effect the next time Lattice starts.

---

## Backup and restore problems

Backups are in **Settings > General > Vault**.

- **Restore… is greyed out.** Wait for indexing to finish, then restore.
- **After restoring a local backup, things look wrong.** Quit Lattice and open it again. It can't reconnect to a restored database without a restart.
- **"That folder is gone."** The off-device backup folder was moved or its drive is disconnected. Reconnect the drive, or choose another folder.
- **"wrong passphrase or recovery code".** Re-enter the passphrase, or use the 24-word recovery code instead. Lattice can't reset either one. If both are lost, the archives can't be opened.
- **"backup file has not been downloaded by …".** The archive exists only in the cloud. Open it in your sync app (iCloud Drive, Dropbox, OneDrive or Google Drive) so it downloads, then retry.
- **A warning says Lattice's database is inside a synced folder.** Sync apps copy a database while it's being written, which can corrupt it. Move Lattice's data folder out of the synced folder, and use off-device backup to keep a copy in the cloud instead.

---

## Database problems

Most `DATABASE_ERROR` messages go away after you quit and reopen Lattice. If one keeps coming back:

1. Make sure your disk isn't full.
2. If the data folder is inside a cloud-synced folder, move it out (see above).
3. If Lattice still can't read the database, restore a backup, or start over (below).

---

## Starting over

Use this when the database won't open, or after an update that can't use your old data. This deletes your notes, conversations, imported documents, flashcards and Studio work, unless you restore a backup afterwards.

1. Quit Lattice.
2. Open the app data folder (see [Where Lattice keeps things](#where-lattice-keeps-things)).
3. Move `lattice.db`, `lattice.db-wal` and `lattice.db-shm` out of the folder or to the Trash.
4. Open Lattice. It creates a new, empty database.

What survives:

- Your downloaded models (they're in `~/.cache/lattice/models/`). Don't delete that folder to fix a database problem.
- Your settings (`settings.json`).
- Backups in `backups/` and any off-device archives.
- Your original files, wherever they are on disk.

What doesn't: Lattice's copies of imported files in `~/.lattice/files/` are cleaned up the next time it starts, because nothing refers to them anymore.

To get your data back, restore an off-device backup archive with **Restore from file** in **Settings > General > Vault**. Archives include your imported files. A local **Back up now** copy holds only the database. **Restore from file** appears only once off-device backup is set up on this computer. If it isn't, click **Set up encrypted backup** first. When the restore asks, enter the passphrase or recovery code of the archive you are restoring.

To reset only your settings, use **Reset all** at the bottom of the Settings sidebar.

---

## Reporting a problem

Report a bug if Lattice crashes, loses data, or a problem survives the steps above. Include:

- **The Lattice version.** Find it in **Settings > General > Display**, under About.
- **Your operating system and version, and your processor** (Apple Silicon, Intel, AMD).
- **What you did, what you expected, and what happened.**
- **The app's diagnostics.** In **Settings > General > Logs**, use **Export logs**. Secrets are redacted, but read the file before sharing.
- **The engine log.** The newest `lattice.log.<date>` from the logs folder (see [the table above](#where-lattice-keeps-things)). It can contain file paths, URLs and search queries, so check it before sharing.
- **Crash reports**, if any, from the `crashes/` folder in the app data folder.

## See also

- [FAQ](faq.md)
- [Error Messages and Codes](error-codes.md)
- [User Manual](user-manual.md)
