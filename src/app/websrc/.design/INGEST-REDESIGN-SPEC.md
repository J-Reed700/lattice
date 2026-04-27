# Ingest Redesign — Implementation Specification

**Status:** proposed
**Sibling pilots:** `CHAT-REDESIGN-SPEC.md`, `JOURNAL-REDESIGN-SPEC.md`, `REFERENCE-REDESIGN-SPEC.md`, `DASHBOARD-REDESIGN-SPEC.md`, `FILEBROWSER-REDESIGN-SPEC.md`. This spec inherits their design language; differences are stated explicitly.
**Paired with:** `AESTHETIC-GUIDE.md`, `TOKENS-SPEC.md`, `PRODUCT-THESIS.md`.
**Scope:** `src/app/websrc/components/IngestHub/**` and the four `components/Ingest/**` import subcomponents (`UrlImport.tsx`, `BatchUrlImport.tsx`, `BatchFileImport.tsx`, `ImportHistory.tsx`). Also touches `FirstFolderPicker.tsx` for first-run shape alignment. No backend, no API, no data-model changes. The existing `VaultAPI.ingestWebUrl`, `startIndexing`, `startBatchImport`, and batch-status APIs are reused.
**Route:** `/ingest` (canonical; no change).

---

## 1. Current state

`IngestHub.tsx` is **151 lines**. A thin shell: title + description at top, a Radix `Tabs` component switching between four panels, each panel owned by a separate import component:

- **Single URL** (`UrlImport.tsx`, ~200 LOC) — input, URL preview fetch, single-URL ingest.
- **Bulk URLs** (`BatchUrlImport.tsx`, ~500 LOC) — multi-line textarea, extract URLs, per-URL preview, selectable table, progress modal, complete state.
- **Files** (`BatchFileImport.tsx`, ~500 LOC) — drop zone, multi-file list, space + collection pickers, batch indexing with progress.
- **History** (`ImportHistory.tsx`, ~400 LOC) — expandable job cards, real-time polling, per-item retry, delete, clear-all.

### 1.1 What's rendered

- **Page header** (`IngestHub.tsx:54-59`): `h1` "Import Content" (bold `text-3xl`) + a muted description. Generic.
- **Tab strip** (`IngestHub.tsx:66-115`): four tabs, underline-on-active. Matches Chat/Journal/Reference chip treatment — good.
- **Tab body** (`IngestHub.tsx:117-147`): whichever import component is selected, in a `flex-1 min-h-0 overflow-y-auto` region.

Each import component is full-featured but independently designed. They share no layout grammar. `UrlImport` uses framer-motion for input transitions; `BatchUrlImport` uses framer-motion for item transitions; `BatchFileImport` uses drop-zone plus a progress bar; `ImportHistory` uses expandable cards. **Four discrete designs in one surface**, loosely unified by the tab shell.

### 1.2 Data flows

Verified:

1. **Single URL**: type into input → debounced `fetchUrlPreview` 500ms → show preview card → user confirms → `ingestWebUrl` → toast + history entry. (`UrlImport.tsx:67-130`.)
2. **Bulk URLs**: paste multi-line text → extract with regex → per-URL preview fetch → selectable table → bulk ingest via `startBatchUrlImport` → progress modal → summary.
3. **Files**: drop or pick files → multi-file list with per-file status → user chooses space and/or collection → `startBatchImport` → progress tracked via `useIndexing` + per-file events → `addDocumentsToCustomCollection` on success.
4. **History**: list jobs via `listBatchJobs` → poll active ones at 2s → expand job to show per-item details via `getBatchJobDetails` → retry / delete / clear-all.

### 1.3 Decorative sins (cite `file:line`)

Surface-level audit — many of these will evaporate once the four components are rewritten in one shared grammar, but captured for precision:

