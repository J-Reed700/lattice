All five research tracks are complete. Here is the consolidated report.

---

# Cloud Backup Options for Lattice — Research Report

**Date: 2026-09-16.** Scope: off-device *backup* durability (not real-time multi-device sync). Target: Tauri 2 desktop app, Rust backend, one SQLite DB via sqlx (~50 tables incl. large re-derivable embedding tables), plus a markdown vault and imported PDFs/DOCX. Marketed local-first and privacy-focused.

Legend: ✅ verified against a primary source · ⚠️ uncertain, inferred, or stale · ❌ could not verify.

**Method note:** this session exhausted its 200-call WebSearch budget across five parallel research tracks. Later findings came from direct WebFetch/curl against known URLs, the crates.io and GitHub JSON APIs, and the RustSec advisory DB. Items that stayed unverified are marked, not guessed. A roll-up of every open item is at the end.

---

## Executive summary — the three most viable approaches, ranked

### 1. Encrypted snapshot archive written to a user-chosen destination folder (ship this first)

One `VACUUM INTO` snapshot + the vault + a manifest, compressed, client-side encrypted, written atomically as a timestamped archive into a folder the user picks with a native file dialog. That folder may be their iCloud Drive, Dropbox, OneDrive or Google Drive folder — or an external disk, or a NAS share. You never name or detect a provider.

**Why it ranks first:** zero infrastructure, zero storage cost, zero DPA, zero liability, no OAuth approval gates, and it is exactly the market norm (Bear, Cryptomator, Joplin's filesystem target, Anki's `.colpkg`). It directly answers the owner's actual worry — a dead drive — on day one. Getting the destination from an `NSOpenPanel` also sidesteps every path-detection problem *and* earns the macOS TCC grant through the file picker rather than a cold permission prompt.

**What it costs you:** the restore path is genuinely hazardous when the destination is a cloud-synced folder, because the file may be evicted to cloud-only (§2.3). You must detect placeholder state before opening. And you must never let the live `.db`/`-wal`/`-shm` near a synced folder (§2.4) — SQLite's own docs tell app authors to *block* that election.

### 2. Direct upload to the user's own cloud via OAuth app-scoped folders (the reliability upgrade)

Same encrypted archive, but pushed over the API to Google Drive (`drive.file`), OneDrive (`Files.ReadWrite.AppFolder`) and Dropbox (App folder). This removes the eviction/placeholder hazard entirely — you get real HTTP with resumable uploads, real progress, and real error codes.

**The decisive finding:** the app-scoped and file-scoped permissions are classified **non-sensitive** by Google and **no-admin-consent** by Microsoft, so you avoid Google's annual, lab-conducted CASA security assessment and Microsoft's tenant-admin gate entirely (§2.6). That is the difference between a viable indie integration and an unaffordable one.

**What it costs you:** roughly three separate OAuth integrations, Dropbox's 50-user/two-week production-approval clock that cannot be reset by unlinking users, and a hard quota wall on Dropbox Free (2 GB) and effectively on Google Free (15 GB shared with Gmail and Photos).

### 3. Vendor-hosted encrypted backup that you operate (the monetizable option, later)

Client-side-encrypted content-addressed objects to Backblaze B2 or Hetzner Object Storage. **The storage cost is trivially small: ~$13.90/month on B2 or ~$16.85/month on Hetzner for 10,000 users at 200 MB each** (§3.4). Storage will not make or break the unit economics — support load and key-recovery tickets will.

**Rank it third, not never.** It is the only option that works for a user with no cloud account, and the only one you can charge for. But it converts a zero-liability product into one holding 2 TB of other people's data, with a GDPR posture, a DPA, an abuse story, and "I lost my passphrase" tickets. Do it once options 1 and 2 have proven the backup format.

### Explicitly not recommended

**Litestream as a bundled sidecar.** It is excellent software and it is the obvious-looking answer, but three things disqualify it here. (a) **v0.5.x has no client-side encryption** — age support was silently dropped during the LTX refactor and the dependency was removed outright in March 2026; the only encryption on offer is SSE-C/SSE-KMS, which is server-side (§1.1). For a privacy-marketed app that is fatal. (b) It is continuous WAL replication designed for an always-on server, and a desktop app that starts and stops forces repeated full snapshots. (c) It is a 13 MB Go binary per platform that you must sign and notarize yourself.

---

## 1. SQLite-specific backup and replication tooling from Rust

### 1.1 Litestream

