# Getting Started with Lattice

This guide covers installing Lattice, setting up its AI models on first launch,
and a short first session.

## Table of Contents

1. [Supported Systems](#supported-systems)
2. [Installation](#installation)
3. [First Launch](#first-launch)
4. [Your First Session](#your-first-session)
5. [Where Lattice Keeps Your Data](#where-lattice-keeps-your-data)
6. [Uninstalling](#uninstalling)
7. [If Something Goes Wrong](#if-something-goes-wrong)
8. [Next Steps](#next-steps)

---

## Supported Systems

Lattice is built for:

- **Windows 11** (x64)
- **macOS 13.3 or later** on Apple Silicon. A separate Intel Mac build is being
  qualified; Intel Macs run models on the CPU, so expect slower answers than on
  Apple Silicon.
- **Ubuntu 24.04 LTS** (x64)

Other Linux distributions, ARM Windows and ARM Linux are not supported.

**Disk space:** models take several gigabytes. The setup dialog on first launch
shows the exact download size and warns you if there isn't enough free space.

**Memory:** Lattice suggests a chat model sized to your computer's RAM. More
memory lets you run larger models.

---

## Installation

Lattice is pre-release. Get the installer for your platform from whoever
distributes your build.

### Windows

Run the installer (`.msi` or setup `.exe`) and follow the prompts, then start
Lattice from the Start menu. Pre-release builds are not code-signed, so Windows
SmartScreen may warn you; choose **More info > Run anyway** if you trust the
source.

### macOS

1. Open the `.dmg` and drag **Lattice** to **Applications**. Apple Silicon and
   Intel Macs use different builds; pick the one for your Mac.
2. Open Lattice from Applications.
3. Pre-release builds are not notarized. If macOS says it can't verify the app,
   open **System Settings > Privacy & Security** and choose **Open Anyway**.

### Ubuntu 24.04

Install the `.deb` package:

```bash
sudo apt install ./Lattice_*_amd64.deb
```

Or use the AppImage:

```bash
chmod +x Lattice_*.AppImage
./Lattice_*.AppImage
```

---

## First Launch

### Set up AI

If no models are installed yet, Lattice opens a **Set up AI** dialog. It checks
your hardware and offers two models that fit it:

- a **chat** model, which writes answers, and
- an **embedding** model, which turns text into vectors for search by meaning.

The dialog shows each model's size and the total. Choose:

- **Install** to download both in the background. You can keep using the app
  while they download.
- **Choose models** to pick your own in Settings. Model downloads are under
  **Settings > AI > Models**.
- **Not now** to skip. The dialog won't come back, but you can install models
  any time from **Settings > AI > Models**.

Importing and indexing need the embedding model, so install at least that one
before you import. Chat needs a chat model, or a provider you configure under
**Settings > AI > Chat**.

After the models download, Lattice runs offline. It uses the network only when
you import a web page, turn on web or Wikipedia search in a chat, browse or
download models, check for updates, or use a remote provider or custom tool you
set up.

---

## Your First Session

### 1. Import some documents

Press **⌘I** (**Ctrl+I** on Windows and Linux) to open **Import**:

- **Files**: drop files onto the page, or click **choose files**.
- **URL** / **URLs**: import one web page, or several, one per line.
- **History**: see past imports and any that failed.

Lattice copies each file into its own library, extracts the text and indexes it.
To import a whole folder, open the command palette (**⌘K**) and choose
**Add folder**.

PDF, Word (`.docx`), text, Markdown and many code and data formats are
supported. See [Supported file types](user-manual.md#supported-file-types).

### 2. Search

Press **⌘1** for **Search** and type what you remember. **Hybrid** (the default)
combines exact keywords with meaning; **Semantic** and **Keyword** use one or the
other. You can also press **⌘K** from anywhere to jump to a document.

### 3. Ask a question

Press **⌘4** for **Chat**, type a question and press **Enter**. Answers cite the
passages they used; hover a citation to see the passage and click it to open the
source.

### 4. Keep a thought

Press **⌘⇧N** for **Quick capture**, type a note and press **Enter**. It is added
to today's page in your **Journal** (**⌘3**).

### 5. Look around

- **Library** (**⌘2**) lists everything you've imported.
- **Explorer** (**⌘6**) opens a folder on disk next to a chat that can read it.
- **Studio** (**⌘7**) builds learning programs and holds your flashcards.

The [User Manual](user-manual.md) covers each of these.

---

## Where Lattice Keeps Your Data

| What | macOS | Windows | Ubuntu |
| --- | --- | --- | --- |
| Database, settings, backups, exports, Explorer folder indexes | `~/Library/Application Support/tech.lattice.app/` | `%APPDATA%\tech.lattice.app\` | `~/.local/share/tech.lattice.app/` |
| Copies of imported files | `~/.lattice/files/` | `%USERPROFILE%\.lattice\files\` | `~/.lattice/files/` |
| Downloaded models | `~/.cache/lattice/models/` | `%USERPROFILE%\.cache\lattice\models\` | `~/.cache/lattice/models/` |
| Logs | `~/Library/Application Support/lattice/logs/` | `%LOCALAPPDATA%\lattice\logs\` | `~/.local/share/lattice/logs/` |

The database file is `lattice.db`. If you turn on **Mirror notes to disk** in
**Settings > Vault**, your notes are also written as Markdown files to a folder
you choose (`~/Lattice` by default).

---

## Uninstalling

1. Quit Lattice.
2. Remove the app: drag it from Applications to the Trash (macOS), use
   **Settings > Apps** (Windows), or run `sudo apt remove lattice` (Ubuntu
   `.deb`) or delete the AppImage.
3. To remove your data as well, delete the folders listed in
   [Where Lattice Keeps Your Data](#where-lattice-keeps-your-data). Back up or
   export first if you want to keep anything.

---

## If Something Goes Wrong

- **The setup dialog says there isn't enough disk.** Free up space, then install
  models from **Settings > AI > Models**.
- **A model download failed.** Try it again from **Settings > AI > Models**.
  Gated models on Hugging Face need a token, which you can add in the
  **Hugging Face** section of the same page.
- **Chat says there's no model.** In **Settings > AI > Downloaded**, choose
  **Set as Chat** on a downloaded chat model, and check the provider under
  **Settings > AI > Chat**.
- **Import says the embedding model is not ready.** Install an embedding model
  in **Settings > AI > Models**, choose **Set as Embedding** on it under
  **Settings > AI > Downloaded**, then import again.
- **Search finds nothing.** Check that your import finished in
  **Import > History**.
- **The app won't start because the database won't open.** Quit Lattice, move
  `lattice.db` out of the data folder (keep it in case you need it), and start
  again with a fresh database. Don't delete the models folder; it has nothing to
  do with database problems.

For more, see the [Troubleshooting](troubleshooting.md) guide and the
[FAQ](faq.md). **Settings > Logs** shows recent log messages.

---

## Next Steps

- [User Manual](user-manual.md): every part of the app in detail.
- [Advanced Features](advanced-features.md): deeper options for power users.