- **Generic hero heading** — `IngestHub.tsx:55`: `"Import Content"` in `text-3xl font-bold` + muted description. Same framing sin as pre-redesign Dashboard ("Dashboard"): tells the user where they are when URL + nav already do. Replaceable with a briefing-style lede.
- **Four independent layouts.** Each subcomponent sets its own padding, its own preview-card pattern, its own progress affordance. No shared grammar beyond the tab switch. This is structural — not a line-number sin but a design-coherence sin.
- **`UrlImport.tsx`** uses `framer-motion` for input wiggle (`AnimatePresence` on error), which is below the animation budget per AESTHETIC-GUIDE §3 ("if animation is noticed, it's too much"). Verify and de-motion.
- **`BatchUrlImport.tsx`** uses `AnimatePresence` + per-row motion on the URL table — 500+ rows of URL imports with motion transitions each time status changes is a classic animation-taxation. De-motion per sibling spec vocabulary (§7).
- **`BatchFileImport.tsx`** uses a colored progress bar (presumed — needs full audit) and the space picker is a raw shadcn Select — fine. The drop zone itself is likely the most heavily-designed element; needs audit.
- **`ImportHistory.tsx`** uses expandable job cards with rounded borders — per AESTHETIC-GUIDE §3, cards are banned outside `--radius-lg` floating elements. Flatten to list rows with hairline dividers.

### 1.4 Conflated responsibilities

Unlike Journal's 3,062-line soup, IngestHub's conflation is **across four files, not within one**. Each file is coherent internally; the problem is that they don't share a grammar. The redesign unifies them into a single ingest surface with four *channels* (URL / Bulk URLs / Files / Folder) plus a single activity feed.

---

## 2. Product soul of Ingest

Ingest is **the onramp to the compound loop**. Without ingest, no corpus; without corpus, no query; without query, no capture. The entire Recall value proposition begins here.

Per the product thesis: Recall becomes whatever you load into it. That puts Ingest in an unusual position — it's the moment where the user *decides what Recall is going to become*. Not the moment where Recall is read; the moment where Recall is written. The surface should feel like **inviting the substrate** — welcoming content in, making it unambiguous what happened to it, and sending the user back to the loop where Recall starts to matter.

Three design imperatives emerge:

1. **Clear what comes in.** For every input channel (URL, bulk URL, file, folder), the user should see — before confirming — exactly what's about to be ingested. A URL shows its preview. A file list shows its count and total size. A folder shows its document count. Before clicking "import," nobody should be guessing.

2. **Clear progress while it's happening.** Indexing is slow. A 500-PDF import can run for minutes. The surface must carry that time honestly: a single, clear progress bar, a current-file name, a remaining count. Not multiple competing progress indicators.

3. **Clear what happens next.** "Your 47 files are indexed. Open the FileBrowser to see them." A button that takes the user back into the loop. Ingest is never a terminal state — it's a pass-through.

The current product nails (1) and (2) reasonably well in each tab, but fragments them across four designs. It misses (3) almost entirely — post-ingest, the user is stranded on the History tab with no prompt to go back to using their vault.

Per AESTHETIC-GUIDE §1: "the interface is a lens onto the user's own material." At Ingest, the user's material *is about to arrive*. The surface should feel like a foyer — restrained, welcoming, transparent, and pointing toward the rooms where the material will live.

---

## 3. Layout

### 3.1 Single-column, reading-width, tall

Centered single column at **`max-width: 760px`** — identical to Chat/Journal/References. Ingest is not a list-scanning surface; it's a *task surface*, with distinct steps (pick channel → preview → confirm → watch progress → land somewhere). Narrow-column focus is correct.

Padding: `24px` horizontal / `48px` top / `64px` bottom. Vertical section spacing: **`40px`** (one step down from Dashboard's 56px because Ingest's sections are more transitional — the user is stepping through them, not reading them all at once).

### 3.2 Tab strip → channel picker

The Radix `Tabs` stays — the underline-on-active treatment already matches sibling chip/tab patterns. But the tabs are **relabeled and reordered** to match the mental model of "what am I giving Recall":

**Old tabs:** Single URL · Bulk URLs · Files · History
**New tabs:** Files · URL · Bulk URLs · Folder · Activity

