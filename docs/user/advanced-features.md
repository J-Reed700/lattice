# Advanced Features

This guide covers the settings and controls that go beyond everyday use: tuning retrieval, steering a chat turn, choosing where models run, adding your own tools, and working with Explorer. For the basics, start with the [User Manual](user-manual.md).

## Table of Contents

- [Retrieval settings](#retrieval-settings)
- [Steering a chat turn](#steering-a-chat-turn)
- [Where models run](#where-models-run)
- [Model tuning](#model-tuning)
- [Custom tools](#custom-tools)
- [Explorer in depth](#explorer-in-depth)
- [Notes as Markdown files](#notes-as-markdown-files)
- [Settings files](#settings-files)
- [Environment variables](#environment-variables)
- [Logs](#logs)
- [What Lattice doesn't have](#what-lattice-doesnt-have)

---

## Retrieval settings

Lattice searches your library in two ways at once:

- **Keyword search** uses SQLite full-text search with English stemming.
- **Search by meaning** uses vectors from your embedding model.

On the Search page (⌘1) you can switch between **Hybrid** (the default), **Semantic** and **Keyword**.

**Settings > General > Search** controls how passages are found for answers:

| Setting | What it does |
|---------|--------------|
| **Rerank results** | A cross-encoder model rescores the shortlist before the answer is written. It needs a one-time model download, and the hint under the switch says whether the reranker is ready. |
| **Corrective retry** | If the first search looks too thin to answer from, Lattice searches again before answering. |
| **Vector storage** | **Full precision**, or **Matryoshka truncation** to store shorter vectors. Truncation only works with an embedding model trained for it (Qwen3 Embedding) and is ignored otherwise. **Stored dimensions** and **Stored precision** (32-bit float or 8-bit integer) trade some accuracy for a smaller index. |
| **Document summaries** | Summarizes each document and section with the utility model, so questions about a whole document start in the right place. Needs a utility model, and takes effect after a restart. |
| **Embedding strategy** | **Chunk first** or **Late chunking**. Late chunking embeds each passage with its whole section as context. Changing it re-embeds your library. |
| **Similarity threshold** | The minimum score (0 to 1) a passage needs to count as a match. |

**Advanced retrieval tuning**, at the bottom of the page, exposes the numbers behind candidate shortlists, web and Wikipedia results, deep research depth, reranking and corrective retrieval. Leave them at their defaults unless you're measuring the effect of a change.

---

## Steering a chat turn

Type `/` in the message box to see the commands:

| Command | Effect |
|---------|--------|
| `/docs` | Search your documents for every answer. |
| `/web` | Let the answer search the web (via DuckDuckGo). |
| `/wiki` | Search and summarize Wikipedia. |
| `/deep` | Deep research: several rounds of searching and reading across sources. Slower. |
| `/auto` | Let the router decide whether a message starts a new topic or follows up on the last one. |
| `/followup` | Keep every turn on the current topic. |
| `/query` | Always search sources before answering. |
| `/compact` | Fold the older messages into a summary. |

The modes a turn will use appear as chips in the message box. Remove a chip to turn that mode off. Custom tools you've added (see [Custom tools](#custom-tools)) can be switched on the same way.

Type `@` to pick a document from the current space. The turn then focuses on that document.

Other chat controls:

- **Verify responses** (**Settings > AI > Prompts**) marks verified and unverified claims in assistant messages.
- **Remember requirements in a conversation** (**Settings > AI > Chat**, on by default) records the constraints and decisions you state, each with its source quotation, and keeps them in later prompts. **Show conversation memory** in the command palette (⌘K) lists them. See the [FAQ](faq.md#does-a-chat-remember-what-i-told-it-earlier) for what it does and doesn't promise.
- **The conversation menu** offers **Copy as Markdown**, **Save to journal**, **Synthesize to journal**, **Create flashcards** and **Continue in new chat**. Continue in new chat starts a fresh conversation from a summary.
- **Synthesis progress** shows the current stage and elapsed time. Choose **Keep working** to minimize it; it stays available when you switch screens. When saving finishes, choose **Open journal page**. If saving fails, **Retry saving** reuses the finished synthesis.
- **Spaces** scope a conversation. A conversation in a space searches only the documents in that space.

---

## Where models run

Lattice uses up to three models, each with a role you assign in **Settings > AI > Downloaded**:

- **Chat** writes the answers.
- **Utility** is a small, fast model used for query planning, follow-up routing and document summaries. If you don't set one, Lattice falls back to basic query planning.
- **Embedding** turns text into vectors for searching by meaning.

**Settings > AI > Chat > Provider** chooses where the chat model runs:

| Provider | Where it runs |
|----------|---------------|
| **Auto** | The built-in engine, then a llama.cpp server, then Ollama, whichever is available first. |
| **Local only** | Only the engine built into Lattice. |
| **llama.cpp** | A `llama-server` you run, on this machine or another. Set its URL, model and an optional auth header (name and value), then use **Test llama.cpp connection**. |
| **Ollama** | An Ollama server, by URL, with an optional auth header or basic auth. |
| **OpenAI**, **Anthropic** | The provider's API, with your API key (stored in your system keychain). Your prompts and the retrieved passages are sent to the provider. |

**The built-in engine** is a llama.cpp `llama-server` that ships inside Lattice and runs GGUF models on your computer. It's reachable only from your computer. It uses Metal on Apple Silicon. On Windows and Linux it uses Vulkan, and falls back to a CPU build if the GPU build can't run. If neither can run, Lattice says why, and what to do, in its error message.

**Getting models:**

- **Settings > AI > Models** has the model catalog.
- Open **Versions** on a chat model to compare its published GGUF files, exact
  download sizes, and estimated memory use. Select a version before downloading.
  Q4, Q5, Q8, IQ, and floating-point labels describe weight precision; they are
  not speed or accuracy scores. The picker currently supports standalone GGUF
  files at the repository root, excluding split files and vision projectors.
- Filter the catalog by category, listed precision, size, capability, popularity,
  or estimated fit. These filters describe the listed version; the version picker
  shows alternatives. Memory estimates do not include every context or runtime
  configuration, and unknown sizes are shown as unknown.
- **External model folders** on the same page let you add folders of `.gguf` (and `.onnx`) files you already have.
- Some models on Hugging Face are gated. Save a token in the **Hugging Face** section of the same page to download them.
- Downloads are stored in `~/.cache/lattice/models/`.

---

## Model tuning

**Settings > AI > Tuning** groups the generation settings:

- **Model runtime:**
  - temperature, top K, top P, repeat penalty, max tokens and streaming
  - **Context window**, and **Local model context window** for the built-in engine. Leave the local window on automatic: it's sized from what the model was trained for and how much GPU memory you have. A window that's too large for the card pushes the model onto the CPU.
  - **Stall timeout**: how long a reply may go silent before Lattice treats it as dead. It doesn't limit total answer time.
- **Router:** whether to route follow-up questions, which model routes them, the ambiguity threshold, and the prompts used.
- **Tool output:** how many results tools return and how much text they include.

---

## Custom tools

**Settings > AI > Tools** lists the built-in tools (`web_search`, `fetch_url_content`, `wiki_search`, `wiki_summary`) and lets you add your own.

A custom tool is an HTTP endpoint that Lattice calls with a GET request. Each tool has:

- a **Name** (letters, numbers and underscores only)
- a **Description**, which tells the model when to use it
- an **Endpoint**
- the **Query param** that carries the search text
- an optional **Max results param** and **Default max results**

Use **Test** to send the **Test query** and see what comes back, then **Save tools**.

**Presets** fill in a tool for you: SearXNG (self-hosted), OpenAlex, Crossref, Europe PMC, Open Library and Semantic Scholar. Pick one and click **Add as tool**. **Import** and **Export** move your tool list between machines.

Once saved, a tool can be switched on for a turn from the chat's controls. Queries sent to a custom tool go to that tool's server.

---

## Explorer in depth

Explorer (⌘6) puts a folder from disk beside a chat.

**The folder is locked once you pick it.** Choose a folder from the start screen and it becomes the scope. Browsing the tree only moves around inside it. **Close folder** returns you to the start screen, where **Your folders** lists every folder you've opened until you remove it.

**The left side is read-only.** You browse the tree and read files with syntax highlighting for about 45 languages, fold blocks from the gutter, and find text in the open file with ⌘F. Explorer never edits, moves or deletes anything in the folder.

**The chat reads the folder.** Explorer's chat can:

- list directories
- read files
- search file contents
- find files by name

It stays inside the folder, and it skips anything the folder's `.gitignore` excludes. The file you have open and the lines you've selected are sent with each message, so even a model without tool calling can answer about what's on screen. Your library, web and Wikipedia work the same way as in Chat.

**Answers point at lines.** When the chat refers to code it writes references like `src/main.rs:10-24`. Click one to open the file with those lines highlighted.

**Threads belong to their folder.** Conversations started in Explorer appear in Explorer's thread list for that folder, and not in Chat's sidebar.

**The folder index.** When you open a folder, Lattice also builds a search index of it, so the chat can find code by meaning ("where do we handle retries?") as well as by text.

- It needs an active embedding model.
- It skips `.gitignore`d and binary files.
- It stays up to date while the folder is open.
- It's stored in Lattice's app data under `folder-index/`, outside both your folder and `lattice.db`, and isn't part of backups.
- The pill beside the folder path shows the percentage done and the time left; its panel gives the passage and file counts, the rate and **Rebuild index**, and **Retry** appears if indexing failed. A line above the chat composer shows while it builds.
- A folder inside one that already has an index uses that index, limited to the sub-folder.
- Lattice won't index a drive's root folder, your whole home folder or Lattice's own data folder, and folders with more than 20,000 files aren't indexed. Explorer still opens them, and the chat falls back to text search.
- On the start screen, each folder's **⋯** menu has **Delete index** (the next open builds it again) and **Remove…**, which also asks whether to delete the folder's threads. Lattice never deletes an index on its own.

---

## Notes as Markdown files

**Settings > General > Vault** can mirror your notes to Markdown files on disk:

- **Mirror notes to disk** writes every note as a `.md` file in the vault folder. The default is `~/Lattice`, and **Choose** picks another folder. The first time you turn it on, Lattice writes out your existing notes. Mirrored journal pages are also indexed, so chat can retrieve them.
- **Watch for external edits** imports changes you make to those files in another editor. It takes effect after a restart.

Both are off by default.

---

## Settings files

At the bottom of the Settings sidebar:

- **Export** saves your settings to `lattice-settings.json`.
- **Import** loads a settings file.
- **Reset all** restores every setting to its default.

Your data isn't touched by any of these. API keys live in the system keychain and aren't part of the file.

Backups and data export are covered in the [FAQ](faq.md#backups-export-and-sync).

---

## Environment variables

Set these before starting Lattice, for example by launching it from a terminal.

| Variable | Effect |
|----------|--------|
| `LATTICE_FORCE_CPU=1` | Runs the embedding model, the reranker and audio transcription on the CPU instead of the GPU. Use it if those crash or misbehave on your graphics hardware. It doesn't affect the chat engine. |
| `RUST_LOG` | Sets how much goes into the log files, for example `RUST_LOG=debug`. The default for a release build is `info`. |

---

## Logs

There are two logs:

- **Settings > General > Logs** shows the newest 300 errors, warnings and events from the interface. You can filter them by severity, search them, export them as JSON, and clear them. Common secrets are redacted.
- **The engine log** is written by Lattice's backend to `lattice.log.<date>` files, one per day, with the last seven days kept. On macOS they're in `~/Library/Application Support/lattice/logs/`, on Windows in `%LOCALAPPDATA%\lattice\logs\`, and on Linux in `~/.local/share/lattice/logs/`. These files can contain file paths, URLs you opened and search queries.

---

## What Lattice doesn't have

Lattice doesn't have:

- a command-line tool
- a REST API or plugin system
- a browser extension
- OCR
- cloud sync

It doesn't scan or watch folders for new files either. The **Watched folders** list in **Settings > General > Indexing** has no effect on importing in this version. Use **Import** (⌘I) or **Add folder** in the command palette instead.
