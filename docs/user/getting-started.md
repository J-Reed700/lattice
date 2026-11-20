# Getting Started with Lattice

This guide covers installing Lattice on Windows, macOS, or Ubuntu, setting up AI (either with a cloud API key or local offline models), and starting your first course or library session.

## Table of Contents

1. [Supported Systems](#supported-systems)
2. [Installation](#installation)
3. [First Launch (Set up AI)](#first-launch)
4. [Start Your First Course (Learning Studio)](#start-your-first-course)
5. [Your First Library Session](#your-first-session)
6. [Where Lattice Keeps Your Data](#where-lattice-keeps-your-data)
7. [Uninstalling](#uninstalling)
8. [If Something Goes Wrong](#if-something-goes-wrong)
9. [Next Steps](#next-steps)

---

## Supported Systems

Lattice is built for:

- **Windows 10/11** (x64)
- **macOS 13.3 or later** on Apple Silicon. Intel Mac support is being qualified and runs local models on the CPU.
- **Ubuntu 24.04 LTS** (x64)

Other Linux distributions, ARM Windows, and ARM Linux are not supported with prebuilt installers.

**Disk space:** If you run local models, they take several gigabytes (the first-launch dialog shows the exact size before downloading). If you use a cloud API key (Claude, OpenAI, or Google), nothing extra is downloaded.

---

## Installation

### Option A: Prebuilt installer (easiest)

Open the [Releases](https://github.com/J-Reed700/lattice/releases) page and look for the latest **Lattice app release** (`Lattice_<version>_...` files — ignore `llama-server` sidecar releases).

#### Windows 10/11

1. Download and double-click `Lattice_<version>_x64-setup.exe`.
2. Pre-release builds are not code-signed yet, so Windows SmartScreen may warn you: click **More info**, then **Run anyway**.
3. Follow the installer prompts (no administrator rights required), then open **Lattice** from the Start menu.

#### macOS (Apple Silicon)

1. Download and open `Lattice_<version>_aarch64.dmg` and drag **Lattice** to **Applications**.
2. Open Lattice from Applications.
3. Pre-release builds are not notarized yet. If macOS says it can't verify the app, open **System Settings → Privacy & Security** and click **Open Anyway**.

#### Ubuntu 24.04

Install the `.deb` package, then open Lattice from your applications menu:

```bash
sudo apt install ./Lattice_*_amd64.deb
```

### Option B: Build from this repository

If no app installer release is published yet (or you cloned / downloaded this repository as a ZIP), run the setup script inside the repo folder. It installs any missing build tools, compiles Lattice, installs it, and opens it automatically (~10–15 minutes on first run):

- **Windows 10/11:** Double-click **`setup.cmd`** in the repo folder (or run `powershell -ExecutionPolicy Bypass -File .\setup.ps1` in PowerShell from the repo folder).
- **macOS / Linux:** Open **Terminal** in the repo folder and run:

  ```bash
  ./setup.sh
  ```

---

## First Launch

### Set up AI

When you open Lattice for the first time, a **Set up AI** window opens. You have two choices:

#### Option 1: Use an API key (Claude, OpenAI, or Google) — fastest

1. Click **Use an API key instead**.
2. Pick your **Provider** (**Anthropic** for Claude is selected by default).
3. Paste your API key and click **Save key** (stored in your computer's keychain).
4. Choose a **Model** from the dropdown and click **Use this model**.

Chats and Learning Studio courses now use that model with zero downloads.

> **Important if you want to import PDFs or files later:** Using an API key downloads nothing, so **chat and topic-based courses work right away**. If you later want to import PDFs, textbooks, or notes for semantic search or source-backed courses, open **Settings → AI → Models**, download an embedding model, and click **Set as Embedding** under **Settings → AI → Downloaded**.

#### Option 2: Download local offline models (no API key needed)

The dialog checks your computer's memory and recommends a **chat** model plus an **embedding** model (for searching your files by meaning), showing the total size before anything downloads.

- Click **Install** to download both in the background. You can keep using the app while they download, and both activate automatically when finished.
- Click **See all models** to pick different models in **Settings → AI → Models**.
- Click **Not now** to skip and set up models later under **Settings → AI**.

Once local models finish downloading, Lattice runs offline. It uses the network only when you import a web page, turn on web/Wikipedia search, research course references, download models, check for updates, or use a cloud/remote provider you configured.

---

## Start Your First Course

1. Open **Studio** from the left sidebar (**Ctrl+7** on Windows/Linux, **⌘7** on macOS).
2. Click **Build your first program** (or **Build a program** in the top right).
3. Fill in **Your goal** — describe what you want to be able to do (e.g., *"Understand SQL query plans and index design well enough to tune slow queries"*). Optionally describe what you already know.
4. Pick a depth (**Focused course**, **Complete course**, or **Deep dive**) and your session length (e.g., 30 minutes).
5. *(Optional)* Click **Add optional materials** to attach imported documents from your Library or paste reference URLs. If you leave this blank, Lattice searches for and verifies supporting web references automatically.
6. Click **Create outline**. Writing and reviewing the outline can take several minutes depending on your model and course depth — you can follow each step live.
7. Once the outline is ready, review it, approve it, and open **Lesson 1** to begin studying.

---

## Your First Session

### 1. Import some documents

Press **Ctrl+I** (**⌘I** on macOS) to open **Import**:

- **Files**: drop files onto the page, or click **choose files**.
- **URL** / **URLs**: import one web page, or several, one per line.
- **History**: see past imports and any that failed.

Lattice copies each file into its own library, extracts the text, and indexes it. To import a whole folder, open the command palette (**Ctrl+K** / **⌘K**) and choose **Add folder**.

PDF, Word (`.docx`), text, Markdown, audio, and many code and data formats are supported. See [Supported file types](user-manual.md#supported-file-types).

### 2. Search

Press **Ctrl+1** (**⌘1** on macOS) for **Search** and type what you remember. **Hybrid** (the default) combines exact keywords with meaning; **Semantic** and **Keyword** use one or the other. You can also press **Ctrl+K** (**⌘K**) from anywhere to jump to a document.

### 3. Ask a question

Press **Ctrl+4** (**⌘4** on macOS) for **Chat**, type a question, and press **Enter**. Answers cite the passages they used; hover a citation to preview the passage and click it to open the source.

### 4. Keep a thought

Press **Ctrl+Shift+N** (**⌘⇧N** on macOS) for **Quick capture**, type a note, and press **Enter**. It is added to today's page in your **Journal** (**Ctrl+3** / **⌘3**).

### 5. Look around

- **Home** (**Ctrl+0** / **⌘0**) shows today's journal, recent chats, and library stats.
- **Library** (**Ctrl+2** / **⌘2**) lists everything you've imported.
- **References** (**Ctrl+5** / **⌘5**) holds saved passages and bookmarked answers.
- **Explorer** (**Ctrl+6** / **⌘6**) opens a folder on disk next to a chat that can read it.
- **Studio** (**Ctrl+7** / **⌘7**) builds learning programs and holds your flashcards.

The [User Manual](user-manual.md) covers each of these in detail.

---

## Where Lattice Keeps Your Data

| What | Windows | macOS | Ubuntu |
| --- | --- | --- | --- |
| Database, settings, backups, exports, Explorer folder indexes | `%APPDATA%\tech.lattice.app\` | `~/Library/Application Support/tech.lattice.app/` | `~/.local/share/tech.lattice.app/` |
| Copies of imported files | `%USERPROFILE%\.lattice\files\` | `~/.lattice/files/` | `~/.lattice/files/` |
| Downloaded models | `%USERPROFILE%\.cache\lattice\models\` | `~/.cache/lattice/models/` | `~/.cache/lattice/models/` |
| Logs | `%LOCALAPPDATA%\lattice\logs\` | `~/Library/Application Support/lattice/logs/` | `~/.local/share/lattice/logs/` |

The database file is `lattice.db`. If you turn on **Mirror notes to disk** in **Settings → Vault**, your notes are also written as Markdown files to a folder you choose (`~/Lattice` by default).

---

## Uninstalling

1. Quit Lattice.
2. Remove the app:
   - **Windows:** open **Settings → Apps → Installed apps**, find **Lattice**, and click **Uninstall**.
   - **macOS:** drag **Lattice** from **Applications** to the Trash.
   - **Ubuntu:** run `sudo apt remove lattice`.
3. To remove your data as well, delete the folders listed in [Where Lattice Keeps Your Data](#where-lattice-keeps-your-data). Back up or export first if you want to keep anything.

---

## If Something Goes Wrong

- **The setup dialog says there isn't enough disk.** Free up space, use a cloud API key instead, or pick a smaller model in **Settings → AI → Models**.
- **A model download failed.** Retry it in **Settings → AI → Models**. Gated Hugging Face models need a token, which you can add in the **Hugging Face** section of the same page.
- **Chat or Studio says there's no model.** Open **Settings → AI → Chat** to check your cloud provider/model (or **Settings → AI → Downloaded** to click **Set as Chat** on a local model).
- **Import says the embedding model is not ready.** Install an embedding model in **Settings → AI → Models**, click **Set as Embedding** under **Settings → AI → Downloaded**, then import again.
- **Search finds nothing.** Check that your import finished in **Import → History**.
- **The app won't start because the database won't open.** Quit Lattice, move `lattice.db` out of the data folder (keep it in case you need it), and start again with a fresh database. Don't delete the models folder; it has nothing to do with database problems.

For more, see the [Troubleshooting](troubleshooting.md) guide and the [FAQ](faq.md). **Settings → Logs** shows recent log messages.

---

## Next Steps

- [User Manual](user-manual.md): every part of the app in detail.
- [Advanced Features](advanced-features.md): deeper options for power users.