Reasoning:
- **Files first.** Files is the most common on-ramp. First-run users drag folders/files; steady-state users paste a URL. Lead with the most-used.
- **Folder becomes its own tab.** Currently folder-indexing happens only via `FirstFolderPicker` in the onboarding flow — but any user wanting to add a folder afterward has to use the Files tab + native folder picker. Surfacing Folder as a first-class channel aligns it with the first-run experience (§9) and gives steady-state users a natural "add another folder" entry point.
- **History → Activity.** "History" reads as archival; "Activity" reads as live. The tab content includes both in-progress and historical jobs — "Activity" is honest.

Tab count grows from 4 to 5. Width-check at 720px viewport: all five fit on one row at `text-sm` weight 500. Confirmed.

### 3.3 Lede — "what are you adding?"

Above the tab strip, a single-element lede matching Dashboard / FileBrowser register:

- Date/status sub-line at top (`text-xs`, uppercase, tracking +loose, `--text-muted`):
  - Idle: `INGEST`
  - Active job: `INGESTING · {N} items in progress`
  - Recent completion: `RECENT · {N} files indexed · {relative time ago}`
- Lede sentence (`--font-serif`, `text-2xl`, weight 600, tracking `-0.015em`, `--text-primary`):
  - Idle: `What would you like to add?`
  - Active: `Adding {N} items to your vault.`
  - Recent: `{N} files just landed.`
- `32px` bottom margin before the tab strip.

The lede sets the frame: this is a welcoming surface, inviting content in. Not a "Content Import Hub."

### 3.4 No header chrome beyond the lede

The current `<h1>Import Content</h1>` + description block is deleted. The lede carries the surface's identity.

### 3.5 What we are NOT doing

- **Not** adding a sidebar. Ingest is task-driven; a sidebar would navigate between channels, which the tab strip already does.
- **Not** adding a global drop zone that accepts drops anywhere on the surface. Drops only work inside the Files tab's drop zone — elsewhere they should fall through to OS handling. Global drop is a follow-up.
- **Not** unifying Bulk URL and Single URL into a single tab. They are different workflows (preview-one-and-commit vs. triage-many). Merging them would hurt both.
- **Not** redesigning the post-ingest flow across the app (space assignment, collection assignment). Those flows stay; we re-home their UI.

---

## 4. Channel-specific surfaces

Each tab follows a shared **three-phase layout**:

1. **Input phase** — what you're giving us (URL, URL list, file list, folder).
2. **Preview phase** — what's about to happen (count, titles, destination).
3. **Commit** — one primary action that starts it.

Progress (while running) and completion (after) are handled by a shared `IngestActivityStream` component (§5), so channel-specific panels dismount when the user clicks away — no duplicated progress state per tab.

### 4.1 Files tab (primary on-ramp)

The most-used channel. The redesign here sets the pattern.

**Input phase — drop zone:**

