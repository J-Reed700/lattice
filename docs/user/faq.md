# Frequently Asked Questions

Short answers about Lattice. For step-by-step fixes, see [Troubleshooting](troubleshooting.md).

## Table of Contents

- [General](#general)
- [Privacy and security](#privacy-and-security)
- [Features](#features)
- [Backups, export and sync](#backups-export-and-sync)
- [Limits and performance](#limits-and-performance)
- [Technical](#technical)
- [Getting help](#getting-help)

---

## General

### What is Lattice?

Lattice is a local-first desktop knowledge base. You import files and web pages. Lattice indexes them on your computer, so you can search them by keyword and by meaning and ask questions in Chat. Every answer cites the passages it came from.

Around that core are:

- a Journal for notes and daily pages
- References for sources you collect
- Explorer, for chatting about a folder on disk such as a code project
- Studio, for learning programs and flashcards

### What file types can I import?

- **Documents:** `.txt`, `.md`, `.markdown`, `.pdf`, `.docx`, `.rtf`, `.odt`, `.xlsx`, `.pptx`
- **Web pages:** `.html`, `.htm`, or a URL (one at a time, or a list) from the Import page
- **Data and config:** `.csv`, `.tsv`, `.json`, `.xml`, `.yaml`, `.yml`, `.toml`, `.ini`, `.conf`, `.config`, `.sql`, `.graphql`, `.gql`
- **Code:**
  - Rust, JavaScript and TypeScript (including `.jsx` and `.tsx`)
  - CSS, Sass and Less
  - Python, C and C++, Go
  - Java, Kotlin, Scala, Clojure
  - Ruby, PHP, Swift, R, Objective-C, Elixir, Erlang
  - shell scripts (`.sh`, `.bash`, `.zsh`, `.fish`, `.ps1`, `.bat`, `.cmd`)
- **Audio:** `.mp3`, `.wav`, `.ogg`, `.flac`, `.aac`, `.m4a`, `.wma`. Audio is transcribed on your computer, and the transcript becomes a searchable document. This needs a transcription model from the model catalog.

Not supported: images, older Office formats (`.doc`, `.xls`, `.ppt`) and e-books. Lattice has no OCR, so pages of a scanned PDF that are only images are skipped.

### Is it free?

Yes. Lattice is released under the MIT License (see the `LICENSE` file). You can use it, change it and redistribute it under that license.

### Does it work offline?

Yes, once your models are downloaded. Importing, indexing, search, local chat, notes and backups all run on your computer. See [What leaves my computer?](#what-leaves-my-computer) for the things that do use the network.

### Which systems does it run on?

Windows 11 (x64), macOS 13.3 or later on Apple Silicon, and Ubuntu 24.04 LTS (x64). An Intel Mac build is being qualified and runs models on the CPU. See [platform support](../development/platform-support.md).

### How much disk space and memory does it need?

Most of the space goes to models. On first run, Lattice suggests a chat model and an embedding model that fit your machine's memory and graphics hardware. It shows the total download size and checks that you have the space before downloading. Expect several GB. Your library adds a copy of each imported file plus its search index.

---

## Privacy and security

### Is my data private?

Your documents, notes, conversations and indexes are stored on your computer. There's no account and no telemetry. Local models run on your computer.

### What leaves my computer?

Only these things, and only when you use them:

- **Model downloads** from Hugging Face. If you save a Hugging Face token, it's sent to Hugging Face with your downloads.
- **URL imports.** Lattice fetches the page you asked for.
- **Web search and Wikipedia** in Chat. These run only when you turn them on for a turn (`/web`, `/wiki`, or their chips). Web search queries go to DuckDuckGo and the pages it finds. Wikipedia queries go to wikipedia.org.
- **Cloud models.** If you set the chat provider to OpenAI or Anthropic, your prompts and the passages retrieved from your library are sent to that provider.
- **Your own servers.** A llama.cpp or Ollama server you point Lattice at, and any custom tools you add in **Settings > AI > Tools**.
- **Update checks**, only when you click **Check for updates**.

**Settings > General > Privacy** has switches for anonymous usage statistics and crash reports. Both are off by default, and this version doesn't send either. Crash reports are only written to the `crashes/` folder in Lattice's app data.

### Where is my data stored?

In Lattice's app data folder: `~/Library/Application Support/tech.lattice.app/` on macOS, `%APPDATA%\tech.lattice.app\` on Windows, and `~/.local/share/tech.lattice.app/` on Linux. A few things live elsewhere:

- Downloaded models are in `~/.cache/lattice/models/`.
- Lattice's copies of imported files are in `~/.lattice/files/`.
- Log files are in a separate `lattice/logs` folder.

The full table is in [Troubleshooting](troubleshooting.md#where-lattice-keeps-things).

### Is the database encrypted?

The local database isn't encrypted. It's protected by your user account's file permissions. To protect it at rest, turn on full-disk encryption (FileVault, BitLocker or LUKS).

Off-device backup archives *are* encrypted, and API keys for cloud providers are kept in your system keychain, not in a file.

### Is it safe to import sensitive documents?

Imported documents are processed on your computer and stay there, unless you choose a cloud chat provider or turn on web search. Even then, only a turn's question and the passages chosen for it are sent, not your library. Use full-disk encryption, and keep your backups somewhere you trust.

---

## Features

### How does search work?

The Search page (⌘1) has three modes:

- **Keyword** matches the words you type.
- **Semantic** matches meaning, using an embedding model on your computer.
- **Hybrid**, the default, blends the two.

Hybrid finds a document about "the offsite" even when you search for "team retreat", and still ranks exact matches well.

### Can Chat search the web?

Yes, when you ask it to:

- `/web` lets an answer search the web.
- `/wiki` searches Wikipedia.
- `/deep` turns on deep research, which runs several rounds of searching and reading. It's slower.
- `/docs` makes every answer search your documents.

### Does a chat remember what I told it earlier?

Partly, and it's worth knowing exactly how much.

A long conversation eventually won't fit in the model's context window, so older messages stop being sent. To keep requirements from getting lost that way, Lattice reads the older part of the conversation and records the constraints, decisions, goals and preferences it finds. It stores each one with the exact quotation from your message that it came from, and adds the required items to every later prompt in that conversation. This is on by default. The switch is **Remember requirements in a conversation** in **Settings > AI > Chat**.

This doesn't remember everything you said. Finding those items is itself a model step, and it can miss things or misread them. What Lattice does promise:

- Anything it recorded can be traced to words you actually wrote.
- It won't quietly drop a requirement it already recorded.

To see every item, the quotation behind it, and any items it's unsure about, choose **Show conversation memory** from the command palette (⌘K).

Your original messages are never changed, deleted or rewritten. They stay in the conversation, stay searchable, and the chat can look back through them when a question needs it. If you edit or delete a message, anything recorded from it stops being quoted.

`/compact` folds older messages into a summary on demand. **Continue in new chat** in a conversation's menu starts a fresh conversation from a summary of the current one.

### Can I keep work and personal material apart?

Yes, with spaces. Create them in **Settings > General > Spaces**. A conversation in a space searches only the documents in that space.

### Can I chat about a folder of code?

Yes, in Explorer (⌘6). Choose a folder and it stays open as the scope until you close it. On the left you browse the files in a read-only viewer. On the right is a chat that can list, read and search the files in that folder, and that points at the exact lines it's talking about. Click a reference to open the file at those lines.

Explorer never changes your files. Its search index lives in Lattice's app data, not in the folder.

### Can I use my own models?

Yes:

- Download models from the catalog in **Settings > AI > Models**.
- Point Lattice at folders of `.gguf` files under **External model folders** on the same page.
- Set the chat provider to a llama.cpp server or Ollama you run yourself.
- Use OpenAI or Anthropic with your own API key.

Each downloaded model can take one or more roles in **Settings > AI > Downloaded**: Chat, Utility (a small fast model for query planning and routing) or Embedding.

### Does it support OCR?

No. Pages of a scanned PDF that are only images are skipped, and the rest of the document is imported. Run scans through an OCR tool before importing them.

### Can I get my notes as plain files?

Yes. Turn on **Mirror notes to disk** in **Settings > General > Vault**, and Lattice writes your notes as Markdown files in `~/Lattice` (or a folder you choose). **Watch for external edits** brings changes you make in other editors back into Lattice.

### Does it support languages other than English?

Lattice stores and displays any Unicode text. How well searching by meaning works depends on the embedding model:

- **Qwen3 Embedding 0.6B**, the default on machines with a supported GPU, is multilingual.
- **all-MiniLM-L6-v2**, the default otherwise, is mainly English.

Keyword search handles English word forms (for example "running" matches "run"). Other languages are matched on the words as written.

---

## Backups, export and sync

### Does it sync across devices?

No. Don't put Lattice's data folder inside Dropbox, iCloud Drive, OneDrive or Google Drive. Sync apps copy the database while Lattice is writing to it, which can corrupt it. If the data folder is inside a synced folder, Lattice warns you in **Settings > General > Vault**. To keep a copy off your computer, use off-device backup.

### How do I protect my library if my drive fails?

Turn on off-device backup in **Settings > General > Vault**:

1. Click **Set up encrypted backup**.
2. Write down the 24-word recovery code Lattice shows you, and confirm three of the words. The recovery code is the only way back in if you forget your passphrase. Lattice can't reset it.
3. Optionally set a passphrase.
4. Choose a folder: one your cloud storage app syncs, a NAS, or a USB drive.

Lattice then writes one encrypted `.lattice-backup` file about once a day while it's running, into a `Lattice Backups` folder inside the folder you chose. **Back up now** writes one immediately. It keeps the newest five archives, and you can change that with **Archives to keep**.

Each archive contains:

- your database: notes, journal, conversations, references, flashcards and Studio work
- the files you imported
- your mirrored notes folder
- your settings

Search indexes are left out and rebuilt after a restore.

**To restore**, choose **Restore from file** and enter the archive's passphrase or recovery code. On a new computer, set up off-device backup first: **Restore from file** appears once it's set up. If the archive is still only in the cloud, open it in your sync app so it downloads, then retry.

The **Backups** list on the same page is different. **Back up now** there saves a copy of the database only, inside Lattice's app data folder. It protects against mistakes, not against losing the drive.

### Can I export my data?

- **Conversations and journals:** Markdown or JSON, from **Export** in **Settings > General > Vault**. Files go to the `exports/` folder in Lattice's app data.
- **One conversation:** **Copy as Markdown** in its menu.
- **Notes as files:** turn on **Mirror notes to disk** (see above).
- **Settings:** **Export** and **Import** at the bottom of the Settings sidebar save and load a JSON file.
- **Everything:** an off-device backup archive.

### Can I import from Evernote, Notion, OneNote or Obsidian?

There are no direct importers. Export from the other app to Markdown, HTML, PDF or Word (`.docx`), then import the files on the Import page (⌘I) or with **Add folder** in the command palette (⌘K).

---

## Limits and performance

### What's the largest file I can import?

50 MB per file. Larger files are rejected. Split them or save a smaller copy.

### How many documents can it handle?

There's no fixed limit. Indexing time and disk use grow with your library.

### Why is chat slow on my computer?

Local models are fastest on a GPU: Metal on Apple Silicon, Vulkan on Windows and Linux. Without one, Lattice runs the model on the CPU, which is much slower. Use a smaller chat model, or run a llama.cpp server on a faster machine and point Lattice at it. See [Chat is slow](troubleshooting.md#chat-is-slow).

### Does Lattice watch folders for new files?

Not in this version. The **Watched folders** list in **Settings > General > Indexing** doesn't scan folders or pick up new files. Import new files yourself. Explorer is different: while a folder is open there, its index is updated as its files change.

---

## Technical

### What's it built with?

Tauri 2, with a Rust backend and a React and TypeScript interface. Data is stored in SQLite (`lattice.db`), with SQLite full-text search for keywords and a USearch vector index for meaning.

### Which AI components does it use?

- **Chat:** GGUF models run by a llama.cpp `llama-server` engine that ships inside Lattice. The engine runs on your computer and is reachable only from it. It can also use a llama.cpp or Ollama server, OpenAI or Anthropic.
- **Embeddings:** Qwen3 Embedding 0.6B or all-MiniLM-L6-v2, running inside Lattice.
- **Optional:** a reranker model that rescores search results, and Whisper models for transcribing audio.

### Do I need to install Python, Ollama or anything else?

No. Everything Lattice needs ships with it. Ollama and external llama.cpp servers are optional.

### Is there an API, CLI or browser extension?

No. Lattice has no command-line tool, no REST API, no plugin system and no browser extension. Chat can call HTTP endpoints you add as custom tools in **Settings > AI > Tools**. See [Advanced Features](advanced-features.md#custom-tools).

---

## Getting help

- [Troubleshooting](troubleshooting.md): fixes for common problems, and how to report a bug
- [Error Messages and Codes](error-codes.md): what an error code means
- [User Manual](user-manual.md): how each part of Lattice works

Lattice keeps a log of recent errors in **Settings > General > Logs**, and you can export it to attach to a bug report.