**Current status:** ✅ **v0.5.17, released 2026-08-31.** Apache-2.0, written in Go, 14,379 stars, actively developed (last push 2026-09-14). Repo: [github.com/benbjohnson/litestream](https://github.com/benbjohnson/litestream). Verified via the GitHub releases API.

v0.5.0 was a major rewrite ([fly.io/blog/litestream-v050-is-here](https://fly.io/blog/litestream-v050-is-here/)): WAL segments were replaced by the **LTX** data-shipping format with hierarchical compaction at 30-second, 5-minute and hourly levels, enabling point-in-time recovery from "only a dozen or so files on average." CGO was dropped (moved from `mattn/go-sqlite3` to `modernc.org/sqlite`). One replica destination per database is now required, where earlier versions allowed several. Fly.io has publicly shifted focus back to Litestream from LiteFS.

**Sidecar only — there is no embedded mode.** The docs describe it as "a sidecar process in the background, alongside unmodified SQLite applications." It *is* a Go library (`pkg.go.dev/github.com/benbjohnson/litestream`) but there is no Rust binding and no C ABI, so from Tauri it can only be an `externalBin` sidecar.

**Bundling it in Tauri is mechanically straightforward.** Release assets are ~12.3–13.9 MB compressed per platform, including **Windows x86_64 and arm64**, macOS arm64/x86_64, and Linux. Tauri 2's `externalBin` requires the `-$TARGET_TRIPLE` filename suffix and a `shell:allow-execute` capability with `"sidecar": true` ([v2.tauri.app/develop/sidecar](https://v2.tauri.app/develop/sidecar/)). ⚠️ Tauri's sidecar docs say nothing about code-signing or notarizing bundled binaries on macOS — you would have to work that out yourself.

**Storage targets:** S3, File, GCS, Azure Blob, SFTP, NATS JetStream, Alibaba OSS, and **WebDAV** ([litestream.io/reference/config](https://litestream.io/reference/config/)). The S3 client auto-detects R2, B2, DigitalOcean Spaces, MinIO and others from the endpoint URL and sets provider-specific options — notably concurrency=2 for R2 to respect its concurrent-upload limits.

**🔴 The disqualifying finding — no client-side encryption:**
- ✅ Litestream 0.4.x supported `age` client-side encryption (PR #468, 2023).
- ✅ It was removed during the LTX storage refactor in **PR #645, commit 90cf9f2, June 2025**, but the config parsing was left in place. From [issue #790](https://github.com/benbjohnson/litestream/issues/790) (2025-10-13), verbatim: *"age encryption configuration is currently accepted but no actual encryption occurs. This is a critical security issue as users believe their data is encrypted when it's actually being written in plaintext to remote storage."*
- ✅ PR #791 made it error out at startup rather than silently lie. PR #870 (**2026-03-26**) removed the age dependency entirely.
- ✅ The config reference now states plainly: *"Age client-side encryption is unsupported in v0.5.x… a replica configuration whose `age:` block… causes Litestream to exit at startup."*
- ✅ What exists instead is **SSE-C and SSE-KMS** (PR #902, 2025-12-30) — server-side encryption, where the key transits to the provider. Not E2EE.

**Desktop lifecycle mismatch.** Litestream takes over checkpointing by holding a long-running read transaction so no other process can restart the WAL ([litestream.io/how-it-works](https://litestream.io/how-it-works/)). When it notices the WAL was overwritten by an external process — which is what happens when the app runs while Litestream doesn't — it forces a new generation and a **full snapshot**. For an app that launches and quits several times a day, that means repeatedly re-uploading the whole database. ⚠️ The "generation" language is from pre-0.5 docs; 0.5 uses LTX compaction levels, but the underlying constraint is the same.

**Operational caveats from [litestream.io/tips](https://litestream.io/tips/):** requires WAL mode; set `PRAGMA busy_timeout = 5000`; under load set `PRAGMA wal_autocheckpoint = 0` so your app doesn't checkpoint between Litestream's checkpoints; replication is asynchronous with a ~1s default window, so unreplicated data is lost on a hard crash; and *"multiple applications replicating into the same bucket & path can cause situations where you will be unable to restore."*

**Restore story is genuinely good:** `litestream restore` supports `-timestamp`, `-txid`, `-o`, `-force`, `-if-db-not-exists`, `-if-replica-exists`, `-integrity-check {none|quick|full}`, `-dry-run`, and `-parallelism` (default 8). It refuses to overwrite an existing non-empty database unless forced, and auto-detects v0.3.x vs LTX formats.

**Maturity caveat worth citing:** [mtlynch.io/notes/hold-off-on-litestream-0.5.0](https://mtlynch.io/notes/hold-off-on-litestream-0.5.0/) (2025-10-14) documents a restore failure with "transaction not available" matching an issue labeled *"CRITICAL — Complete Data Loss"*, plus B2 endpoint validation failures and uncompacted L0 files. All were fixed by 0.5.2, and the author explicitly frames it as a migration caution rather than criticism — but it is a useful reminder that 0.5.x is a young rewrite.

**Litestream VFS** ([fly.io/blog/litestream-vfs](https://fly.io/blog/litestream-vfs/), last updated 2025-12-11) is a separate SQLite plugin that reads pages directly from object storage without fetching the whole database, including at a historical timestamp. Described as production-ready at Fly.io. Ships as its own 14–26 MB binary. Interesting for a "preview your backup before restoring" feature, but it doesn't change the encryption problem.

### 1.2 The SQLite online backup API and `VACUUM INTO`

✅ [How To Corrupt An SQLite Database File §1.2](https://www.sqlite.org/howtocorrupt.html) lists exactly **three** safe ways to copy a live database — `sqlite3_rsync`, `VACUUM INTO`, and the backup API — and states *"Any of the above approaches will work even on a live database."* Naive file copy is explicitly unsafe: *"Systems that run automatic backups in the background might try to make a backup copy of an SQLite database file while it is in the middle of a transaction. The backup copy then might contain some old and some new content, and thus be corrupt."*

**`VACUUM INTO`** — ✅ added in **SQLite 3.27.0 (2019-02-07)**; changelog line verbatim: *"Added the VACUUM INTO command."* (Confirmed by grepping [sqlite.org/changes.html](https://sqlite.org/changes.html); an earlier automated read of that page misattributed it to 3.41.0 — it is 3.27.0.) 3.28.0 made it work on read-only databases. Semantics from [sqlite.org/lang_vacuum.html](https://www.sqlite.org/lang_vacuum.html):
- *"The VACUUM INTO command is transactional in the sense that the generated output database is a consistent snapshot of the original database."*
- *"However, if the VACUUM INTO command is interrupted by an unplanned shutdown or power loss, then the generated output database might be incomplete and corrupt."*
- *"The file named by the INTO clause must not previously exist, or else it must be an empty file, or the VACUUM INTO command will fail with an error."*
- It honors `PRAGMA synchronous` — keep it at NORMAL or FULL for the snapshot so SQLite fsyncs the output.
- It fails if there is an open transaction on the connection running it.
- Output is a single file with no `-wal`/`-shm`, and it drops free pages, so it is smaller than the live DB.

**This is the right primitive for Lattice**, and it is reachable from sqlx today as a plain SQL statement with no extra dependency.

**The online backup API** ([sqlite.org/c3ref/backup_finish.html](https://sqlite.org/c3ref/backup_finish.html)) is the incremental alternative. Key semantics:
- *"If the source database is modified by an external process or via a database connection other than the one being used by the backup operation, then the backup will be automatically restarted by the next call to sqlite3_backup_step()."* — on a busy DB this can livelock; on a desktop app it is fine.
- Source and destination must be **different connections**, or `sqlite3_backup_init` fails.
- *"The first call to sqlite3_backup_step() obtains an exclusive lock on the destination file"*, and the destination connection must not be touched by any other API or thread during the backup, on pain of mutex deadlock.

**From Rust:** `rusqlite`'s `backup` module (feature flag `backup`) wraps this with `Backup::new(src, &mut dst)`, `step(pages)`, `progress()` and `run_to_completion(pages_per_step, sleep, callback)` ([docs.rs/rusqlite/latest/rusqlite/backup](https://docs.rs/rusqlite/latest/rusqlite/backup/index.html)). rusqlite is at **0.40.2 (2026-08-08)**.

**🔴 Concrete blocker for Lattice — you cannot just add rusqlite alongside sqlx today.** Verified via the crates.io dependencies API:
- `sqlx-sqlite` 0.9.0 requires `libsqlite3-sys >=0.30.1, <0.38.0`
- `rusqlite` 0.40.2 requires `libsqlite3-sys ^0.38.1`
- `libsqlite3-sys`'s Cargo.toml declares **`links = "sqlite3"`** (confirmed from the raw manifest)
- Cargo's rule, verbatim from [the Cargo book](https://doc.rust-lang.org/cargo/reference/build-scripts.html): *"Cargo requires that there is at most one package per `links` value. In other words, it is forbidden to have two packages link to the same native library."*

So adding rusqlite 0.40 to a workspace using sqlx 0.9 is a hard build error. Three ways out, in order of preference:
1. **Use `VACUUM INTO` through sqlx.** It is one SQL statement, needs nothing new, and gives you the consistent snapshot. This is what I would do.
2. **Call `sqlite3_backup_*` directly through the `libsqlite3-sys` that sqlx already links.** sqlx exposes the raw handle: `SqliteConnection::lock_handle() -> LockedSqliteHandle`, and `LockedSqliteHandle::as_raw_handle(&mut self) -> NonNull<sqlite3>` ([docs.rs](https://docs.rs/sqlx/latest/sqlx/sqlite/struct.LockedSqliteHandle.html)). The docs note *"as long as this LockedSqliteHandle exists, it is guaranteed that the background thread is not making FFI calls on this database handle."* This is unsafe but supported.
3. Pin an older rusqlite that uses `libsqlite3-sys` 0.37.x. ❌ I did not verify which rusqlite version that is.

⚠️ **Also worth knowing about sqlx:** the repository moved from `launchbadge/sqlx` to **`transact-rs/sqlx`** shortly after the 0.9.0 release — *"SQLx has not been owned or maintained by LaunchBadge, LLC"* and has been *"informally transferred to the collective ownership of its principal authors."* Old URLs redirect. ⚠️ Release dates for 0.9.0 are inconsistent across sources (crates.io says 2026-05-21, the release discussion says 2026-05-06, docs.rs says 2026-07-20) — not load-bearing, but flagged.

### 1.3 `sqlite3_rsync`

✅ Introduced in **SQLite 3.47.0 (2024-10-21)** — changelog line verbatim: *"Add the experimental sqlite3_rsync program."* ✅ **3.50.0 (2025-05-29)** removed the two big constraints: *"The requirement that the database be in WAL mode has been removed"* and the protocol was enhanced *"to use less network bandwidth when both sides start out being very similar to one another."* 3.53.0 added `-p|--port`. Latest SQLite is **3.53.4 (2026-07-24)**.

**It is not usable for cloud backup.** From [sqlite.org/rsync.html](https://sqlite.org/rsync.html): it transports over **SSH**, requires `sqlite3_rsync` installed on the remote system's PATH, requires at least one database to be local, and *"While sqlite3_rsync is running, REPLICA is read-only."* It is a separate tool that must be compiled independently — not part of the amalgamation, no Rust bindings.

**But its numbers are the best argument for your backup design:** *"a 500MB database typically synchronizes with approximately 20KB of network traffic when source and replica are similar"* — roughly 25,000:1, achieved by exchanging **cryptographic hashes of pages or groups of pages** and shipping only differing page content. No content-defined chunking anywhere. This is the model to imitate (§4.3).

### 1.4 Rust crates for incremental/encrypted SQLite backup to object storage

**There is nothing production-ready.** Verified via the crates.io API on 2026-09-16:

| Crate | Latest | Last updated | Recent downloads | Verdict |
|---|---|---|---|---|
| `bottomless` (libSQL S3 replication) | 0.1.16 | **2023-01-20** | 148/90d | ❌ Dead |
| `verneuil` (S3 VFS, Backtrace Labs) | 0.6.4 | **2022-02-23** (repo pushed 2026-09-09) | 109/90d | ❌ **Linux-only**, and rollback-journal only, not WAL. MIT, 524 stars. Unusable cross-platform. |
| `wal-backup` | 0.8.11 | 2026-05-20 | **25 downloads total** | ❌ Single publish, no history, one author. Self-describes as *"Like Litestream, but embedded."* Do not ship this. |
| `infinitree` (encrypted embedded DB w/ tiered cache) | 0.11.0 | **2024-11-13** | 185/90d | ❌ Dormant |
| `libsql` / `turso` | 0.9.30 / 0.7.2 | 2026 | high | Different database engines, not backup tools |

**Conclusion: write it yourself.** The whole job — `VACUUM INTO` a scratch file, exclude the embedding tables, tar+zstd with the vault, encrypt with `chacha20poly1305`, PUT via `object_store` — is on the order of 300 lines and has no viable off-the-shelf alternative that is both cross-platform and E2EE.

---

## 2. Using the user's own cloud, with no server from us

### 2.1 Detecting cloud folders — macOS

✅ Verified first-hand on macOS 26.6.2 (build 25G83):

| Thing | Path | Notes |
|---|---|---|
| iCloud Drive (user-visible) | `~/Library/Mobile Documents/com~apple~CloudDocs` | ✅ exists, mode **0700**, writable by owner |
| iCloud per-app containers | `~/Library/Mobile Documents/iCloud~com~example~app` | ✅ 100+ present on the test machine |
| Parent directory | `~/Library/Mobile Documents` | ✅ mode **0500 — no write bit.** You cannot create a top-level container here with POSIX calls. |
| Third-party providers | `~/Library/CloudStorage/` | ✅ exists, mode 0755 |

**The File Provider migration is done.** Since macOS 12.3 Apple deprecated the kernel extensions cloud clients used for virtual drives; Dropbox, Google Drive, OneDrive and Box are now File Provider extensions living under `~/Library/CloudStorage/`. Background: [TidBITS](https://tidbits.com/2023/03/10/apples-file-provider-forces-mac-cloud-storage-changes/), [Michael Tsai's roundup](https://mjtsai.com/blog/2023/03/14/update-on-cloud-file-provider-extensions/).

⚠️ **The subdirectory naming convention under `~/Library/CloudStorage/` is inferred from community reports, not from any Apple or vendor specification.** Expected: `GoogleDrive-<email>`, `Dropbox` or `Dropbox-<TeamName>`, `OneDrive-Personal` / `OneDrive-<TenantName>`, `Box-Box`. Google Workspace admin docs do confirm Drive appears in `~/Library/CloudStorage` ([source](https://knowledge.workspace.google.com/admin/drive/advanced-drive-for-desktop-configuration?hl=en)); Dropbox's own [macOS File Provider page](https://help.dropbox.com/installs/dropbox-for-macos-support) states the macOS 12.5+ requirement but **does not state the path**. **Do not hardcode these** — `readdir` the directory and prefix-match.

**Dropbox has a real config file and you should use it:** `~/.dropbox/info.json` on macOS and Linux, `%APPDATA%\Dropbox\info.json` or `%LOCALAPPDATA%\Dropbox\info.json` on Windows ([Dropbox Help](https://help.dropbox.com/installs/locate-dropbox-folder)). Schema is `{"personal": {"path": "...", "host": ..., "is_team": false, ...}, "business": {...}}`.

### 2.2 Detecting cloud folders — Windows and Linux

The most reliable concrete reference is the **live production source of the Files file manager**, which does exactly this job across all providers: [files-community/Files](https://github.com/files-community/Files), `src/Files.App/Utils/Cloud/CloudDrivesDetector.cs` and `Detector/*.cs`. All of the following is code-verified against that source.

**OneDrive** — ⚠️ these keys are an implementation detail, not a documented Microsoft API:
```
HKCU\SOFTWARE\Microsoft\OneDrive\Accounts\<Personal|Business1|Business2|…>
    DisplayName  (REG_SZ)
    UserFolder   (REG_SZ)  <- the local sync root
    ScopeIdToMountPointPathCache  (subkey, SharePoint library mounts)
```

**Google Drive** — Files reads Drive's persisted state rather than the shell, with a source comment noting that touching the live shell/DriveFS can block for ~19 seconds:
```
%LOCALAPPDATA%\Google\DriveFS\root_preference_sqlite.db  (+ .db-wal)
  SELECT * FROM roots                  -> mirrored folders; last_seen_absolute_path (strip \\?\), title
  SELECT * FROM media WHERE fs_type=10 -> streaming mounts; last_mount_point + "\My Drive"
```
Registry alternative: `HKCU\Software\Google\DriveFS`, value `PerAccountPreferences` (JSON with `value[].mount_point_path`). ⚠️ **Files copies the `.db` and `-wal` to temp before opening** because it is a live database owned by another process — which is itself a neat demonstration of §2.4. The Windows drive letter is configurable (`DefaultDriveLetter`, commonly `G:`) — never assume it.

**Dropbox on Windows:** `%LOCALAPPDATA%\Dropbox\info.json`, with a Microsoft Store fallback at `%LOCALAPPDATA%\Packages\DropboxInc.Dropbox_*\LocalCache\Local\Dropbox\info.json` (pick the most recently modified).

**Generic Explorer-namespace walk** (catches OneDrive, Box, Dropbox, iCloud Drive, Nextcloud, Proton):
```
HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\Desktop\NameSpace\{CLSID}
HKCR\CLSID\{CLSID}                            System.IsPinnedToNameSpaceTree == 1
HKCR\CLSID\{CLSID}\Instance\InitPropertyBag   TargetFolderPath   <- the sync root
HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\SyncRootManager\{id}
```
iCloud Drive on Windows is matched by a namespace identifier beginning with the literal `iCloudDrive`.

**Linux: there is essentially nothing to detect.** Dropbox's `~/.dropbox/info.json` is the only first-party option. Google Drive and OneDrive have no official Linux clients; iCloud Drive does not exist. **On Linux, offer "pick a folder" and the API path only.**

### 2.3 Pitfalls: eviction and placeholders — the restore hazard

**This is the single most under-appreciated risk in the whole synced-folder approach, and it bites on restore, not on backup.**

**macOS / iCloud — measured first-hand on macOS 26.6.2:**

Evicted files under "Optimize Mac Storage" are **dataless objects** carrying the BSD flag `SF_DATALESS` (`0x40000000`, defined in `<sys/stat.h>`):
```
$ ls -lO "01 - Shoot It Out.mp3"
-rw-------@ 1 josh staff compressed,dataless 5702119 Jul 13 13:19 ...
$ stat -f 'size=%z blocks=%b flags=%Sf' ...
size=5702119 blocks=0 flags=compressed,dataless
```
✅ **`stat()` reports the full logical size but `st_blocks == 0`.** A size check will lie to you; the block count will not. Detect from Rust with `libc::lstat` and test `st_flags & 0x40000000` — no Objective-C bridge needed. ✅ The old `.icloud` placeholder files are **gone**; modern macOS uses the flag.

✅ **What a read actually does, measured:**
```
$ dd if=<4,207-byte dataless file> bs=1 count=1 of=/dev/null
1 bytes transferred in 5.583670 secs      <-- 5.6 SECONDS to read ONE byte
```
Afterwards the dataless flag is gone and the file is fully materialized. **So: reading an evicted file transparently materializes it, and `read()` blocks for the entire download** — with no progress signal, and outright failure if the machine is offline. Extrapolate to a multi-GB backup archive on a slow link.

✅ **Mitigation** (verified in `man setiopolicy_np` and `<sys/resource.h>` on this machine): call `setiopolicy_np(IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES, IOPOL_SCOPE_THREAD, IOPOL_MATERIALIZE_DATALESS_FILES_OFF)` on a probe thread so `open()` fails fast instead of silently blocking, then show real "downloading from iCloud…" UX. ⚠️ **Discrepancy:** the man page says the process-scope default is already `OFF`, yet the measurement above shows materialization happening by default. Either the CloudDocs path overrides the policy or the man page is wrong. Test before relying on it.

✅ **`brctl download` / `brctl evict` no longer exist.** Confirmed by running `brctl` on macOS 26.6.2 — subcommands are only `diagnose, log, dump, status, accounts, quota, monitor`. `fileproviderctl` has no `materialize`. The blog-post escape hatches you will find online ([Eclectic Light](https://eclecticlight.co/2024/03/11/icloud-drive-in-sonoma-optimise-mac-storage-or-not/), [icanhasjonas/icloud-tools](https://github.com/icanhasjonas/icloud-tools)) are stale. The supported API is `NSURLUbiquitousItemDownloadingStatusKey` + `FileManager.startDownloadingUbiquitousItem(at:)` ([Apple docs](https://developer.apple.com/documentation/foundation/nsurlubiquitousitemdownloadingstatuskey)).

✅ **Useful side discovery:** `brctl quota` works unprivileged and prints remaining iCloud quota (`13389059372 bytes of quota remaining in personal account`) — a cheap pre-flight check.

⚠️ When macOS decides to evict is undocumented; Eclectic Light reports it kicks in around <20 GB free. You have no control and cannot pin a file.

**Windows / OneDrive Files On-Demand:** ✅ three states, settable with `attrib` ([Microsoft Learn](https://learn.microsoft.com/en-us/onedrive/files-on-demand-windows), updated 2025-11-17): `attrib +p` (always available), `attrib -p` (locally available), `attrib +u` (online-only). All items are reparse points owned by the `CldFlt` filter driver. ⚠️ `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` (0x00400000) and `FILE_ATTRIBUTE_RECALL_ON_OPEN` (0x00040000) are the modern markers and `CfGetPlaceholderStateFromFileInfo` the modern API — ❌ **not verified in this session.** The legacy [Placeholder files](https://learn.microsoft.com/en-us/windows/compatibility/placeholder-files) page is dated 2018-07-11 and genuinely stale, but it does confirm the behavior: if you use the common file dialog, *"the file content will be downloaded and will be passed to your app"* — the same transparent blocking hydration. `FILE_FLAG_OPEN_NO_RECALL` is the opt-out (❌ unverified).

**Dropbox:** ✅ online-only items are placeholders and on macOS 12.3+ *"show as zero bytes in size"* ([Dropbox Help](https://help.dropbox.com/installs-integrations/sync-uploads/smart-sync)). ⚠️ Dropbox publishes **no developer API** for detecting or forcing hydration. ❌ No reliable cross-platform detection method found.

**Google Drive:** ✅ [streaming vs mirroring](https://support.google.com/drive/answer/13401938?hl=en) is the biggest variable. Streaming (the default) means the "Drive folder" is a virtual filesystem and reading back a large archive may require a full re-download; mirroring behaves like a normal folder. ⚠️ You cannot reliably tell which mode a user is in from the filesystem alone.

**Universal rule:** the read either blocks for the full download or fails when offline, and in both cases your progress UI shows nothing. Probe for placeholder/dataless state *before* opening, tell the user explicitly, never wrap the read in a short timeout, and never assume `stat().size` means the bytes are local.

### 2.4 Why the live SQLite DB must never go in a synced folder

This is the least ambiguous finding in the report; the evidence is unanimous and primary.

✅ [How To Corrupt An SQLite Database File §2.1](https://sqlite.org/howtocorrupt.html): *"SQLite depends on the underlying filesystem to do locking as the documentation says it will. But some filesystems contain bugs in their locking logic… This is especially true of network filesystems and NFS in particular."* §1.3: *"If the hot journal files are moved, deleted, or renamed after a crash or power failure, then automatic recovery will not work and the database may go corrupt."* §1.4 explicitly lists *"Copying a database file without also copying its journal"* as a corruption cause — which is exactly what a sync client does when it uploads `.db` and `-wal` at different moments.

✅ [SQLite Over a Network](https://www.sqlite.org/useovernet.html): *"SQLite relies on exclusive locks for write operations, and those have been known to operate incorrectly for some network filesystems. **This has led to database corruption.**"* … *"Network filesystem sync operation can be less robust than local filesystem sync… **Rely upon it at your (and your customers') peril.**"* And, directed at you specifically as an app author: *"a programmer should consider **blocking that election** unless reliability is of little importance."*

✅ **Anki's manual**, verbatim ([docs.ankiweb.net/files.html](https://docs.ankiweb.net/files.html)): *"We do not recommend you sync your Anki folder directly with a third-party synchronization service, **as it can lead to database corruption when files are synced while in use.**"*

✅ **Obsidian** warns that running two sync mechanisms at once *"may cause data loss, corruption, and other issues"*, and its own docs state *"iCloud Drive on Windows may lead to file duplication or corruption"* ([obsidian.md/help/sync-notes](https://obsidian.md/help/sync-notes)). ⚠️ Community reports of the same pattern exist for Joplin, Trilium ([#3856](https://github.com/zadam/trilium/discussions/3856)) and onedrive-abraunegg ([#688 "Sqlite database corrupted prevents synchronization"](https://github.com/abraunegg/onedrive/issues/688)) — forum-quality evidence, flagged as such.

**The mechanism for Lattice specifically:** in WAL mode a consistent database is `foo.db` + `foo.db-wal` + `foo.db-shm`. `-shm` is shared memory that must never be synced or restored; `-wal` is meaningless without its matching `.db`. Sync clients upload files independently at different times and write them back in arbitrary order, so you will eventually pair a `-wal` from one moment with a `.db` from another → *"database disk image is malformed."* On top of that, sync daemons rewrite files in place and take their own locks, and `fcntl` advisory locks mean nothing to a userspace sync daemon or a File Provider extension.

The irony worth noting: **Google's own Drive client keeps a live SQLite DB with a `-wal`, and third-party detectors copy it to temp before opening rather than reading it in place.**

### 2.5 Safe snapshot-into-a-synced-folder discipline

Writing a **single** snapshot file is safe, and it is what SQLite sanctions. The discipline:

1. **`VACUUM INTO` a scratch path on local disk**, never directly into the synced folder.
2. **Exclude the re-derivable embedding tables.** `VACUUM INTO` copies everything, so `ATTACH` a fresh DB and copy only the non-derivable tables (or drop the embedding tables in a copy, then `VACUUM INTO`). This is the single biggest lever on archive size, upload time, and quota fit.
3. **Wrap everything in one archive** (`.tar.zst`) — snapshot + vault + manifest with schema version, per-file SHA-256, total size, timestamp, app version. One file means "present or absent," never "half a set." It also sidesteps OneDrive's illegal-filename rules (§2.6), which a markdown vault with `:` in note titles will otherwise trip.
4. **Hash and fsync the archive locally**, then fsync the containing directory.
5. **Write as `.part` inside the destination, fsync, then `rename()`** to the final timestamped name. ⚠️ **Caveat:** `~/Library/CloudStorage/*` is a userspace File Provider filesystem and a Windows sync root is a reparse-point filesystem; `rename()` atomicity there is **not** guaranteed the way it is on APFS/ext4. ❌ Unverified. Design so a torn write is detectable via the manifest hash.
6. **Never overwrite the previous backup.** Timestamped filenames plus keep-N retention. Overwriting a 3 GB file means that for the duration of the re-upload there is *no* valid backup in the cloud.
7. ⚠️ Dropbox reportedly supports an ignore marker (xattr `com.dropbox.ignored=1`, NTFS ADS on Windows) for the `.part` file — ❌ unverified. No equivalent for iCloud or OneDrive.
8. **Bandwidth reality check:** a compressed archive has no stable block boundaries, so Dropbox's block-level delta sync and OneDrive's differential sync buy you nothing. Either don't compress (to let block dedup work) or compress and accept full re-uploads on a low frequency.
9. **Verify after the fact.** `brctl status` / `brctl monitor -w` exist on macOS 26.6.2 to wait for upload completion.

**On macOS, do not chase iCloud entitlements.** Writing to your app's own iCloud *container* requires `com.apple.developer.ubiquity-container-identifiers`, which requires a provisioning profile — unavailable to a Developer ID distribution. ✅ Independently confirmed by the filesystem: `~/Library/Mobile Documents` is mode 0500, so you couldn't create the container even if you wanted to. The user-visible `com~apple~CloudDocs` folder is an ordinary 0700 directory and almost certainly writable, ⚠️ though I inferred this from permission bits rather than performing a write. Apple's intent is that you write there via `NSFileCoordinator`; a raw Rust write bypasses that, which is probably fine for a write-once-then-rename snapshot but is unblessed.

**macOS TCC:** ✅ [Apple Platform Security](https://support.apple.com/guide/security/controlling-app-access-to-files-secddd1d86a6/web) confirms macOS 10.15+ requires user consent for *"Documents, Downloads, Desktop, iCloud Drive, and network volumes"*; Eclectic Light's [2025 survey](https://eclecticlight.co/2025/02/24/gaining-access-to-privacy-protected-folders/) adds third-party cloud storage and notes Apple's controls are *"largely undocumented, and discovered largely by trial and error."* **Full Disk Access is not required — do not ask for it.** ⚠️ macOS 13+ shows a separate provider-specific prompt for `~/Library/CloudStorage/<Provider>`; exact wording unverified. **Strong recommendation: get the destination from an `NSOpenPanel` (Tauri's dialog plugin)** — the grant comes through the picker, which is both less alarming and more reliable.

**Windows:** no TCC equivalent. ⚠️ Microsoft Defender **Controlled folder access** (off by default on consumer, often on in managed environments) blocks unrecognized apps from writing to Documents/Desktop and OneDrive-backed folders with an opaque access-denied — ❌ not tested. **Linux:** no prompts for AppImage/.deb; a Flatpak would need `--filesystem=home` or a portal.

### 2.6 Desktop OAuth for Dropbox / Google Drive / OneDrive

| | **Dropbox** | **Google Drive** | **OneDrive / MS Graph** |
|---|---|---|---|
| PKCE | ✅ recommended | ✅ "recommended" | ✅ recommended for all client types |
| Client secret needed? | ✅ **No** with PKCE | ⚠️ issued but listed **"Optional"** | ✅ **No** — *"public clients must not use secrets"* |
| Loopback redirect | `http://localhost:PORT` — **exact match incl. port** | `http://127.0.0.1:PORT` / `http://[::1]:PORT`, **random port explicitly blessed** | `http://localhost/path` — **port ignored for matching** |
| App-scoped folder | App folder access type | `drive.appdata` / `drive.file` | `special/approot` |
| Avoids verification burden? | n/a (own review process) | ✅ **both non-sensitive → no CASA** | ✅ **no admin consent** |
| Single-request upload cap | **150 MiB** | **5 MB** | ⚠️ use sessions |
| Chunk quantum | 4 MiB (concurrent sessions) | 256 KB | **320 KiB = 327,680 B** |
| Max file | ~2 TiB | 5 TB | **250 GB** |
| Counts against user quota | ✅ | ✅ | ✅ |
| Free tier | **2 GB** | 15 GB (shared w/ Gmail + Photos) | 5 GB |

**Google — the decisive finding on cost.** ✅ [Drive API scopes](https://developers.google.com/workspace/drive/api/guides/api-specific-auth) classifies `drive.file` and `drive.appdata` as **non-sensitive** (both marked *Recommended*), while `drive`, `drive.readonly` and `drive.metadata` are **restricted**. ✅ [Verification requirements](https://support.google.com/cloud/answer/13464321?hl=en), verbatim: *"Apps requesting access to **restricted scopes** must meet the additional requirement of secure data handling by submitting to an **annual security assessment** from a Google empanelled group of security assessors."* ✅ [CASA tiering](https://appdefensealliance.dev/casa/casa-tiering) confirms **both AL1 and AL2 are lab-tested** by an [authorized lab](https://www.appdefensealliance.org/certification/authorized-labs) (Bishop Fox, NCC Group, NowSecure, Leviathan, DEKRA, …), and *"All applications must be revalidated every year."*

❌ **No official dollar figure for CASA exists anywhere public.** The commonly-repeated $15k–$75k/year range could not be verified. What *is* verified is that it is a commercial engagement with a third-party lab that **recurs annually** — an open-ended operating cost for an indie app. ✅ **Using `drive.file` makes the entire problem disappear.** ⚠️ Being precise about the inference: I verified the non-sensitive classification and the "sensitive **or** restricted" trigger sentence, but Google never writes the literal sentence "drive.file requires no CASA." The inference is solid but is an inference.

**Prefer `drive.file` over `drive.appdata` for a backup product.** Same non-sensitive classification, but appDataFolder is *"hidden from the user and from other Google Drive apps"* and ✅ *"Users can also delete your app's data folder manually"* — a user tidying up hidden app data silently destroys the backup, and they can never see, copy, or independently restore it. `drive.file` writing to a visible, user-named folder is strictly better UX for backup and reads well on the consent screen.

⚠️ **Google publishing-status trap:** a project with an external consent screen in **"Testing"** status issues refresh tokens that **expire in 7 days**. You must move to "In production" or every user re-authenticates weekly. Also: published-app refresh tokens die after **six months** unused, and there is a **limit of 100 refresh tokens per Google Account per client ID** — exceeding it silently invalidates the oldest.

✅ **OOB is dead** (all clients blocked 2023-01-31) but ✅ **loopback is explicitly still supported for desktop**: *"The loopback IP address flow is being deprecated for iOS, Android, and Chrome OAuth client types but **will continue to be supported on desktop apps**"* ([loopback migration guide](https://developers.google.com/identity/protocols/oauth2/resources/loopback-migration)). ❌ Not verified whether Google made any 2025–2026 change to the Desktop client type.

**On Google's client secret:** ✅ the token-exchange parameter table lists `client_secret` as **"Optional"**, and the page concedes installed apps *"cannot keep secrets."* Google still issues one because the Desktop client type predates RFC 8252/PKCE and older libraries require the field. With PKCE the exchange is bound to the `code_verifier`, not the secret, so it adds no security. ⚠️ Test whether the endpoint actually rejects the exchange without it; if required, embed it and treat a leak as a non-event.

**Dropbox — the sharpest business constraint.** ✅ Verbatim from the [developer guide](https://docs.dropboxapi.com/dropbox-api/docs/developer-resources/developer-guide): *"once your app links **50** Dropbox users, you will have **two weeks** to apply for and receive production status approval before your app's ability to link additional Dropbox users will be **frozen**"* and *"the only way to unfreeze your app is to apply for and receive production status. Simply unlinking all of your users will not unfreeze your app."* Dev status caps at 500 users. ❌ No published SLA for review turnaround. **Apply early.**

Other Dropbox specifics: ✅ PKCE needs no secret even on refresh (confirmed by staff); **`token_access_type=offline` is mandatory** or you get no refresh token; refresh tokens do not expire and are not rotated (re-confirmed by staff 2026-07-10); ⚠️ redirect URIs must match **exactly including the port**, which is directly incompatible with `tauri-plugin-oauth`'s random-port default — register 3 fixed ports and try them in order; ⚠️ the "Apps" folder segment is **localized per user locale** (`/Appar/…` in Swedish), so never hardcode or display the absolute path, and your API paths are relative to the app root anyway; ✅ the access type **cannot be changed later** without deleting and recreating the app. Upload: 150 MiB single-request cap, sessions with 4 MiB append multiples for concurrent mode, 7-day session lifetime, resume via `{session_id, offset}` with HTTP 409 `incorrect_offset` carrying the correct value. ⚠️ **Rust gotcha:** the `Dropbox-API-Arg` header must JSON-escape `0x7F` and **all non-ASCII characters**, which `serde_json` does not do by default — non-ASCII vault filenames will break you.

**Rust SDK:** ✅ [`dropbox-sdk`](https://crates.io/crates/dropbox-sdk) **0.20.3 (2026-07-29)**, Apache-2.0, by `wfraser@dropbox.com`, MSRV 1.85, rustls, PKCE built in, with a first-party [`large-file-upload.rs` example](https://github.com/dropbox/dropbox-sdk-rust/blob/master/examples/large-file-upload.rs) doing concurrent sessions with resume. ⚠️ The README still says *"This SDK is not yet official… There is no formal Dropbox support for the SDK at this point."*

**OneDrive / Graph — the cleanest of the three.** ✅ `Files.ReadWrite.AppFolder` needs **no admin consent** and works for both personal MSA and work/school accounts ([permissions reference](https://learn.microsoft.com/en-us/graph/permissions-reference)). ✅ Special folders are *"automatically created the first time an application attempts to write to one"* and *"If a user deletes one, it is recreated when written to again"* — nicer than Google's appDataFolder. ✅ **Port is ignored for localhost redirect matching** ([reply-url docs](https://learn.microsoft.com/en-us/entra/identity-platform/reply-url)), so register once and bind any port. ⚠️ Three traps: don't register multiple localhost URIs differing only by port (differentiate by path); **IPv6 `[::1]` is not supported**; and you cannot add an `http` loopback URI through the Azure portal text box — you must hand-edit `replyUrlsWithType` in the manifest.

Upload ([createUploadSession](https://learn.microsoft.com/en-us/graph/api/driveitem-createuploadsession), updated 2026-08-11): ✅ *"the size of each byte range MUST be a multiple of 320 KiB (327,680 bytes)"* — *"Using a fragment size that doesn't divide evenly by 320 KiB results in errors committing some files."* Recommended 5–10 MiB fragments, max 60 MiB per request, **250 GB max file**. ✅ **Do not send the `Authorization` header on the PUTs** — *"it might result in an HTTP 401 Unauthorized response."* ✅ Built-in pre-flight quota check: passing `fileSize` returns **507 Insufficient Storage** and refuses to create the session. ⚠️ **Two official pages disagree on refresh-token lifetime** — the [refresh tokens page](https://learn.microsoft.com/en-us/entra/identity-platform/refresh-tokens) (2025-11-05) says 90 days for native apps and that tokens *"replace themselves with a fresh token upon every use"*, while the newer [auth code flow page](https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-auth-code-flow) (2026-01-09) says *"Refresh tokens for web apps and native apps don't have specified lifetimes."* Design for: refresh on a schedule, always handle `invalid_grant` by re-authenticating interactively.

⚠️ **OneDrive filename restrictions are a real problem for markdown vaults:** invalid characters `" * : < > ? / \ |`, no leading/trailing spaces, reserved names (`CON`, `PRN`, `AUX`, `NUL`, `COM0-9`, `LPT0-9`, `_vti_`, `desktop.ini`), nothing starting `~$` ([restrictions page](https://support.microsoft.com/en-us/office/restrictions-and-limitations-in-onedrive-and-sharepoint-64883a5d-228e-48f5-b3d2-eb39e07630fa)). Another argument for shipping one opaque archive rather than mirroring the vault tree.

### 2.7 How Tauri 2 apps do OAuth

**`tauri-plugin-oauth`** — ✅ **2.1.0, published 2026-07-07**, 395,589 downloads, dual Apache-2.0/MIT, ~215 stars. ✅ It is [FabianLars/tauri-plugin-oauth](https://github.com/FabianLars/tauri-plugin-oauth), co-authored with Cuervolu, and **is maintained as of mid-2026** (a publish two months ago, plus an open Tauri v3 compatibility roadmap issue). It is a small single-maintainer **community** plugin, not an official `tauri-apps` one — factor that into your risk budget. It spawns a temporary localhost server to capture the redirect. ⚠️ **Default is a random free port** — fine for Google and Microsoft, incompatible with Dropbox. ⚠️ Known limitation: with implicit flows it doesn't hand you the URL fragment; irrelevant if you use authorization-code + PKCE, which you should.

**`tauri-plugin-deep-link`** — ✅ **official** Tauri plugin in [tauri-apps/plugins-workspace](https://github.com/tauri-apps/plugins-workspace). Latest stable **2.4.10 (2026-08-31)**; a **3.0.0-alpha.0** landed 2026-09-13 for the Tauri v3 track. 5.6M downloads. Desktop supports custom schemes only. ⚠️ **macOS:** *"Registration requires the bundled application installed in `/Applications`. Testing deep links is only possible with installed builds; runtime registration is not supported"* — you cannot test deep-link OAuth in `tauri dev` on macOS. ⚠️ **AppImage:** *"When AppImages relocate on Linux, absolute path-based deep links become invalidated."*

**Recommendation: use `tauri-plugin-oauth` (loopback + PKCE) on all three desktop platforms**, with fixed ports registered for Dropbox. It is RFC 8252's recommended pattern, it is what Google documents for Desktop clients, Microsoft ignores the port so registration is trivial, and it avoids every deep-link caveat above. Open the authorize URL in the **system browser** via `tauri-plugin-shell`, never a webview — Dropbox requires it and Google blocks embedded webviews outright.

---

## 3. Vendor-hosted object storage we would run

All prices fetched 2026-09-16, USD unless marked. AWS figures come from the bulk pricing API (`publicationDate: 2026-09-11`) rather than the JS-rendered pricing page.

### 3.1 Pricing

**Cloudflare R2** — [developers.cloudflare.com/r2/pricing](https://developers.cloudflare.com/r2/pricing/), page footer *"Last updated Aug 7, 2026"*:

| | Standard | Infrequent Access |
|---|---|---|
| Storage | **$0.015/GB-mo** | **$0.010/GB-mo** |
| Class A (PUT/POST/LIST/COPY) | **$4.50/million** | **$9.00/million** |
| Class B (GET/HEAD) | **$0.36/million** | **$0.90/million** |
| Egress | **Free** | **Free** |
| Retrieval | — | $0.01/GB |
| Min duration | none | 30 days |

Free tier (Standard): 10 GB-month, 1M Class A, 10M Class B. ⚠️ Once lifecycled to IA an object **cannot be lifecycled back** — you must rewrite it via `CopyObject`.

**Backblaze B2** — [backblaze.com/cloud-storage/pricing](https://www.backblaze.com/cloud-storage/pricing), ⚠️ **no last-updated date on the page**:
- Storage **$6.95/TB/month = $0.00695/GB-month**; first 10 GB free
- Egress **free up to 3× average monthly stored data**, then $0.01/GB
- **Class A, B and C API calls are FREE for pay-as-you-go customers** — only Class D (event notifications) is billed at $0.004/10,000
- No minimum file size, no minimum duration
- ⚠️ **Free transactions are a major departure from B2's historical pricing** (Class B was $0.004/10k, Class C $0.004/1k). Stated plainly on the live page but **no announcement post or effective date found**, and the qualifier *"for pay-as-you-go customers"* may not cover committed plans.

**Tigris** — [tigrisdata.com/pricing](https://www.tigrisdata.com/pricing/), ⚠️ no date: Standard **$0.02/GB-mo**, IA $0.01, Archive $0.004 (90-day min), Archive Instant Retrieval $0.004 + $0.03/GB retrieval. Egress **zero**. Class A **$5/million**, Class B **$0.50/million**. Free tier 5 GB / 10k / 100k. ⚠️ **There is no "accelerated" tier** — the docs list only Standard / IA / Archive / Archive Instant Retrieval. $25M Series A announced 2025-10-08, led by Spark with a16z; runs on Fly.io infrastructure.

**Hetzner Object Storage** — [hetzner.com/storage/object-storage](https://www.hetzner.com/storage/object-storage/), prices extracted from the page's embedded JSON (they're JS-injected), all ex-VAT:
- Base **€6.49 / $7.99/month** including 1 TB storage + 1 TB egress; billed hourly, **charged for every hour you have ≥1 bucket, even an empty one**
- Additional storage **€6.26 / $8.86 per TB/month**; additional egress **€1.00 / $1.20 per TB**
- **All S3 API calls free.** Ingress free.
- **Minimum billable object size 64 KB.** Limits: 100 buckets/account, 100 TB and **50M objects per bucket**
- Regions: Falkenstein, Nuremberg, Helsinki — EU only

**AWS S3 (us-east-1)** — bulk pricing API, `publicationDate 2026-09-11`:
- Storage/GB-mo: Standard **$0.023** (first 50 TB) · Standard-IA **$0.0125** · One Zone-IA $0.010 · Glacier Instant Retrieval **$0.004** · Glacier Flexible **$0.0036** · Deep Archive **$0.00099**
- Requests: Standard PUT **$0.005/1,000**, GET **$0.0004/1,000**; Standard-IA $0.01 / $0.001; Glacier IR $0.02 / $0.01
- **Lifecycle transitions**: to IA/One Zone-IA/Intelligent-Tiering **$0.01/1,000**; Glacier IR $0.02; Glacier Flexible $0.03; **Deep Archive $0.05/1,000**
- Egress **$0.090/GB** for the first 10 TB. The API's own dimension description reads *"first 10 TB / month data transfer out **beyond the global free tier**"*, which supports the 100 GB/month free egress applying to all accounts (distinct from the new-account 12-month tier, which AWS replaced with a $200 credits model on 2025-07-15)
- **Minimums:** Standard-IA/One Zone-IA 128 KB and 30 days; Glacier IR 128 KB and 90 days; Glacier Flexible 90 days; Deep Archive 180 days

### 3.2 S3 API compatibility

| Feature | R2 | B2 | Tigris | Hetzner | AWS |
|---|---|---|---|---|---|
| Multipart | ✅ | ✅ | ✅ | ✅ | ✅ |
| Conditional writes (If-Match/If-None-Match) | ✅ | ❓ **unverified** | ✅ | ⚠️ partial | ✅ |
| Presigned URLs | ✅ (no POST) | ✅ (no browser POST) | ✅ | ❓ unverified | ✅ |
| Object versioning | ❌ **not implemented** | ✅ on by default | ❌ **not implemented** | ✅ | ✅ |
| Lifecycle rules | ✅ | ⚠️ hide/delete only | ✅ | ⚠️ partial | ✅ |
| SSE-C | ✅ | ✅ (SSE opt-in) | ❌ | ✅ | ⚠️ **off by default since Apr 2026** |

Gaps that matter for a backup product:
- **R2 has no object versioning**, so no S3-versioning-based accidental-delete protection — you implement generation numbering in key names. R2 also lacks tagging and ACLs. ⚠️ R2 multipart quirk: **all parts except the last must be the same size** (S3 allows varying); min 5 MiB, max 5 GiB, 10,000 parts; incomplete uploads auto-expire after 7 days. Presigned URLs only work on `<ACCOUNT_ID>.r2.cloudflarestorage.com`, not custom domains.
- **B2's "lifecycle rules" are not S3 lifecycle rules** — they're B2-native `daysFromUploadingToHiding`/`daysFromHidingToDeleting`, applied once daily, no class transitions. ❓ **B2 conditional writes could not be verified from any Backblaze page.** This is the most important open item in the table: a provider that *silently ignores* `If-None-Match` is worse than one that rejects it, because a compare-and-swap would appear to succeed while losing a concurrent write. **Test empirically before depending on it.**
- **Hetzner's unsupported list is long**: Tagging, Replication, Restore, Intelligent Tiering, GetObjectAttributes, copy of SSE-C objects, conditional PUT/DELETE on versioned buckets, and *"CopyObject may fail, even if the Buckets are in the same location."*
- ✅ **AWS disabled SSE-C by default for new buckets in April 2026**: *"all new general purpose buckets have SSE-C encryption disabled for all new write requests… applications that need SSE-C must deliberately enable SSE-C by using the `PutBucketEncryption` API operation after creating a new bucket"* ([S3 user guide](https://docs.aws.amazon.com/AmazonS3/latest/userguide/serv-side-encryption.html)).

**Cross-cutting landmine:** since December 2024 AWS SDKs default `request_checksum_calculation` and `response_checksum_validation` to `WHEN_SUPPORTED`, emitting CRC32 trailers on every upload. This broke non-AWS providers — R2 returned *"Header 'x-amz-checksum-algorithm' with value 'CRC32' not implemented"* and B2 returned *"Unsupported header 'x-amz-checksum-crc32'"*. Set both to **`WHEN_REQUIRED`** ([AWS SDK reference](https://docs.aws.amazon.com/sdkref/latest/guide/feature-dataintegrity.html)). ⚠️ B2 reportedly added support in July 2025; R2's current status is unverified — assume you still need the override.

### 3.3 Rust SDK options

| Crate | Version | License | Downloads (90d) | Verdict |
|---|---|---|---|---|
| **`object_store`** | **0.14.2** (2026-09-15) | MIT/Apache-2.0 | 19.2M | ✅ **Best fit.** ASF governance (`apache/arrow-rs-object-store`), MSRV 1.85. `put`/`get`/`put_multipart`/`list`/`delete`, presigning, conditional put. **Already models S3-compatible quirks** — `AmazonS3Builder::with_url()` accepts R2 URLs including jurisdiction forms, and there's a `with_disable_bulk_delete()` "because the bulk DeleteObjects API is not implemented by all S3-compatible providers." `reqwest`/`tokio`/crypto backend are feature-gated, so it slims down well. Only gap is no SSE-C — irrelevant if you encrypt client-side. |
| `aws-sdk-s3` | **1.146.1** (2026-09-11) | Apache-2.0 | 20.1M | Full fidelity, every knob, but heaviest by far (1.81 MB source crate plus `aws-config` and the whole smithy tree). MSRV 1.94.1. ⚠️ **1.147.0 was published 2026-09-15 and is YANKED — pin 1.146.1.** |
| `opendal` | **0.59.2** (2026-09-15) | Apache-2.0 | 4.2M | Mature, dozens of backends — good if you later want "bring your own storage of any kind." Heavier and higher MSRV (1.91) than needed for PUT/GET/multipart. |
| `rust-s3` | 0.37.2 (2026-05-04) | MIT | 1.5M | ❌ **Avoid.** Single maintainer; the README says verbatim *"As many have noted my attention to the this library oscialtes [sic], the best way to ensure I keep maintaining and adding features to it is to donate som BTC."* 4-month commit gap, 13 open PRs. Wrong risk profile for a path where silent data loss is the failure mode. |
| `s3s` | 0.16.0 | Apache-2.0 | — | For *implementing* an S3 server, not consuming one. Useful as a local fake for integration tests. |
| `reqsign` | 0.20.6 | Apache-2.0 | — | SigV4 signing only. Plausibly the smallest-binary option if you hand-roll over `reqwest`, at the cost of writing multipart yourself. |

### 3.4 Cost model — 10,000 users × 200 MB = 2 TB

Assumptions: 2,000 GB stored; incremental backup at **50 objects/user/day = 15M PUTs/month**; 1.5M GETs/month; 100 GB egress/month (5% of users restoring 200 MB).

| Vendor / config | Storage | Writes | Reads | Egress | **Total/mo** |
|---|---|---|---|---|---|
| **Backblaze B2** | $13.90 | free | free | free (3× allowance = 6 TB) | **$13.90** |
| **Hetzner** | $16.85 | free | free | included in 1 TB | **$16.85** |
| **Tigris IA** | $20.00 | $75.00 | $0.75 | $1.00 | **$96.75** |
| **R2 Standard** | $30.00 | $67.50 | $0.54 | $0 | **$98.04** |
| **Tigris Standard** | $40.00 | $75.00 | $0.75 | $0 | **$115.75** |
| **AWS S3 Standard** | $46.00 | $75.00 | $0.60 | $0 (under 100 GB free) | **$121.60** |
| **R2 Infrequent Access** | $20.00 | $135.00 | $1.35 | $1.00 | **$157.35** |
| **S3 Std → IA lifecycle** | $25.00 | $225.00 | $1.50 | $1.00 | **$252.50** |
| **S3 Glacier IR direct** | $8.00 | $300.00 | $15.00 | $3.00 | **$326.00** |

Worked example for R2 Standard: `2,000 × $0.015 = $30.00` + `15,000,000 ÷ 1e6 × $4.50 = $67.50` + `1,500,000 ÷ 1e6 × $0.36 = $0.54` + `$0` egress = **$98.04**.

**Which model punishes which behavior:**
- **At 50 small objects/user/day, request pricing dominates everywhere except B2 and Hetzner.** Storage is $14–$46; the writes are $67–$300.
- **R2's IA tier is a trap for incremental backup** — it doubles Class A ops to $9.00/million, so **R2-IA costs more than R2-Standard** ($157 vs $98). You save $10 on storage and pay $67.50 more in writes.
- **AWS lifecycle transitions are billed per object.** Moving 15M objects/month to Standard-IA costs **$150 — six times the $21 storage saving.** Deep Archive at $0.00099/GB looks irresistible ($1.98/month for 2 TB) until you price the transitions at $0.05/1,000 = **$750/month**, on top of a 180-day minimum duration.
- **AWS's 128 KB minimum billable object size is the second trap.** 32 KB average objects bill as 128 KB — 4× storage inflation, wiping out the class discount. Hetzner's 64 KB minimum is a milder version.
- **B2 punishes egress, not requests.** It is essentially insensitive to this access pattern; its exposure is restores exceeding 3× stored data.
- **Hetzner punishes aggregate object count** — 50M objects per bucket means 50 objects/user/day retained for a year forces sharding.

**The number that actually matters — batch your writes.** Two packfiles per user per day (600k PUTs/month, restic-style) instead of 50 loose objects:

| R2 Standard | Tigris Standard | AWS S3 Standard | B2 |
|---|---|---|---|
| **$32.72** | **$43.03** | **$49.02** | **$13.90** |

R2 drops $98 → $33; AWS $122 → $49. **Architecture matters more than vendor choice.** Per-user economics land between $0.0014 and $0.033/user/month across every option — storage will not make or break this product.

### 3.5 Compliance posture

| | R2 | B2 | Tigris | Hetzner | AWS |
|---|---|---|---|---|---|
| Self-serve DPA | ✅ v6.4, 2026-04-03 | ⚠️ **2022 text** | ✅ 2025-11-18 | ✅ checkbox in console | ✅ Service Terms §1.14.1 |
| EU residency | `eu` jurisdiction | EU Central AMS, account-locked | fra/ams opt-in | **DE/FI only** | 7 EU regions + Sovereign Cloud |
| Jurisdiction | US | US | US | **EU (DE)** | US |
| Default at-rest encryption | ✅ | ❌ opt-in | ✅ | ❌ **none** | ✅ |
| ISO 27001 | ✅ | ⚠️ data centres only | ❌ | ✅ **+ BSI C5 Type 2** | ✅ |
| Transparency report / canary | ✅ **both** | ❌ | ❌ | ❌ | ⚠️ unfetchable |

- **Hetzner is the strongest EU story**, not a compromise: only EU-domiciled vendor, ISO 27001:2022 (SOCOTEC, all DCs) plus BSI C5 Type 2, self-serve DPA at [accounts.hetzner.com/account/dpa](https://accounts.hetzner.com/account/dpa) (*"A handwritten signature is not required"*), and the strongest transfer clause: *"Any transfer to a third country will require the prior consent of the Client"* — with **no** law-enforcement carve-out. Compare AWS DPA §12.1, which does have one: *"AWS will not transfer Customer Data from Customer's selected Region(s) except as necessary to provide the Services… or as necessary to comply with the law or valid and binding order of a governmental body."* ⚠️ Hetzner's cost: **no default at-rest encryption** (*"There is no default data-at-rest encryption of objects"* — fine when you encrypt client-side, but don't misdescribe it) and **single-DC buckets with no cross-DC redundancy**, so you own replication.
- **Cloudflare has the best privacy posture among US vendors** — a real warrant canary (*"Cloudflare has never turned over our encryption or authentication keys or our customers' encryption or authentication keys to anyone"*), a semiannual [transparency report](https://www.cloudflare.com/transparency/), and a DPA commitment to *"pursue legal remedies prior to producing Personal Data up to an appellate court level."* ⚠️ Note R2's `eu` jurisdiction is **not** the Enterprise-only Data Localization Suite — don't conflate them in marketing copy.
- **🔴 Tigris is the one to rule out for a privacy-marketed product.** Its DPA Appendix C lists **every sub-processor as United States** (Equinix, Oracle Cloud, Fly.io, AWS, Slack, Plain, Stripe, Loops) while it sells `fra` and `ams` regions. Its DPA Appendix B security measures contain **no encryption obligation whatsoever**. Its own docs say globally-distributed buckets are *"not suitable for strict data residency requirements"*, and Appendix A says the service *"is not intended to meet any legal obligations for any Sensitive data."* Plus Series-A vendor-continuity risk.
- **Backblaze wins on cost but is hard to defend on a privacy page**: 2022-dated DPA, opt-in encryption, ISO 27001 by subcontractor only, account-locked regions, and a requirement for *"valid and legally-binding U.S. legal process"* — including, on its face, for Amsterdam-stored data. Its DPA also applies **SCC Module One (controller-to-controller)** to individuals, i.e. Backblaze positions itself as a joint controller rather than a processor. A reviewer will notice.

---

## 4. End-to-end encryption for backups

### 4.1 The `age` ecosystem — good design, wrong container for you

✅ `age` (library) **0.12.1, 2026-07-14**, MIT OR Apache-2.0, MSRV 1.74, maintained by str4d at [github.com/str4d/rage](https://github.com/str4d/rage) (3,656 stars, last push 2026-08-20). `rage` CLI same version. Go reference impl v1.3.2 (2026-08-29). 0.12.0 added post-quantum recipients (`age::tag`, `age::tagpq`) pulling in `ml-kem` and `hpke`.

⚠️ **The crate self-describes as BETA**: *"all crate versions prior to 1.0 are beta releases for testing purposes only."* This has been true for years and is arguably over-cautious at 4.68M downloads, but it's the vendor's own language.

The **STREAM** payload design is worth copying: 64 KiB chunks, ChaCha20-Poly1305 per chunk, nonce = 11-byte big-endian counter + a final byte `0x01`/`0x00` marking the last chunk, payload key = `HKDF-SHA-256(ikm=file key, salt=nonce, info="payload")` ([c2sp.org/age](https://c2sp.org/age)). ✅ **Seekable decryption works** — `impl<R: Read + Seek> Seek for StreamReader<R>`.

**🔴 The finding that rules age out as your envelope:** the spec states *"an scrypt stanza, if present, MUST be the only stanza in the header."* **You therefore cannot make one age file openable by either a passphrase or an X25519 recovery key** — which is exactly the multi-slot recovery design you want (§4.4).

Other critiques worth knowing: no sender authentication ([Filippo's own post](https://words.filippo.io/age-authentication/), 2022-09-29); recipient stanzas aren't individually authenticated, only the whole header via HMAC-SHA-256; no built-in compression (deliberate, to avoid CRIME/BREACH); 128-bit file key. **`age` has 24 required direct deps** including `i18n-embed` and `rust-embed` localization machinery — heavy for embedding.

⚠️ **No security audit of age or rage exists that I could find.** There is one advisory: **RUSTSEC-2024-0433 / GHSA-4fg7-vxc8-qx5w** (2024-12-18, code-execution) — malicious plugin names could cause arbitrary binary execution via `age::plugin::Identity::from_str`; affected 0.6.0–0.11.0, fixed by restricting plugin names. Only reachable with the plugin feature enabled.

⚠️ The age format spec is versioned `age@v1.0.0` in the C2SP registry and stable in practice since 2021, but **no explicit freeze declaration exists** and no v2 work was found.

### 4.2 RustCrypto — the 1.0-track releases landed mid-2026

**🔑 If you have notes from before mid-2026, they're wrong.** After years on `aead` 0.5 / `chacha20poly1305` 0.10, the rewrite shipped:

| Crate | Version | Released | MSRV |
|---|---|---|---|
| `aead` | **0.6.1** | 2026-06-17 | 1.85 |
| `chacha20poly1305` | **0.11.0** | 2026-06-28 | 1.85 |
| `aes-gcm` | **0.11.1** | 2026-08-21 | 1.85 |
| **`aead-stream`** | **0.6.0** | 2026-06-28 | 1.85 |
| `argon2` | **0.6.0** | 2026-08-27 | 1.85 |
| `scrypt` | **0.12.0** | 2026-04-22 | 1.85 |
| `aes` | **0.9.3** | 2026-08-28 | 1.89 |
| `zeroize` | **1.9.0** | 2026-06-12 | 1.85 |

The RC churn (`-rc.0` through `-rc.10`, May 2025 onward) is over. **`chacha20poly1305` has 4 required direct deps** vs age's 24.

**`aead-stream` 0.6.0 is new and directly relevant** — a standalone implementation of the STREAM construction from Hoang–Reyhanitabar–Rogaway–Vizár (2015), with `StreamBE32`/`StreamLE31`, defending against reordering and truncation. **This is the right primitive if you build your own envelope.**

**⚠️ The NCC Group audit is from 2020, not 2023.** I retrieved the PDF: *RustCrypto AES/GCM and ChaCha20+Poly1305 Implementation Review*, sponsored by MobileCoin, **2020-02-13**, by Gérald Doussot and Thomas Pornin, **two consultants, five person-days**. Key finding: *"NCC Group did not find any vulnerability in the audited crates."* Live copy is the [Wayback link](https://web.archive.org/web/20240108154854/https://research.nccgroup.com/wp-content/uploads/2020/02/NCC_Group_MobileCoin_RustCrypto_AESGCM_ChaCha20Poly1305_Implementation_Review_2020-02-12_v1.0.pdf) referenced from the aes-gcm README. **🔴 It is 6½ years old and reviewed crates that no longer exist** (`aesni`, `aes-soft`, `stream-cipher`). Treat it as evidence of engineering culture, not assurance for today's code.

Advisories: `aes-gcm` **RUSTSEC-2023-0096 / CVE-2023-42811** (plaintext exposed in `decrypt_in_place_detached` on tag failure) affects ≥0.10.0 <0.10.3 — not an issue on 0.11.1. No advisories for `chacha20poly1305`, `argon2`, `scrypt`, `fastcdc`.

**Argon2id parameters.** OWASP's [cheat sheet](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html) recommends `m=47104 (46 MiB), t=1, p=1` down to `m=7168, t=5, p=1`. ⚠️ **These are server-side password-storage numbers and are the wrong target for you** — OWASP optimizes for a server hashing thousands of logins per second; you derive a KEK once per unlock on the user's own laptop. RFC 9106 §4's first recommendation is **m=2 GiB, t=1, p=4**. **Suggested: m=256 MiB, t=3, p=4, calibrated at setup to ~1s on the user's machine, with the actual parameters stored in the header** so you can raise them later and re-wrap transparently. Cap memory on low-RAM machines. Use `hash_password_into` for raw KEK bytes, not the PHC-string `PasswordHasher` API. ⚠️ The OWASP page carries no version or date stamp.

**Alternatives:** `orion` 0.18.0 is extremely lean (2 deps) but its README says verbatim *"This library has not undergone any third-party security audit. Usage is at own risk."* `dryoc` hit **1.0.0 (2026-07-22)** — pure-Rust libsodium reimplementation with `mlock`ed memory types. **`sodiumoxide` is ✅ confirmed deprecated** — RUSTSEC-2021-0137, last release 2021-06-24. `ring` 0.17.14's unmaintained advisory (RUSTSEC-2025-0007) was **withdrawn two days later** when the rustls team got access.

**Verdict: `chacha20poly1305` + `argon2` + `aead-stream` + `zeroize`, all RustCrypto, all pure Rust, no C toolchain** — which keeps Tauri cross-compilation painless.

### 4.3 Chunking and dedup — skip it

✅ `fastcdc` **5.0.0 (2026-08-22)**, MIT, [nlfiedler/fastcdc-rs](https://github.com/nlfiedler/fastcdc-rs), 208 stars, actively maintained. Three modules: `v2016` (original FastCDC), `v2020` (improved, 64-bit hashes — **docs recommend this**), `ronomon`. Streaming chunkers plus optional async. ⚠️ `cdchunking` last published 2020 — unmaintained. ⚠️ The crates.io crate named **`desync` is an unrelated async library**, not casync-in-Rust; `folbricht/desync` is Go.

✅ `rustic_core` **0.13.0 (2026-08-16)**, Apache-2.0/MIT, and it **is** a real library that reads and writes the restic repository format, with `LocalBackend`/`OpenDALBackend`/`RcloneBackend`/`RestBackend` in `rustic_backend` 0.7.0. But the caveats are serious:
- **README warning, verbatim: *"`rustic_core` is in an early development stage and its API is subject to change in the next releases."***
- **Twelve breaking changes across four minor releases in six months** (0.10.0 with 8, then 0.11.0, 0.12.0, 0.13.0). No 1.0 roadmap.
- **MSRV 1.91.0** on `rustic_backend` — very aggressive, and it constrains your whole workspace.
- **46 required direct deps** including `rayon`, `jiff`, `nix`, `xattr`, `binrw`, `ignore`, `zstd`, plus opendal and tokio transitively. ❌ No published binary-size or compile-time numbers.

✅ **restic's crypto, confirmed** from [the design reference](https://restic.readthedocs.io/en/latest/100_references.html): **scrypt** with per-keyfile salt derives 64 bytes → AES-256 key + Poly1305-AES key material; **AES-256-CTR with Poly1305-AES** (the original Bernstein construction, *not* ChaCha20-Poly1305); `IV || CIPHERTEXT || MAC`, 32 bytes overhead; 16 random IV bytes per file. CDC uses **Rabin fingerprints, 64-byte window, min 512 KiB / avg 1 MiB / max 8 MiB**, with a randomly chosen irreducible polynomial stored in the config *"so that watermark attacks are much harder."* Repo format v2 added zstd compression.

**✅ The cryptanalysis exists, and the paper the brief half-remembered does not.** There is **no paper titled "A Formal Analysis of Backup Encryption"** — do not cite that. The two real sources are:

**(a) Alexeev, Percival & Zhang, "Chunking Attacks on File Backup Services using Content-Defined Chunking," arXiv:2504.02095, 2025-04-02** ([link](https://arxiv.org/abs/2504.02095)) — note **Colin Percival attacking his own system**. It extracts the secret chunking parameters of Tarsnap (~2⁴²), Borg (~2⁴²) and **restic (~2⁴⁴ with minimal data requirements)**, turning a keyed chunker into a known function. After that, chunk sizes alone let an attacker identify specific files, infer private edits to large known files, and run inference attacks. Money quote: *"content-defined chunking systems are essentially using computationally efficient (instead of cryptographically secure) hash functions as a sort of pseudo-encryption."*

**(b) L. Schenck, "(Secure?) Cloud Backup Solutions: A Survey," ETH Zürich, 2023-08-28** ([PDF](https://ethz.ch/content/dam/ethz/special-interest/infk/inst-infsec/appliedcrypto/education/theses/bachelors-thesis_lucas-schenck.pdf)), advised by Kenny Paterson. ⚠️ **This is a Bachelor's thesis, not peer-reviewed.** Confidentiality and integrity hold everywhere, but **fingerprint resistance fails in Borg, Kopia and restic** under a weak adversary. ⚠️ It claims restic uses no chunking secret, which contradicts restic's own docs and the Alexeev paper — unresolved.

**🎯 Is CDC worth it for a ~200 MB SQLite file? No.**

SQLite stores the database as a fixed array of pages (default 4096 bytes). An UPDATE rewrites **whole pages in place at fixed byte offsets**; a B-tree split allocates a new page **at the end of the file** without shifting existing offsets. A day of edits produces a **scattered set of modified 4 KiB pages at stable offsets**, not an insertion that slides the rest of the file. CDC's entire purpose is resilience to byte-offset shifts, which SQLite does not produce. **Fixed-size blocks aligned to a multiple of the page size dedup just as well, at a fraction of the CPU, with no rolling hash and no secret to leak.**

Worse, **restic's 512 KiB minimum chunk means one changed 4 KiB page dirties ≥512 KiB — a 128× write amplification.** Its CDC parameters are actively badly matched to SQLite. `VACUUM` and `auto_vacuum` are the exceptions, and they defeat CDC equally.

The supporting data point is `sqlite3_rsync`: page-hash exchange, no CDC anywhere, **500 MB database syncing in ~20 KB**. ⚠️ **Honest gap: no published benchmark of CDC dedup ratios on SQLite files was found** — the argument is mechanistic plus that data point.

**And the decisive privacy argument: CDC is a documented fingerprinting side channel.** For a product whose pitch is local-first and privacy-focused, deliberately adding a known side channel to save bandwidth you can save better with page-aligned blocks is a bad trade. If you ever host the storage yourself, you'd be building the oracle *and* holding it.

**Recommendation:**
1. **Exclude the embedding/vector tables entirely.** Re-derivable by definition, likely most of the 200 MB, and it costs one exclusion rule plus a rebuild step on restore. Back up the source documents and the settings needed to regenerate.
2. **Fixed 64 KiB blocks aligned to the page size**, content-addressed by BLAKE3, deduped against the previous snapshot.
3. **Whole-file, content-addressed** for markdown and PDFs. CDC buys nothing there either.
4. **Compress before encrypting** (zstd level 3) and **pad to fixed size buckets** to blunt size-based inference.

### 4.4 Key-recovery UX

**What the market actually does:**

| Product | Escrow? | Notes |
|---|---|---|
| Obsidian Sync | None | ✅ AES-256-GCM + scrypt + HKDF. *"We're not able to recover your password, or any encrypted data for you."* |
| Joplin | None | *"cannot be recovered."* ⚠️ KDF not publicly documented |
| Standard Notes | None | *"password resets are simply not possible."* XChaCha20-Poly1305 + Argon2id (m=64 MiB, t=5, p=1) |
| Signal account PIN | **Yes** — enclave-escrowed, guess-limited | |
| Signal Secure Backups | **No** | *"Losing it means losing access to your backup permanently."* |
| 1Password | No for password/Secret Key; **yes** for vault keys | Shipped recovery codes in 2024 |
| Apple ADP | No keys held; **mandatory** recovery contact or key | |

**1Password's 2SKD**, ✅ verified from the [white paper PDF](https://1passwordstatic.com/files/security/1password-white-paper.pdf) (⚠️ PDF CreationDate **2024-12-10**, ~21 months stale): Secret Key is 26 random chars from a 31-char alphabet ≈ 2¹²⁸·⁸. The derivation: password NFKD-normalized → salt stretched with **HKDF salted by the lowercased email** → **PBKDF2-HMAC-SHA256, 650,000 iterations** → 32 bytes; Secret Key → HKDF (salt = account ID) → 32 bytes; **XOR** → the Account Unlock Key. They chose PBKDF2 *"largely a function of there being (reasonably) efficient implementations available for all our clients"* — a **browser-JavaScript** constraint Lattice does not have. Their candid admission about the Emergency Kit: *"It's a challenge for us to find ways to encourage people to print and save their Emergency Kits."*

**The most telling datapoint in the whole survey:** 1Password shipped **recovery codes on 2024-06-20**, and framed it as *"before recovery codes, if Family Organizers and customers using 1Password Individual forgot their password or lost their Secret Key, even with 1Password Support, they wouldn't be able to regain access."* A vendor admitting the pure "we can never recover" model was failing real users. The mechanism: 32 CSPRNG bytes, client-generated, stored server-side wrapped under the user's key-set key, with **HKDF yielding three subkeys** — `_AUTH_v1` (SRP-x), `_ENC_v1` (wraps the key-set key), `_UUID` (16-byte non-secret identifier).

**Apple ADP** ✅ ([support.apple.com/en-us/102651](https://support.apple.com/en-us/102651)) is a *deletion*, not an addition — it removes the CloudKit service keys from Apple's HSMs, *"immediate, permanent, and irrevocable."* **🎯 The best UX idea found: Apple gates the privacy feature on the recovery artifact existing** — *"you'll be guided to set up at least one recovery contact or recovery key before you turn on Advanced Data Protection."* Recovery contacts are an elegant **2-of-2 split between Apple and your friend** requiring no Shamir: a random 256-bit AES key encrypts a Recovery Contact Packet; the packet goes to the contact, the key stays with Apple, and *"Neither the AES key nor the packet provides any information about the underlying key by itself."* Recombination via SPAKE2+ bootstrapped by a spoken code.

**Signal SVR** ✅: SVR2 ([repo](https://github.com/signalapp/SecureValueRecovery2), AGPL-3.0, SGX, audited by NCC Group) limits *"the number of attempts to recover such a secret to a very small guess count,"* with design philosophy *"if there is a choice between 'lose the secret material forever' and 'store the secret material but potentially leak it', we'll choose the former."* SVR3 (OSDI '24, [eprint 2024/887](https://eprint.iacr.org/2024/887.pdf)) splits trust across **three enclave types on three clouds** using Password-Protected Secret Sharing — $0.0025/user/year, 365 ms recovery. ⚠️ **No 2025–2026 source confirms SVR3 is fully in production.** **The instructive contrast inside one product:** Signal applies enclave-escrowed guess-limited recovery to the *account*, and zero recovery to *backups*.

**Mnemonics and Shamir in Rust:** ✅ **use `bip39` 2.2.2 (2025-12-04, CC0-1.0, 16.2M downloads, `no_std`, `zeroize` feature)** — not `tiny-bip39` (dormant ~2 years, v1.0.1 yanked). **🔴 There is no maintained, audited SLIP-39 implementation in Rust.** The only crate claiming SLIP-0039 compatibility is `slip39` 0.1.1, **last published 2020-01-30**, 3,122 lifetime downloads, **GPL-3.0-or-later** — a licensing problem on top of abandonment. `sharks` is dead (2021); `vsss-rs` 6.0.1 is maintained but is a verifiable-secret-sharing toolkit, far heavier than "split 32 bytes."

**The one piece of hard UX data that exists:** Bailey, Markert & Aviv, *"I have no idea what they're trying to accomplish": Enthusiastic and Casual Signal Users' Understanding of Signal PINs*, **USENIX SOUPS 2021, n=235** ([PDF](https://gwusec.seas.gwu.edu/understanding-of-signal-pins/soups21-102-signal-pins.pdf)):
- **44% could not explain the purpose of the PIN at all**
- **14% opted out**; among casual non-setters, **24% declined based on an inaccurate understanding**
- Only 12% reported difficulty remembering it — **but 67% of enthusiasts who disabled reminders did so because they use a password manager**, and password-manager users chose PINs averaging +2.1 digits and +5.3 letters. **The people who don't lose the secret are the people already running a password manager.**

⚠️ **No vendor publishes lost-passphrase ticket volumes.** This 5-year-old study is the only quantitative source; everything else is revealed preference.

**🎯 Recommendation: a LUKS-style multi-slot key envelope with two slots at v1 — passphrase and recovery code. Make the recovery code mandatory, Apple-style. Do not ship Shamir. Do not ship vendor escrow.**

- **One random 256-bit data key `DK`**, never leaving the device unwrapped. Per-object subkeys via `HKDF-SHA256(DK, salt, info)`. Content encryption **XChaCha20-Poly1305** chunked via `aead-stream`'s STREAM so truncation and reordering are detected.
- **Slot 0 (passphrase):** `KEK = Argon2id(passphrase, salt₀, m=256MiB, t=3, p=4)`, params in the header, wrap `DK` with the **entire header as AAD**.
- **Slot 1 (recovery code):** 256 CSPRNG bits, not derived from anything. Because it's already full-entropy, **skip Argon2** — `HKDF-SHA256(ikm=recovery_entropy, salt=salt₁, info="lattice/recovery/v1")`, exactly 1Password's `_ENC_v1` shape. Derive a 16-byte non-secret slot ID too, so the UI can say "recovery code #2, generated 2026-03-11" without holding the secret.
- **Encoding:** BIP-39, 24 words — the checksum catches typos *before* a failed decryption.
- **Key check value:** `truncate(HMAC-SHA256(DK, "lattice/kcv/v1"), 8)` in the header, so you can distinguish "wrong passphrase" from "backup corrupt." Eliminates a whole class of support tickets.
- **Store the header in three places:** in the backup, in the local app dir, and as a QR code in the Emergency Kit.
- **Re-wrap, never re-encrypt.** Changing the passphrase touches slot 0 only — unwrap `DK`, derive new KEK, re-wrap, write ~100 bytes. This is why the recovery code must be a *slot*, not the data key itself; Signal made the opposite choice and rotating their 64-char key means re-uploading everything.

**UX, which matters more than the crypto:**
1. **Copy Apple: block the feature on the artifact.** No encrypted backups until a recovery code is generated *and confirmed*. Highest-leverage decision here, because the dominant failure is never saving it.
2. **Confirm by re-entering 3 random words**, not a checkbox.
3. **Ship an Emergency Kit PDF** in 1Password's format, including a blank ruled line for the hand-written passphrase.
4. **"Save to password manager" as a first-class button** next to "Print." The SOUPS data is unambiguous about who succeeds.
5. **Generate the passphrase by default** (6-word Diceware). You have no HSM and no server-side guess limiting, so a user-chosen passphrase is your weakest link.
6. **Name it unambiguously** — "Backup encryption passphrase," never "password," kept far from any login UI. The recurring structural failure across every product surveyed is users confusing the encryption secret with the login secret.
7. **Note the genuine advantage you have:** in every Obsidian/Joplin/Standard Notes forum thread, the resolution is *"are you still signed in on another device?"* — the local copy is the de facto recovery mechanism. Lattice is local-first, so this safety net is real. Say so in the docs; don't rely on it.

### 4.5 Performance — encryption is a rounding error

Measured on an M3 Max with OpenSSL 3.6.3, `openssl speed -evp`, 16 KiB blocks: **AES-256-GCM 7.91 GB/s**, **ChaCha20-Poly1305 2.04 GB/s**, SHA-256 2.69 GB/s. (AES-GCM is ~3.9× faster where hardware AES exists; the ordering inverts without it.)

✅ **The old aarch64 footgun is fixed.** RustCrypto's `aes` used to need nightly or `--cfg aes_armv8`, so on Apple Silicon you silently got the slow bitsliced fallback. As of **`aes` 0.9.3**: *"On Linux and macOS, support for ARMv8 AES intrinsics is autodetected at runtime"*, and on x86 *"this crate uses runtime detection on i686/x86_64 targets in order to determine if AES-NI and VAES are available."* ⚠️ Verified from docs.rs, not by compiling and profiling — worth a one-time `cargo bench` sanity check.

**The arithmetic for 200 MiB:** 26 ms at 7.9 GB/s, 103 ms at 2.04 GB/s, 419 ms on a weak laptop with no hardware AES. **Upload at a typical 10 Mbps home upstream: 168 seconds.** Encryption is **0.015% of wall-clock — a factor of ~6,500×.** Reading 200 MB from a SATA SSD takes longer than encrypting it.

**Practical consequence: optimize for bytes uploaded (exclude the embedding tables, compress, dedup at page granularity), not for cipher throughput.** Pick the cipher for ecosystem reasons; a 4× difference on a 26 ms operation is 78 ms.

Chunked-AEAD overhead is negligible either way — 64 KiB chunks cost 0.0244% in tag bytes, 1 MiB chunks 0.0015%. **Prefer 64 KiB**: it matches age, bounds memory, and gives finer-grained seek.

---

## 5. How comparable apps handle this

| App | Backup or Sync | Who hosts | E2EE | Default? | Price (2026) | Local backup/export |
|---|---|---|---|---|---|---|
| **Obsidian** | Sync — says explicitly it is *not* backup | Vendor | Yes, AES-256-GCM / scrypt+HKDF | **Default**, non-E2EE mode exists | $4–8/mo annual; 1–10 GB; 1–12 mo history | Vault is plain MD on disk; File Recovery plugin (5 min / 7 days, local only) |
| **Joplin** | Sync | **User's choice** — Cloud, S3, WebDAV, Dropbox, OneDrive, Nextcloud, FS, self-host | Yes | **OFF by default** | €2.99–9.99/mo Cloud; self-host €2.50–3.33/user/mo | JEX export; sync target is user-owned |
| **Anki** | Sync (free) + strong local backup | AnkiWeb or **self-host** | **No** | — | **Free**; 100 MB collection, 100 MB/media file | Auto backup every 30 min; `.colpkg` |
| **Logseq** | Sync/RTC, **invite-only paid beta** | Vendor (+ self-host in development) | Claimed, **no published spec** | Required for synced graphs | ⚠️ unannounced | Auto local backups, last 12; EDN export |
| **Bear** | Sync only | **User's own iCloud** (CloudKit) | **Per-note**, AES-256-GCM/Themis | Opt-in per note; Pro only | **$2.99/mo or $29.99/yr**, no storage tiers | `.bear2bk`; **manual only, destructive restore** |
| **Notion** | Sync, server-authoritative | Vendor (AWS) | **No** — structurally can't | — | $0/$10/$20 per seat/mo; **7/30/90-day** history | HTML/MD/CSV export, 7-day link, **no automation** |
| **Apple Notes** | Sync only | Apple | Only with **ADP** (opt-in); locked notes always (AES-128-GCM/PBKDF2) | **No** | iCloud+ $0.99–$59.99/mo | MD export since macOS 26; **loses folder structure** |
| **Standard Notes** | Sync + history | Vendor + **self-host** | Yes, XChaCha20-Poly1305 / Argon2id | **Default** | $0 / $90 / $120 per **year** | **Daily encrypted email backups even on free** |
| **Cryptomator** | Crypto layer over user's cloud | **User's own cloud** | Yes, AES-SIV, scrypt | Default | Desktop **free**; €29.99 one-time per mobile platform | N/A — it wraps what the user already has |
| **Backblaze Personal** | **Backup only** | Vendor | Optional private key ⚠️ | No | **$9/mo, $99/yr, $7.88/mo 2-yr** per computer | Restore by download or mailed drive |
| **Arq** | **Backup only** | **User's own** storage | Yes, client-side | Default | **$59.99 one-time** or **$69.99/yr w/ 1 TB** | Snapshot-based, BYO destination |
| **Day One** | Sync | Vendor | Yes | **Default for new journals** | **$0 / $49.99 / $74.99 per year** | ⚠️ **Exports are NOT encrypted** |

**Five observations about the market norm:**

**1. Almost nobody sells "backup." They sell sync and hope you conflate the two — and the honest ones say so out loud.** Obsidian's own help page states *"Syncing is not a backup"* and recommends *"a dedicated backup tool that creates a one-way copy of your data"* ([obsidian.md/help/backup](https://obsidian.md/help/backup)). Anki's manual says local backups *"will not protect you if your device breaks or is stolen."* Of twelve products surveyed, only Arq and Backblaze are actually backup products, and neither is a note app. **A first-class, scheduled, restorable, versioned off-device backup for a local-first notes app is genuinely unoccupied ground.**

**2. The apps that achieve real E2EE mostly do it by not hosting anything.** Bear → the user's CloudKit. Cryptomator → the user's Dropbox. Joplin → whatever the user picks. Notion, which *does* host, cannot offer E2EE at all because server-side search, real-time collaboration and AI all need plaintext. **The tradeoff is not negotiable: if you host ciphertext you get no server-side search, no web viewer, no cross-user dedup.** Bear documents exactly this cost — *"full-text search does not work on encrypted notes"* — and notes that ADP would break its planned web app. **Decide whether you ever want a web viewer before choosing option 3.**

**3. "Free hosted sync" is a cache with an expiry date, not durability.** ✅ AnkiWeb: *"If you haven't accessed your account or synced in the last 6 months, the data on your account might get deleted"* and *"once your deck data has expired, it is not possible for us to recover your data from AnkiWeb"* ([docs.ankiweb.net/syncing.html](https://docs.ankiweb.net/syncing.html)). Notion keeps 7 days of history on free; Obsidian Standard keeps 1 month and counts version history against a 1 GB quota. **This is the strongest argument for making Lattice's backup explicitly a backup — named, dated, restorable, with stated retention — rather than a sync feature with an implied safety net.**

**4. Pricing anchors cluster in three shapes, and storage-based is the worst one for you.** Feature-based with zero marginal cost: Bear $2.99/mo, Cryptomator €29.99 once. Storage-based: Obsidian $4–8/mo for 1–10 GB, Joplin €2.99–9.99/mo. Seat-based: Notion $10–20. A knowledge base of markdown is *tiny* — Anki's own calibration is 25,000 cards ≈ 25 MB. **If you host, you'll be selling gigabytes nobody uses while carrying the liability, and anything above ~$3/mo gets compared directly against Bear's entire subscription.**

**5. What users actually complain about, ranked — and this is your feature spec.** (i) **Conflicts and silent data loss from consumer file sync** — iCloud duplicating `Note 2.md`, corrupting `.obsidian/`, and Obsidian's own docs conceding *"iCloud Drive on Windows may lead to file duplication or corruption"*; see e.g. [forum.obsidian.md/t/icloud-data-loss-issue-sync-conflict-handling/113584](https://forum.obsidian.md/t/icloud-data-loss-issue-sync-conflict-handling/113584). (ii) **Paying for a beta that never ships** — the Logseq [HN thread](https://news.ycombinator.com/item?id=48896229) is brutal: *"I donated $15/month… to try Logseq Sync when it was in beta. It's still in beta. What a joke."* (iii) **Portability panic when the format changes** — *"my notes being in plain text is a non-negotiable for me"*, after Logseq 2.0 (2026-07-13) moved the canonical store from markdown files to SQLite. (iv) **Destructive or manual restore** — Bear's restore *"permanently deletes"* all notes including trashed ones, and its backup cannot be scheduled.

**So: conflict-free by construction, ship before you charge, never make the local data less portable, and make restore non-destructive, automatic and previewable.**

Two patterns worth stealing outright:
- **Obsidian's [verify-your-own-encryption post](https://obsidian.md/blog/verify-obsidian-sync-encryption/)** (2023-06-05, updated 2025-09-05) ships console snippets so a user can pull their salt, capture WebSocket frames in DevTools and decrypt them locally. *Let the user verify the claim themselves.*
- **Standard Notes ships daily encrypted email backups even on the free tier** — pushing an encrypted archive *out* to the user rather than only holding it. Cheap, high-trust durability.

One legal datapoint worth internalizing: in January 2025 the UK Home Office served Apple a Technical Capability Notice under s.253 of the Investigatory Powers Act; rather than build a backdoor **Apple withdrew Advanced Data Protection for UK users** and filed at the Investigatory Powers Tribunal ([The Register](https://www.theregister.com/2025/03/05/apple_reportedly_ipt_complaint/), [Privacy International](https://privacyinternational.org/legal-action/pi-apple-tcn-challenge)). ADP remains unavailable to UK users as of September 2026. **"We hold no keys" is a legal posture, not just an engineering one, and jurisdictions will test it.**

---

## Everything I could not verify

**Corrections to premises in the brief:**
1. 🔴 The NCC Group RustCrypto audit is **2020-02-13**, not 2023 — and it reviewed crates that no longer exist.
2. 🔴 **There is no paper titled "A Formal Analysis of Backup Encryption."** The real sources are arXiv:2504.02095 (2025) and an ETH **Bachelor's thesis** (2023).
3. 🔴 The crates.io `desync` is **not** casync-in-Rust.
4. 🔴 RustCrypto is **no longer on aead 0.5.x** — the 1.0-track releases shipped mid-2026.
5. 🔴 Tigris has **no "accelerated" tier** — Standard / IA / Archive / Archive Instant Retrieval only.
6. 🔴 Bear **does** have per-note E2EE (AES-256-GCM via Themis, since 2019), contrary to the brief's framing.
7. 🔴 `VACUUM INTO` is **3.27.0 (2019-02-07)** — an automated read of the SQLite changelog initially misattributed it to 3.41.0; I corrected this by grepping the raw page.

**Open items, by topic:**

*SQLite tooling:* macOS notarization/code-signing requirements for Tauri sidecar binaries (Tauri docs silent). Which rusqlite version pins `libsqlite3-sys` 0.37.x, if you wanted that route. sqlx 0.9.0's actual release date (three sources disagree). Whether Litestream 0.5.x will restore age support (the dependency was removed, so not soon).

*User's cloud:* the exact subdirectory names under `~/Library/CloudStorage/` (inferred from community reports, no vendor spec). Whether a non-entitled process can actually write to `com~apple~CloudDocs` (inferred from 0700 permission bits; I did not perform a write). Whether the `IOPOL_MATERIALIZE_DATALESS_FILES` man page contradicts observed behavior. Windows `CfGetPlaceholderStateFromFileInfo` / `FILE_FLAG_OPEN_NO_RECALL` specifics. Dropbox placeholder detection cross-platform. Dropbox's `.part` ignore marker. `rename()` atomicity on File Provider and reparse-point filesystems. macOS TCC prompt wording for iCloud Drive and CloudStorage. Microsoft Defender Controlled Folder Access behavior. **Google's CASA assessment cost — no official figure exists anywhere public; treat the widely-quoted $15k–$75k range as unverified.** Dropbox's production-review turnaround SLA (none published) and its Business data-transport call limit (unpublished and unreadable from a user-linked app). Whether Google changed the Desktop OAuth client type in 2025–2026. Whether Microsoft publisher verification is required for multi-tenant consent in 2026. `google-drive3` and `graph-rs-sdk` crate versions and maintenance status.

*Object storage:* **B2 conditional writes (If-Match/If-None-Match) — no Backblaze page confirms or denies; test empirically, because a silent no-op is worse than a rejection.** Effective date and permanence of B2's free Class A/B/C transactions. Cloudflare Bandwidth Alliance 2026 status. Hetzner presigned-URL support. Whether R2 still rejects AWS-SDK default CRC32 checksums. Whether S3 is among the AWS European Sovereign Cloud's 90+ services. Whether R2 is in scope for Cloudflare's SOC 2 / ISO 27001 (the trust hub names no certifications). Whether R2's `eu` jurisdiction constrains metadata and logs or only object bodies. Tigris's SOC 2 report (gated). Tigris and Backblaze pricing pages carry **no last-updated date**.

*E2EE:* age/rage have **no security audit** I could find. No explicit age spec freeze declaration. `fastcdc` 4.0/5.0 breaking changes not enumerated. `rustic_core` binary size and compile time — no published numbers. The restic-chunking-secret discrepancy between the ETH thesis and restic's own docs. **No benchmark of CDC dedup ratios on SQLite files exists** — §4.3's argument is mechanistic plus the sqlite3_rsync data point. Signal SVR3's 2025–2026 production status. Apple recovery-key format. Rust `argon2` measured timings and C-reference parity. `age`/`rage` vs Go `age` throughput (no numbers found in either direction). aarch64/VAES autodetection verified from docs.rs, not by profiling. "No maintained SLIP-39 Rust crate" verified for crates.io only.

*Comparable apps:* Obsidian Sync Plus monthly price and the 100 GB expansion (plan *limits* are official; those two prices came from search summaries). Notion monthly vs annual (the live page renders $10/$20 in both toggle states). Notion's offline-mode specifics (their own help article could not be located). Logseq pricing — **no official pricing page exists**, and its E2EE has **no published spec**. Joplin's cipher and KDF — not documented in prose anywhere; the docs punt to source. Cryptomator Hub per-seat price. Backblaze Extended Version History prices. Day One's pricing looks freshly restructured — treat as volatile. Whether Bear uses `CKRecord.encryptedValues` under ADP.