- Full-column, `240px` tall region.
- `--surface` background (no tint), `--border-default` 2px dashed border (acceptable exception to the hairline rule — dashed border signals "drop target"; it's structural feedback, not decoration), `--radius-md`.
- Centered content: 32px `Upload` icon at `--text-tertiary` · `text-sm` `--text-secondary` `"Drop files or folders here"` · `text-xs` `--text-muted` `"or click to pick from Finder"`.
- Hover/drag-over: `--border-strong` border, `--accent-muted` background. Instant transition (no fade). Border becomes solid (no longer dashed) — signals "ready to release."
- Click: opens native file picker (multi-select).

**Preview phase — file list:**

- When files are added, the drop zone shrinks to `96px` tall and becomes a compact version (icon + "Add more files" text, same click-to-open behavior).
- Below, a flat file list — no card frame, hairline dividers:
  - Each row: 44px tall, `10px 16px` padding.
  - Left: file-type icon (14px, `--text-tertiary`, reuse `FileIcon.tsx`).
  - Title: filename, `text-sm` weight 500 `--text-primary`, truncate.
  - Sub-line (below, `text-xs` `--text-tertiary`): `{size} · {fileType}`.
  - Right: status icon + small text. States: `pending` (clock icon, `--text-muted`), `importing` (spinner, `--accent`), `success` (check, `--success-fg`), `error` (alert-circle, `--danger-fg` + `text-xs` error message).
  - Remove action (only in `pending` / `error` states): small X icon on hover, right-aligned, `--text-muted` hover `--danger-fg`.

- Above the file list, a "destination row":
  - Label: `text-xs` uppercase tracking +loose weight 500 `--text-muted`: `DESTINATION`.
  - Inline: `Space` picker (shadcn `Select`) + `Collection` picker (shadcn `Select`). Both optional. Defaults: current conversation's space (existing behavior) and no collection.
  - `text-xs` `--text-tertiary` explainer: `"Where these files show up. Defaults to your current workspace."`

**Commit phase — button:**

- At the bottom of the list, full-column width button.
- `Button` primary variant (`--accent` fill, `--accent-fg` text), `48px` tall, `--radius-md`.
- Label varies:
  - No files: `Import files` (disabled).
  - N files pending: `Index {N} files` (enabled).
  - Importing: `Indexing…` (disabled, with small spinner inline).
  - Completed: the button is replaced by the activity-stream's completion card (§5).

**Deleted from current BatchFileImport:**

- The colored progress bar inside the import component — progress moves to the shared Activity Stream.
- Per-row motion transitions — instant state changes.
- The `X` close button in the top right (Batch import was originally in a modal; it's not anymore).

### 4.2 URL tab

Single-URL ingest. Simpler than Files.

**Input phase:**

- URL input at top of column. Same `Input` treatment as other surfaces — `--surface` bg, `--border-default` 1px, `--radius-sm`, `text-sm`, `36px` tall.
- Placeholder: `"Paste a link…"`.
- On input (debounced 500ms): fetch preview via `fetchUrlPreview`.

**Preview phase:**

- Below the input, a preview block appears when a preview is available. Flat (no card frame):
  - Hairline top border `--border-subtle`, `16px` top margin.
  - Title (from preview), `--font-serif`, `text-xl`, weight 600, `--text-primary`. Truncate at 2 lines.
  - Site + byline: `--font-sans`, `text-xs`, `--text-tertiary`, interpunct-separated: `{siteName} · {author} · {readingTime}min`.
  - Description paragraph from preview: `--font-serif`, `text-sm`, line-height 1.6, `--text-secondary`, line-clamp-3.
  - `16px` bottom margin before the destination row.

  Loading state: single `text-sm` `--text-tertiary` line `"Fetching preview…"` where the preview would appear, no spinner.

  Error state: single `text-sm` `--danger-fg` line `"Couldn't fetch preview. You can still import."`  The import button stays enabled — a broken preview doesn't block the actual ingest.

**Destination row:**

- Same pattern as Files tab §4.1 Destination row.

**Commit:**

- Button, same treatment. Label: `Import page` / `Importing…` / handled-by-activity-stream-on-completion.

### 4.3 Bulk URLs tab

Same skeleton as URL tab but scales to many.

**Input phase:**

- Multi-line textarea, `--surface` bg, `--border-default`, `--radius-sm`, `text-sm`, `--font-mono` (URLs feel right in mono).
- Min-height `120px`, auto-grows to `280px` then scrolls. Placeholder: `"One URL per line…"`.
- On blur / "Extract URLs" button: parse with the existing regex, populate the preview list.

**Preview phase — list of URLs:**

- Compact list of URLs (40px per row, no card frame):
  - Left: selection checkbox (user can deselect individual URLs before ingesting).
  - Center: URL text, `text-xs` `--font-mono` `--text-secondary`, truncate.
  - Right: preview status icon (`pending` clock, `fetching` spinner, `ready` check, `error` alert).
- Hover: `--surface` background.
- "Fetch previews" button at top: manually triggers preview batch. Alternatively auto-triggers on blur of textarea. Prefer auto.

When previews are ready, each row can expand (inline, no modal) to show the full preview (same preview block as §4.2). Expand via chevron toggle. Defaults collapsed — with 100 URLs you don't want 100 previews open.

**Destination row:** same pattern.

**Commit:**

- Button label: `Import {N} selected URLs`. During: `Importing {N}…`.

### 4.4 Folder tab (new)

A tab that doesn't exist today. Added to align with `FirstFolderPicker` — so users can add folders after first-run.

**Input phase:**

- Three suggestion rows at top (same pattern as `FirstFolderPicker.tsx:245-275`, restyled):
  - `Documents` / `Desktop` / `Custom folder…` — flat rows, hairline dividers, no border-on-hover, no cards.
  - Row layout: folder icon (14px) · name (`text-sm` weight 500) · path (`text-xs` `--text-muted`).
  - Hover: `--surface` background.
  - Click: if `Custom folder…`, opens `VaultAPI.selectFolder()`. If `Documents`/`Desktop`, uses that path directly.

**Preview phase:**

- Once a folder is picked, scanning begins. Replace the suggestion rows with a single "scanning" row:
  - `text-sm` `--text-secondary`: `Scanning {path}…`
- Once scan completes, show preview:
  - Title: `--font-serif`, `text-xl` weight 600 `--text-primary`: `{folder name}`.
  - Sub-line: `text-sm` `--text-tertiary`: `{full path}`.
  - Below: three horizontal counter pairs (same pattern as Dashboard §4.2 counters): `{N} documents found` · `{M} MB estimated` · `{K} file types`.
  - Below counters, a file-type filter chip row (from `FirstFolderPicker`'s fileTypes list, restyled): `PDFs` · `Word` · `Text` · `Markdown` · etc. User can toggle which types to include.

**Destination row:** same pattern. Space + Collection optional.

**Commit:**

- Button label: `Index {N} documents from {folder name}`. During: `Indexing…`.

Notes:
- The scanning phase in current `FirstFolderPicker.tsx:199-208` is **mocked** (random count). For this redesign, wire the real folder-scan API (may require a new backend call). Flag as a minor backend dependency — if unavailable, ship with the mock and flag for follow-up.

### 4.5 Activity tab (reborn History)

The fifth and final tab. Combines historical jobs with live progress, giving users a single place to see all ingest activity.

- If there is no history: empty state. Single serif line centered: `"No ingest activity yet."` + sub-line `"Files, URLs, and folders you've imported show up here."` + inline button `"Start with Files ↗"` (routes to Files tab).

- If there are jobs:
  - Top section: **Active jobs** (if any). Flat list of rows, no card frame, hairline dividers.
    - Row layout: job title (`text-sm` weight 500 `--text-primary`) + one-line status sub-line (`text-xs` `--text-tertiary`) + progress percentage (`text-sm` tabular-nums right-aligned).
    - Below each row, a 2px progress bar in `--accent` (this is the single legitimate use of `--accent` on this surface — see DASHBOARD-REDESIGN §4.3).
    - Click row to expand: shows per-item statuses as a nested flat list (same pattern as current `ImportHistory`'s expanded job details).
  - Middle section: **Recent** (completed in last 24h). Same row pattern but no progress bar; instead, right-aligned summary counter (`{success} of {total}`).
  - Bottom section: **Earlier** (completed >24h ago). Same pattern.
  - Below all: `Clear history` text button — `--text-muted` hover `--danger-fg`. Confirmation via `ConfirmDialog` (reuse existing).

- Section headings match sibling surfaces: `text-xxs` uppercase tracking +loose `--text-muted` weight 500.

- The current framer-motion per-row expand/collapse is replaced by a height transition `--duration-base --ease-out`.

- Individual job delete affordance: hover-revealed `Trash2` icon on the right of each row, `--text-tertiary` hover `--danger-fg`. No background pill.

- Retry-failed action: inline text button on expanded rows where `failedCount > 0`. Label `Retry {N} failed`.

---

## 5. Shared ingest activity stream

A conceptual element, not a separate UI region — but it's the reason the three phases (input / preview / commit) on each channel end cleanly. After commit, the channel's panel does **not** render a progress bar or completion card inline. Instead:

1. A **toast** fires on start: `"Indexing {N} {noun}…"` (existing pattern, keep).
2. The Activity tab's count badge increments — subtle visual feedback that the job exists somewhere.
3. The channel's input phase resets (textarea clears, file list clears, folder pre-selection clears) — user is invited to submit another.
4. On completion: a **toast** fires: `"{N} {noun} indexed — View in your vault"` with an action button that routes to `/files`.

This is the "clear what happens next" imperative from §2. The user doesn't sit on the Ingest surface watching a progress bar — they're freed to go use the vault, or queue another ingest. Progress is always available on the Activity tab if they want it.

**Deleted:** the in-tab progress modals in `BatchUrlImport.tsx` and `BatchFileImport.tsx`. Progress is centralized on Activity tab + toast notifications.

---

## 6. Empty states

Each tab gets a clean empty state. First-time user lands on Files tab by default (the first tab). If they've never imported anything, Activity tab also shows its empty state.

| Tab | Empty state |
|---|---|
| Files | The drop zone (§4.1) IS the empty state — it's already instructive. No extra copy. |
| URL | The input field IS the empty state. Placeholder does the work. |
| Bulk URLs | Textarea IS the empty state. Placeholder does the work. |
| Folder | The three suggestion rows + "Custom folder" ARE the empty state. `FirstFolderPicker` pattern. |
| Activity | Centered serif `"No ingest activity yet."` + sub-line + inline button (see §4.5). |

Note that most tabs have no "empty state" because the input affordance itself carries the prompt. This is correct — the user came to Ingest to ingest; put the input in front of them, not a pep talk.

---

## 7. Motion

Sibling vocabulary.

**Animates:**

- Drop zone drag-over: instant border + background change (`--duration-fast` color-only transition). No size change.
- Preview appearance (URL tab, Folder tab): `opacity 0 → 1` over `--duration-fast`. No rise.
- Activity tab row expand/collapse: height transition `--duration-base --ease-out`.
- Progress bar fill (on Activity tab): `width` transition `--duration-base --ease-out`.
- Toast entries: standard toast animation (existing).

**Does NOT animate:**

- Tab switch (Radix default — `data-state` transition; keep as instant).
- File list row additions (when a user drops more files): instant. Framer-motion per-row is banned per §1.3.
- URL list row additions: instant.
- Progress status icon changes (per file/URL): instant icon swap.
- Commit button label swap (Idle → Importing → resets): instant.
- Lede sentence change on state transition (idle → active → recent): instant.

---

## 8. Component ownership

### Decomposition strategy: redesign in place + extract channels

The four import components are reasonably sized already (~200-500 LOC each). Redesigning them means:

1. Unify them under a shared `IngestChannel` pattern (input phase → preview phase → commit).
2. Extract common helpers (destination-row, preview-card styling, commit-button) into shared components.
3. Add the new Folder channel.
4. Collapse per-tab progress into the shared Activity Stream approach.

No extreme file surgery needed, unlike Journal.

### Files to create (in `components/IngestHub/`)

| File | Responsibility | Approx LOC |
|---|---|---|
| `IngestHub.tsx` | Shell. Reads active tab from URL param or localStorage, renders lede + tab strip + active channel. | 120 |
| `IngestLede.tsx` | Date-status sub-line + serif lede sentence. Pure presentation; takes state as prop. | 50 |
| `DestinationRow.tsx` | Shared: Space + Collection pickers + explainer text. Reused by all four input channels. | 100 |
| `CommitButton.tsx` | Shared primary-action button with state-aware label. | 60 |
| `channels/FilesChannel.tsx` | New Files channel: drop zone + file list + destination + commit. Replaces current `BatchFileImport.tsx`. | 300 |
| `channels/UrlChannel.tsx` | New URL channel: input + preview block + destination + commit. Replaces `UrlImport.tsx`. | 180 |
| `channels/BulkUrlChannel.tsx` | New Bulk channel: textarea + URL list + destination + commit. Replaces `BatchUrlImport.tsx`. | 360 |
| `channels/FolderChannel.tsx` | New Folder channel: suggestion rows + scan + preview + commit. **New surface.** | 240 |
| `channels/ActivityChannel.tsx` | Reborn History: sections for Active / Recent / Earlier + expansion. Replaces `ImportHistory.tsx`. | 280 |
| `useIngestActivity.ts` | Hook composing active-jobs state + history state + dispatch for start/cancel/retry. Lifts orchestration out of channel components. | 200 |

### Files to delete (after new channels ship)

- `components/Ingest/UrlImport.tsx` → replaced by `channels/UrlChannel.tsx`.
- `components/Ingest/BatchUrlImport.tsx` → replaced by `channels/BulkUrlChannel.tsx`.
- `components/Ingest/BatchFileImport.tsx` → replaced by `channels/FilesChannel.tsx`.
- `components/Ingest/ImportHistory.tsx` → replaced by `channels/ActivityChannel.tsx`.
- `components/Ingest/DropZone.tsx` → absorbed into `channels/FilesChannel.tsx` (inline).
- `components/Ingest/DropZone.example.tsx` — delete (not in use).
- `components/Ingest/index.ts` — delete once the above four are gone.

The whole `components/Ingest/` directory is deleted after migration.

### Files to modify

| File | Change |
|---|---|
| `components/FirstFolderPicker/FirstFolderPicker.tsx` | Align visual grammar with `FolderChannel.tsx` — same suggestion rows, same preview pattern, same commit button. The first-run experience and the "add another folder" experience read identically. See §9 for scope. |
| `hooks/useIndexing.ts` | Unchanged. Continues to manage `startBatchImport` + event listening. |
| `utils/batchImport.ts`, `utils/batchHistory.ts` | Unchanged. APIs reused. |

### shadcn primitives used

- `Tabs` (existing) — tab strip, unchanged.
- `Select` — Space picker, Collection picker.
- `Input` — URL input.
- `Textarea` (add if missing) — bulk URL textarea.
- `Button` — primary commit + ghost buttons.
- `Checkbox` — per-URL selection in Bulk.
- `Tooltip` — status icon hover hints.

All present except possibly `Textarea` — verify during implementation.

### Cross-surface reuse

- `FirstFolderPicker.tsx` shares the folder-scan + commit pattern with `channels/FolderChannel.tsx`. The two should consume shared scan/commit helpers. Extract a `useFolderIngest.ts` hook if the pattern is used in both places (likely yes).
- Toast notifications for ingest start/completion use existing `toastStore` + existing patterns. No changes.

---

## 9. First-run alignment

The `FirstRun` / `FirstFolderPicker` flow is a user's first experience of Recall. It currently lives in `components/FirstFolderPicker/` and is styled independently from IngestHub — with its own suggestion rows, preview card, progress UI, and completion state.

**Scope:** Align `FirstFolderPicker.tsx` with `FolderChannel.tsx`'s visual grammar. Same suggestion rows, same scan/preview pattern, same counters, same file-type chips, same commit button. The **flow** stays distinct (first-run has a welcome header, post-index celebration, onComplete/onSkip routing); the **visual register** unifies.

The lede sentence on first-run reads differently:

- `--font-serif text-3xl weight 600`: `"Pick a folder to load into Recall."`
- Sub-line `text-sm --text-tertiary`: `"Whatever you load, Recall becomes a tool for. Pick a folder you want to think with."`

That sub-line is the single place in the app where the domain-morphic thesis is stated aloud to the user. It earns its place because first-run is the one moment where users are ready to hear it.

Out of scope: changing the first-run routing, the welcome steps, or any other `FirstRun/*` component beyond `FirstFolderPicker.tsx`.

---

## 10. Scope boundaries

This spec is **NOT**:

- Changing the backend ingest pipelines, batch APIs, or indexing behavior.
- Changing the space / collection data models.
- Adding new ingest sources (cloud providers, email, etc.). `SourceConnection` UI moves out of FileBrowser into Settings per `FILEBROWSER-REDESIGN-SPEC §3.5`, but adding new source providers is a separate spec.
- Implementing a cancellation UI during batch ingest (cancel exists at API level; exposing it cleanly in UI is follow-up).
- Adding scheduled imports / "watch this folder" continuous sync.
- Adding URL-pattern imports (crawl a domain, fetch all pages matching a pattern).
- Adding a first-run folder-contents preview (show a sample of files before confirming). The folder preview is just counts.
- Changing the `IndexProgress` event schema or the streaming-progress mechanism.

---

## 11. Risks

1. **Unifying four discrete components is structurally simple but visually risky.** Four separate authors shipped four subtly different conventions; the new unified grammar must work for all four. Implementation should build `DestinationRow.tsx` and `CommitButton.tsx` first, validate with one channel (Files), then extend.

2. **Centralizing progress off-channel is a behavior shift.** Users who watched the progress bar inside Bulk URL or Files today will find it moved. Toasts + Activity tab count badge must be clear enough that nobody misses the move. If review finds this jarring, fallback: keep a compact mini-progress-row inside the channel during active ingest (one line, `text-xs`, hairline separator, no full progress bar — just `"Indexing 12 of 47…"`).

3. **Folder channel's real scan API.** Current `FirstFolderPicker` mocks the scan with a random delay + random count (line 199-208). For `FolderChannel.tsx` we need a real scan. If no backend API exists to count files matching filter-types without indexing them, either (a) add one, or (b) ship folder preview as "estimated" and let the user proceed. Flag during implementation.

4. **First-run UI aligned but separate.** If the first-run flow evolves independently of IngestHub (welcome animations, different routing), keeping them in sync will take discipline. Document the shared components in both places so future changes don't drift them.

5. **Bulk URL preview fetching at scale.** Fetching 100 URL previews is slow and can timeout. Current implementation doesn't rate-limit. Preserving current behavior is the minimum; future improvement is a rate-limited queue with a visible progress line on the command. Out of scope for this pass.

6. **Five tabs may be one too many.** If Folder as its own tab proves rarely used, collapse it back into Files (with a "Or pick a folder…" link in the drop zone). Measure after a release.

---

## 12. Tokens and patterns needed beyond TOKENS-SPEC

None required. TOKENS-SPEC is sufficient.

Patterns inherited / introduced:

- **Three-phase channel layout (input / preview / commit)** — introduced here. If it works, could extend to other task surfaces (e.g., Journal's Synthesize popover has a similar three-phase shape).
- **Destination row with Space + Collection pickers** — shared component, first cross-component reuse of these two pickers (they appear separately in FileBrowser's bulk-selection bar and BatchFileImport today). Consolidating them has value beyond Ingest.
- **Activity stream with sectioned Active / Recent / Earlier lists** — new. Similar in shape to the Dashboard's Activity block (§4.3) but surfaces just ingest-jobs, not the cross-loop activity feed. No reuse across surfaces; they're deliberately different granularities.
- **Dashed-border drop-zone** — the one accepted exception to the hairline-border rule. Dashed signals "drop target" — structural, not decorative. Documented here so future reviewers don't flag it.

---

## 13. Questions for Josh

1. **Folder as its own tab, or tucked inside Files?** Spec default: **own tab**, symmetric with first-run. Alternative: make Files a hybrid (drop zone + folder picker link in one surface). Own tab is cleaner but costs one tab of horizontal real estate. Confirm.

2. **Post-commit behavior — toast + go back, or stay with progress?** Spec default: **toast + reset channel + user can navigate away; Activity tab carries the progress**. Alternative: keep an in-channel mini-progress-row during the active job so users who want to stay and watch can. Spec's default is the more confident move; the alternative is a safer fallback. Confirm.

3. **First-run sub-line copy — "Whatever you load, Recall becomes a tool for. Pick a folder you want to think with."** That sentence is the single place in the app where the domain-morphic thesis is stated aloud. It's assertive and a little precious. Voice review may soften it. Alternative drafts:
   - `"Pick a folder. Recall becomes a lens on whatever you load into it."`
   - `"This folder becomes your vault. Choose one you want to think with."`
   - `"Whatever you put in, Recall helps you think about. Start here."`
   Confirm the register, or propose a replacement.
