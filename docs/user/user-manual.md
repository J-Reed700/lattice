# Lattice User Manual

A guide to each part of Lattice. If you haven't installed it yet, start with
[Getting Started](getting-started.md).

## Table of Contents

1. [Key Ideas](#key-ideas)
2. [Getting Around](#getting-around)
3. [Keyboard Shortcuts](#keyboard-shortcuts)
4. [Importing](#importing)
5. [Home](#home)
6. [Search](#search)
7. [Library](#library)
8. [Chat](#chat)
9. [Journal](#journal)
10. [References](#references)
11. [Explorer](#explorer)
12. [Studio](#studio)
13. [Compare](#compare)
14. [Settings](#settings)
15. [Backup, Restore and Export](#backup-restore-and-export)
16. [Where Your Data Lives](#where-your-data-lives)
17. [Getting More Help](#getting-more-help)

---

## Key Ideas

- **Library**: the documents and web pages you've imported. Lattice keeps its
  own copy of each imported file, so moving or deleting the original doesn't
  break anything.
- **Index**: when you import something, Lattice extracts its text, splits it
  into passages, and stores both the words (for keyword search) and an
  embedding of each passage (for search by meaning).
- **Models**: a chat model writes answers and an embedding model powers search
  by meaning. Both run on your computer unless you choose a remote provider.
- **Spaces**: groups of related conversations. Documents can be imported
  straight into a space.
- **Citations**: chat answers, flashcards and comparison tables point back to
  the passages they came from.

---

## Getting Around

The rail on the left of the window holds the main surfaces:

| Surface | Shortcut | What it's for |
| --- | --- | --- |
| Home | ⌘0 | What's in your library, today's journal, and conversations to pick back up |
| Search | ⌘1 | Find a document by keyword or meaning |
| Library | ⌘2 | Browse, organise and manage everything you've imported |
| Journal | ⌘3 | Dated pages for notes, captures and syntheses |
| Chat | ⌘4 | Ask questions and get answers with citations |
| References | ⌘5 | Passages you've saved |
| Explorer | ⌘6 | A folder on disk next to a chat that can read it |
| Studio | ⌘7 | Learning programs and flashcards |
| Import | ⌘I | Add files and web pages |
| Settings | ⌘, | Models, search, backups and everything else |

On Windows and Linux, use **Ctrl** wherever this manual says **⌘**.

**Command palette (⌘K).** Type to jump to a document, a recent search or any
surface, start a new conversation or journal entry, add files, add a folder or
a web page, or open the list of keyboard shortcuts.

**Quick capture (⌘⇧N).** Type a thought and press **Enter**; it is added to
today's page in your journal. If your clipboard holds a link and you haven't
typed anything, Quick capture offers to import that page instead.

---

## Keyboard Shortcuts

| Shortcut | Action |
| --- | --- |
| ⌘K | Command palette |
| ⌘N | New conversation or journal entry |
| ⌘⇧N | Quick capture |
| ⌘, | Settings |
| Esc | Close the open dialog or panel |
| ⌘0 to ⌘7 | Home, Search, Library, Journal, Chat, References, Explorer, Studio |
| ⌘I | Import |
| Enter | Send a chat message |
| Shift+Enter | New line in a chat message |
| ⌘⇧K | Find in conversations and references (in Chat) |
| ⌘\ | Hide or show the sidebar (Chat, Journal, References) |
| ⌘F | Find in the open file (in Explorer) |
| ⌘[ / ⌘] | Back to the previous file / forward to the next (in Explorer) |

The same list is available from the command palette under **Keyboard
shortcuts**.

---

## Importing

Open **Import** with **⌘I**. It has four tabs:

- **URL**: import one web page.
- **URLs**: import several pages, one per line.
- **Files**: drop files onto the page or click **choose files**.
- **History**: past imports, including any that failed and why.

Imports can go straight into a **space** or a **collection**. For a manual or
book split across several files or pages, you can give them a shared source
title and edition so they're treated as one source.

Other ways in:

- **Add folder** in the command palette (⌘K) imports every supported file in a
  folder and its sub-folders.
- Drop files onto a chat to attach them to that conversation.
- Quick capture can import a link from your clipboard.

Importing needs an embedding model. If you see "embedding model is not ready",
install one in **Settings > AI > Models** and choose **Set as Embedding** on it
under **Settings > AI > Downloaded**.

### Supported file types

- **Documents**: PDF, Word (`.docx`), OpenDocument text (`.odt`), RTF, Excel
  (`.xlsx`), PowerPoint (`.pptx`), plain text, Markdown
- **Web**: HTML files, and web pages imported by URL
- **Data**: CSV, TSV, JSON, XML, YAML, INI and config files, SQL, GraphQL
- **Code**: most common languages, including Python, JavaScript, TypeScript,
  Rust, Go, C and C++, Java, Kotlin, Scala, Swift, Ruby, PHP, R, Elixir, Erlang,
  Clojure, CSS and shell scripts

Images, audio, video, archives and executables can't be imported. Lattice
doesn't run OCR, so a scanned PDF needs a text layer before its text can be
searched.

---

## Home

Home (⌘0) is a summary of your library and your recent work: what's been
indexed recently, today's journal entry, what you've saved, and conversations
worth continuing. If Lattice can't work out a number, it leaves that tile out
rather than showing zero.

---

## Search

Search (⌘1) finds documents in your library. Pick a mode under the search box:

- **Hybrid** (default): exact keywords and meaning combined.
- **Semantic**: by meaning, so "notes about the offsite" can find a document
  that never uses the word "offsite".
- **Keyword**: exact words only.

Results update as you type. Open a result to read the document. Retrieval
options such as reranking, maximum results and the similarity threshold are in
**Settings > Search**.

---

## Library

Library (⌘2) is everything you've imported.

- **Views**: list, grid or tree. Sort by name, date or length, and filter by
  type or by where it came from (local files or the web).
- **Side rail**: collections, folders, saved searches, sources, and **Themes**,
  which groups your documents by topic.
- **Collections**: add documents with **Add to collection…** and manage them
  from the rail.
- **Document actions** (right-click a document): view it in Lattice, open it in
  your system viewer, show it in its folder, copy its path, ask about it in
  Chat, rename, reindex, add to or remove from a collection, remove it from the
  index, or delete it.
- **Related**: shows documents connected to the selected one, what's similar,
  and the conversations that cite it.
- **Select several** documents to delete them together or to
  [compare](#compare) them.

Deleting a document removes it and its passages from Lattice. Your original file
is never touched; Lattice only deletes its own copy once nothing uses it.

---

## Chat

Chat (⌘4) answers questions using your library, and can also use the web,
Wikipedia, or the model's own knowledge.

### Conversations and spaces

The sidebar lists your conversations. Filter them by **All**, **Starred**,
**Pinned**, **Archived** or **Referenced**. Open the **…** menu on a
conversation to:

- copy it as Markdown,
- save it to the journal,
- synthesize it into a journal entry, or
- continue it in a new chat that starts from a summary.

Choose a **space** to see only that space's conversations. Open the spaces
panel in the sidebar to create a space, give it an accent colour, or archive
it. You can also create spaces in **Settings > Spaces**.

### Asking

Type a question and press **Enter**. Choices for each turn sit under the
message box, and you can also type `/` to set them:

| Command | What it does |
| --- | --- |
| `/docs` | Search your documents for every answer |
| `/web` | Let the answer search the web |
| `/wiki` | Search and summarise Wikipedia |
| `/deep` | Deep research: multi-step research across sources (slower) |
| `/auto` | Let Lattice decide whether this is a new topic or a follow-up |
| `/followup` | Keep every turn on the current topic |
| `/query` | Always search sources before answering |
| `/compact` | Fold older messages into a summary |

Type `@` and part of a document's name to point the question at that document.
Drop files onto the chat to attach them to the conversation.

### Reading an answer

- Numbered citations link to the passages used. Hover to preview a passage;
  click to open the source.
- With **Verify responses** on (in **Settings > AI > Prompts**, on by default),
  Lattice checks claims against the passages they cite and marks which ones it
  could and couldn't verify.
- A turn record shows what was searched and which sources were pulled in.

### Message actions

On a message you can copy, edit and resend, regenerate, try the question with
another model, branch the conversation from that point, add it to your
references, add it to the journal, or delete it.

### Memory

With **Remember requirements in a conversation** on (the default, in
**Settings > AI > Chat**), Lattice records the constraints, decisions and goals
you state, with the words they came from, and keeps them in view for later
turns. The conversation's memory panel shows what it has kept.

---

## Journal

Journal (⌘3) holds pages of notes. You can keep more than one journal and switch
between them.

- Create a page with **New page**, or use the calendar to go to a date.
- Quick capture (⌘⇧N) adds to today's page, creating it if needed.
- The side panel shows the conversation, highlights and sources behind a page.
- **Synthesize the past week** or **Synthesize pinned entries** to have a summary
  written for you, with sources.
- **Ask in Chat, filed under this journal** starts a conversation linked to the
  journal.

To keep plain Markdown copies of your notes outside Lattice, turn on
**Mirror notes to disk** in **Settings > Vault**. Notes go to `~/Lattice` unless
you choose another folder. Turn on **Watch for external edits** to bring
changes made in another editor back into Lattice (takes effect after a
restart).

---

## References

References (⌘5) collects passages you've saved: answers and passages from Chat,
passages from documents, and captures. Filter by origin (**From documents**,
**From Chat**, **From Journal**, **Captured**), search them, add a note, copy
them, or jump back to where they came from.

Save a chat answer here with **Add to references** on the message.

---

## Explorer

Explorer (⌘6) puts a folder from your disk on the left and a chat on the right.

1. Click **Choose a folder…** and pick a folder, or open one from **Your
   folders** on the start screen.
2. Browse the tree and open files in a read-only viewer. It colours the syntax
   of about 45 languages (C and C++, Rust, TypeScript, Python, Markdown,
   CMake, shell and more), folds blocks from the gutter, and finds text in the
   open file with ⌘F. The ‹ › buttons above the file (or ⌘[ and ⌘]) step back
   and forward through the files you've opened, line links included.
3. Ask about the folder in the chat. The chat can list, read and search files
   inside the folder, and when it mentions lines (for example
   `src/main.rs:10-24`) they appear as links that open the file and highlight
   those lines.
4. Click line numbers in the viewer to send those lines with your next message.

The folder is locked once it's open: the chat can't read outside it. To work
somewhere else, choose **Close folder** and pick another one.

Each folder has its own threads, listed in Explorer's thread switcher. Explorer
threads don't appear in Chat's sidebar.

**Folder index.** When you open a folder, Lattice also builds a search index for
it in the background so the chat can find code by meaning, not only by exact
text. The pill beside the folder's path shows how far it has got and the time
left (for example "Indexing 28% · ~14 min left"); click it for the counts and
**Rebuild index**. While it builds, a line above the chat composer says so, and
search covers what is indexed so far. The index is kept separately from
your library (in the `folder-index` folder of your data folder) and isn't
included in backups. It follows changes to files while the folder is open, and
picks up where it left off when you reopen the folder. A folder inside one
that already has an index uses that index. Lattice doesn't index your home
folder itself, the root of your disk, or folders with more than 20,000 files;
Explorer still works in those, just without the index. Indexing needs an
embedding model.

**Your folders.** Every folder you open stays on the start screen until you
remove it. Each row shows the folder's index (Indexed, Paused at a percentage,
Not indexed, Too large, or Folder missing when it has moved), how many threads
it has, the index's size and when you last opened it. Pin a folder to keep it
at the top; the **⋯** menu has **Rename**, **Settings…**, **Delete index**
(frees the space; the next open builds it again) and **Remove…**, which deletes
the index and asks whether to delete the folder's threads too. Nothing in the
folder on disk is changed, and Lattice never deletes an index unless you ask.

**Folder settings.** Each folder has its own **system prompt** and **space**,
set from **Settings…** in its **⋯** menu or the sliders button beside **Close
folder**. The system prompt is used in every chat about the folder in place of
the space's prompt (leave it empty to use the space's). The space decides which
library documents the folder's chat can search when you turn library search on,
and which space memory it reads. Folders start in **General**; changing the
space moves the folder's existing threads with it. The folder's own search
index belongs to the folder, whatever space it's in.

---

## Studio

Studio (⌘7) builds learning programs from material you trust.

- **Programs.** Describe what you want to learn and what you already know,
  choose up to eight library documents or add reference URLs, and Studio drafts
  an outline for you to review before it prepares any lessons. Each program has
  lessons, practice, assessments, recall cards, a notebook and a canvas, and
  your progress is saved on this device.
- **Flashcards.** Make a deck from up to three documents, optionally with a
  topic and a learning goal. Lattice writes the questions from your documents,
  each with its source. Review them as flashcards; your results and next review
  dates are saved.

---

## Compare

Compare builds a table across several documents. Select two or more documents in
the Library and choose **Compare**, then name up to six columns (for example
"method, sample size, finding"). Each cell is answered from that document with a
citation, or marked "not stated" if the document doesn't say. You can save the
table to your journal.

---

## Settings

Open Settings with **⌘,**. The buttons at the bottom of the tab list
**Export** your settings to a JSON file, **Import** them from one, or
**Reset all** to defaults.

### General

- **Search**: retrieval options, including reranking, a corrective retry when
  the first search comes back thin, document summaries, how vectors are stored,
  maximum results, the similarity threshold, and the keyword/meaning balance.
- **Indexing**: indexing options and a list of folders and exclude patterns.
  Lattice doesn't watch these folders for new files; use Import or **Add
  folder** to bring files in.
- **Vault**: the Markdown mirror of your notes (see [Journal](#journal)), plus
  [backups and export](#backup-restore-and-export).
- **Spaces**: create spaces and see the ones you have.
- **Display**: light, dark or system theme, and the app version.
- **Privacy**: switches for anonymous usage statistics and crash reports. Both
  are off by default, and Lattice doesn't currently send either anywhere.
- **Logs**: recent log messages; search them, filter by severity, or clear them.

### AI

- **Chat**: the chat provider (**Auto**, **Local only**, **Ollama**,
  **llama.cpp**, **OpenAI** or **Anthropic**), connection details and API keys,
  the active chat, embedding and utility models, and conversation memory.
- **Models**: the model catalog, which shows how well each model fits your
  computer and downloads it; external folders where you keep your own GGUF
  models; and a Hugging Face token for gated downloads.
- **Downloaded**: installed models. **Set as Chat**, **Set as Embedding** or
  **Set as Utility** to put one to work, or delete it.
- **Prompts**: prompt templates and answer verification.
- **Tuning**: context window, maximum tokens, stall timeout, repeat penalty,
  follow-up routing and related options.
- **Tools**: the built-in tools a chat can use, plus your own search tools that
  call an HTTP endpoint.

---

## Backup, Restore and Export

All of this is in **Settings > Vault**.

### Backups on this computer

**Back up now** saves a copy of your database to the `backups` folder in your
data folder. Earlier backups are listed with **Restore**. Restoring replaces
everything indexed since that backup, and you need to quit and reopen Lattice
afterwards. These backups contain the database only, not your imported files or
models.

### Encrypted off-device backup

**Set up encrypted backup** writes an encrypted archive (`.lattice-backup`) to a
folder you choose, such as a folder synced by your cloud storage service. Setup
takes five steps:

1. Start setup.
2. Write down the 24-word recovery code.
3. Confirm you saved it.
4. Optionally add a passphrase as a second way in.
5. Choose the folder.

Each archive holds your database, imported files, mirrored notes and settings,
but not models. Once set up, Lattice writes an archive daily and when you press
**Back up now**, and deletes the oldest ones beyond **Archives to keep**.

To restore, choose **Restore from file**, pick an archive and unlock it with the
recovery code or passphrase, then quit and reopen Lattice. Keep the recovery
code safe: Lattice never stores it and can't replace it.

### Export

**Export conversations and journals** as Markdown files or a JSON file. They're
written to the `exports` folder in your data folder. To copy a single
conversation, use **Copy conversation as Markdown** from its menu in Chat.

---

## Where Your Data Lives

| What | macOS | Windows | Ubuntu |
| --- | --- | --- | --- |
| Data folder: database (`lattice.db`), settings, backups, exports, Explorer folder indexes | `~/Library/Application Support/tech.lattice.app/` | `%APPDATA%\tech.lattice.app\` | `~/.local/share/tech.lattice.app/` |
| Copies of imported files | `~/.lattice/files/` | `%USERPROFILE%\.lattice\files\` | `~/.lattice/files/` |
| Downloaded models | `~/.cache/lattice/models/` | `%USERPROFILE%\.cache\lattice\models\` | `~/.cache/lattice/models/` |
| Logs (one file per day, about a week kept) | `~/Library/Application Support/lattice/logs/` | `%LOCALAPPDATA%\lattice\logs\` | `~/.local/share/lattice/logs/` |
| Markdown mirror of notes (if turned on) | `~/Lattice/` | `%USERPROFILE%\Lattice\` | `~/Lattice/` |

Lattice is pre-release and doesn't migrate data from earlier pre-release builds.
If a new build can't open your database, quit Lattice, move `lattice.db` aside,
and start again; then restore what you need from an export or backup.

---

## Getting More Help

- [Getting Started](getting-started.md): installation and first launch
- [Advanced Features](advanced-features.md): deeper detail for power users
- [Troubleshooting](troubleshooting.md): fixes for common problems
- [FAQ](faq.md): short answers
- [Error Codes](error-codes.md): what error messages mean
- [GitHub Issues](https://github.com/J-Reed700/lattice/issues): report a problem
