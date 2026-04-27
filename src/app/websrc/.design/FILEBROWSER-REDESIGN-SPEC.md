# FileBrowser Redesign — Implementation Specification

**Status:** approved (2026-04-23) — Oracle rounds 1 + 2 complete. See §15.
**Sibling pilots:** `CHAT-REDESIGN-SPEC.md`, `JOURNAL-REDESIGN-SPEC.md`, `REFERENCE-REDESIGN-SPEC.md`, `DASHBOARD-REDESIGN-SPEC.md`. This spec inherits their design language; differences are stated explicitly.
**Paired with:** `AESTHETIC-GUIDE.md`, `TOKENS-SPEC.md`, `PRODUCT-THESIS.md`.
**Scope:** `src/app/websrc/components/FileBrowser/**` only. No backend, no API changes, no data-model changes. The `useFileBrowserStore` is reused; we may deprecate slices of it (saved views, saved searches, three-mode tree) but we do not restructure it.
**Route:** `/files` (canonical; no change).

---

## 1. Current state

The most architecturally-loaded surface in the app. Two files dominate:

- `FileBrowser.tsx` — **1,010 lines**. Orchestrator: store wiring, bulk actions, content-search scheduling, dialogs (rename, delete, saved search naming), selection state, space-assignment flow, viewer state. Plus the view-mode switching between `TreeView`, `ListView`, `GridView`. (`FileBrowser.tsx:108-1009`.)
- `TreeView.tsx` — **2,442 lines**. A three-mode sidebar (Collections / Presets / Sources) + a virtualized document list + context menus + saved-view editor + source connections management. The three "modes" are `leftMode: 'collections' | 'presets' | 'sources'` (`TreeView.tsx:434`), each rendering a completely different left-rail treatment.

Ancillary: `LibraryToolbar.tsx`, `GridView.tsx`, `ListView.tsx`, `ContextMenu.tsx`, `RenameDialog.tsx`, `SavedSearchControls.tsx`, `SavedSearchNameDialog.tsx`, `FileIcon.tsx`, `FileTypeBadge.tsx`. These are the token-swept survivors; most are small and clean.

### 1.1 What is rendered (visually)

- **Top strip** — `LibraryToolbar` (`FileBrowser.tsx:798`) with search input, saved-view Select, saved-search dropdown/pinned rail, save-as-collection, filter by source (all / local / web), group-by-date toggle, view-mode toggle (tree / list / grid), density toggle, refresh. A very full bar.
- **Selection bar** (when N > 0, `FileBrowser.tsx:846-893`): "N selected" + space picker + Assign to Space + Delete Selected + Clear Selection. A tinted `bg-[hsl(var(--accent-muted))]/25` panel — acceptable but an outlier from sibling patterns (sibling surfaces don't have bulk actions on the rail).
- **Main canvas** — a `rounded-2xl border bg-[hsl(var(--surface-raised))] shadow-sm` frame wrapping whichever view is active (`FileBrowser.tsx:895-939`). The frame itself is a card, and AESTHETIC-GUIDE §3 bans card frames wrapping primary content. This is a vestige of the prior bento-card era — it survived the token sweep because it uses token values, but the *shape* is still card-chrome.
- **TreeView interior** — two-pane: a left rail (Collections / Presets / Sources), a scrolling virtualized document table. The document table has sortable column headers, density toggle, selection checkboxes, a `RefreshCw` button per row.
  - Collections mode: Smart collections (All / Recent / Web / Local / Docs / Media / Code / Archives) + auto-derived projects (path-root-derived clusters) + custom collections (user-created) with rename/delete/children.
  - Presets mode: saved-view editor. Lets the user compose a view (filters + sort + columns) and save it. An entire mini-IDE for view construction.
  - Sources mode: `SourceConnection` list (local folders / web / cloud providers) with add/remove/sync controls.

### 1.2 Data flows

Verified by reading the store + the components:

1. **Document list** (`VaultAPI.listAllDocuments`) — all indexed files as `DocumentMetadata[]`, client-filtered by search / filter / source / group. Virtualized with `@tanstack/react-virtual`.
2. **Content search** (`VaultAPI.readFileContent`) — fires batch-of-6 when the search query matches no filename; caches normalized content per doc.
3. **Custom collections** — CRUD on `customCollections` in the store. Two kinds: `manual` (user-curated) and `snapshot` (frozen from a result set).
4. **Saved views** — CRUD on `savedViews`. Each saves the full filter + sort + view-mode + column state; applying a view restores all of it. Used in the LibraryToolbar's "Views" Select.
5. **Saved searches** — CRUD on `savedSearches`. A subset of a saved view: just query + filter + source. Can be pinned into a horizontal rail.
6. **Source connections** — CRUD on `sourceConnections`. Local folders, web-import marker, cloud providers (placeholder).
7. **Bulk operations** — selection model, assign-to-space, bulk delete, save-results-as-collection.
8. **Context menu** — per-row: view, rename, delete. Plus a rename dialog and delete confirm dialog.

### 1.3 Decorative sins (cite `file:line`)

Most have been token-swept. What remains structural:

- **Card-frame wrapper around the canvas** — `FileBrowser.tsx:896`: `rounded-2xl border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] shadow-[var(--shadow-sm)]`. The shadow is `--shadow-sm`; the `rounded-2xl` is `16px` which exceeds the `--radius-lg` (`12px`) ceiling in TOKENS-SPEC §5. Delete the frame entirely. The view lives on `--bg`.
- **Tinted selection bar** — `FileBrowser.tsx:847`: `bg-[hsl(var(--accent-muted))]/25` panel. Per TOKENS-SPEC §1.4, accent-muted is for selected-row backgrounds and subtle highlights — a full-width action bar is a larger accent surface than intended. Replace with a `--surface` fill + hairline border + `--border-subtle` per sibling pattern.
- **Per-row `RefreshCw` icon button** — appears on every row (TreeView document table). Refresh is a view-level action, not a row-level one; move it to the toolbar and delete the per-row button.
- **Empty state illustration tile** — `FileBrowser.tsx:899-914`: `rounded-full border ... bg-[hsl(var(--surface))] p-4` with a 48px `FileUp` icon inside. Token-clean but still a "tile with centered icon" anti-pattern from AESTHETIC-GUIDE §3. Flatten to the sibling empty-state convention (icon + headline + body + button, no frame around the icon).
- **`bg-white/[0.0X]` and `border-white/XX`** — none in `FileBrowser.tsx` (token-swept). Scan `TreeView.tsx` remains expected — during this redesign the 2,442 lines get decomposed anyway.
- **Three-mode left rail** — not strictly a decorative sin but a structural one: offering three completely different spatial treatments (Collections tree / Presets editor / Sources manager) creates three surfaces pretending to be one. See §3.5 and §11.

Tokens overall: cleaner than pre-sweep siblings. The bulk of the work here is **structural**, not chromatic.

### 1.4 The conflated responsibilities

`FileBrowser.tsx` (1,010 LOC) owns:

1. Store wiring for ~25 slices of state.
2. Content-search scheduling (debounce + batched reads + cache + failure set).
3. View-mode switching.
4. Selection/bulk flows (assign-to-space, bulk delete).
5. Saved-search CRUD (create/rename/duplicate/delete/pin).
6. Saved-view integration.
7. Frozen-collection creation from results.
8. Six dialogs (rename, delete-doc, delete-many, delete-saved-search, saved-search-name, viewer).
9. Space list loading + membership inference for selected docs.
10. Context menu coordination.

`TreeView.tsx` (2,442 LOC) owns:

1. Three-mode left rail (Collections / Presets / Sources).
2. Collection-model computation (smart + projects + custom + source-derived).
3. Saved-view editor (draft state + apply/save/capture/delete).
4. Saved-search inline rename + pin toggle.
5. Source connection CRUD + reconcile.
6. Virtualized document table with 5-8 sortable columns, density toggle, selection checkboxes.
7. Inline renames for collections, searches, sources.
8. Collapsed/expanded collection states.
9. Drag-and-drop reordering of pinned searches.

This is a 3,000+-line surface doing the job of three different products (library browser, view-composer IDE, source-connection manager). The redesign separates them spatially and files-wise.

---

## 2. Product soul of FileBrowser

This is **the most identity-defining surface in Recall**. Not Chat, not Journal — FileBrowser. It is *where you see what your corpus IS*.

Chat is "what I'm asking it right now." Journal is "what I'm writing right now." References are "what I chose to save." FileBrowser is **the substrate itself, rendered** — the content, in its native units (files, documents, pages), with the structure the user sees as the structure Recall sees.

Per the product thesis: Recall is domain-morphic. "Load recipes → cookbook, load research papers → research assistant, load legal docs → legal review tool." The cookbook user must open FileBrowser and feel *yeah, this is my recipe collection*. Not through illustrations or theming — through *their actual recipes*, rendered in a way that makes their recipe-ness undeniable: filenames legible, categories visible, counts obvious, previews cheap to glance at.

This means three shifts from the current product:

1. **The documents themselves are the subject.** Currently they're rows in a table with chrome (checkbox, icon, filename, fileType, category, size, indexedAt, actions). The new version gets out of their way: one clear title per row, one subtle metadata line, and enough density to see 20+ at once without scrolling.
2. **Shape is revealed, not navigated.** The three-mode tree (Collections / Presets / Sources) treats the vault as a filesystem to traverse. That's backward: the user knows what's in their vault — they want to see it, not navigate it. Most of the three-mode machinery exists to compensate for "too many documents to scan;" the redesign solves that by grouping-and-folding the list itself, not by hiding it behind a sidebar.
3. **The "what your corpus IS" question is answered at the top, not the side.** A single scannable strip at the top of the surface shows type distribution + scale + recency. The user's eye reads identity from it in one beat. (Mirrors DASHBOARD-REDESIGN §4.2 but uses the full canvas width and treats the browser as a magazine spread, not a morning briefing.)

Per AESTHETIC-GUIDE §1: "the interface is a lens onto the user's own material." Nowhere is this more literal. FileBrowser's chrome is the lens; the corpus is the image. Every pixel of chrome we save is a pixel the image gets.

---

## 3. Layout

### 3.1 Two-region shell: identity strip + corpus body

A deliberate break from the sibling `280px` sidebar pattern. FileBrowser is not a reading surface with a navigable list — it's a magazine-spread of the corpus. The sidebar would carry navigation for something that doesn't benefit from a sidebar (the list *is* the thing; navigating within the list happens via grouping, not via a separate pane).

**Full-canvas layout:**

- **Top strip — "Corpus identity band."** Full width, `~140px` tall. Contains type breakdown + counts + last-ingested timestamp. (§4.) No border below; a `40px` vertical gap separates it from the body.
- **Body — the documents.** Full-width of the canvas minus `32px` horizontal padding. A single scrollable region. Grouped by the user's current choice (date / type / source / none). Virtualized via `@tanstack/react-virtual` (already present — reuse).
- **No card frame.** The canvas is `--bg`, the body sits on it directly. Delete the `rounded-2xl border ... bg-[hsl(var(--surface-raised))]` wrapper at `FileBrowser.tsx:896`.

### 3.2 Why not a sidebar

Three reasons:

1. **The three-mode tree is the wrong abstraction.** Collections / Presets / Sources are three different things — a way to group, a way to save view-state, and a way to manage connectors. They don't belong in the same panel. They don't even all belong on this surface. (See §3.5.)
2. **Sibling sidebars carry a navigable list (conversations, entries, references).** FileBrowser's primary content IS a list. A sidebar listing groups of list items is a list of lists — a level of indirection that adds steps to see anything.
3. **The corpus must occupy the visual budget.** A 280px sidebar reserves ~30% of canvas for chrome the user rarely touches. Full-canvas gives the documents that 30% back. Exactly the identity move this surface is supposed to make.

What about filtering? Filters live in a single **command bar** at the top of the body (§5), not in a sidebar. Filtering is a transient action; chrome for transient actions shouldn't be always-visible.

### 3.3 Reading width

**No reading-column constraint.** The body is full-canvas width minus `32px` horizontal padding. This is a list-scanning surface, not a prose-reading surface — the 760px reading column from sibling specs doesn't apply. Filenames benefit from horizontal room.

A single list item never exceeds `880px` wide though — at wider viewports, the list is centered at `880px` max, and the extra canvas becomes whitespace on both sides. Without this cap, filenames stretch to 140-character lines and the metadata line becomes unreadable.

### 3.4 What we are NOT doing

- **Not** keeping the three-mode tree (Collections / Presets / Sources). See §3.5.
- **Not** keeping saved-views as a top-level feature on this surface. See §11.
- **Not** keeping the per-row `RefreshCw` icon button. Refresh is a view-level action on the command bar.
- **Not** keeping Grid view and List view as separate modes. See §6.
- **Not** building a file-preview-on-click experience inline. The existing `ContentViewer` modal (full-screen viewer) handles click-to-open; keep that unchanged.
- **Not** touching `SourceConnection` management. See §3.5 — sources move to Settings.

### 3.5 Where the three modes go

| Old mode | New home |
|---|---|
| **Collections** (smart + projects + custom) | Deprecated as a sidebar mode. Smart buckets (Web / Local / PDFs / Media / Code) become **filter chips on the command bar** (§5). Auto-projects (path-derived clusters) become a **group-by option** (§6.3). Custom collections (user-curated) become first-class as a **Collections Strip** rendered above grouped lists when collections exist (§7). |
| **Presets / Saved Views** | Deprecated as a sidebar mode. View state is restorable but without a dedicated UI surface — the user composes a view with filters/sort/group in the command bar, and a single `Save this view` affordance in the overflow menu writes it to the existing `savedViews` store. Stored views are listed in an inline dropdown from the command bar, not a sidebar mode. **Saved Searches are deprecated entirely** (they were a strict subset of saved views; the complexity-to-value ratio doesn't survive review). See §11 open questions. |
| **Sources** | Moves to Settings (a new "Sources" tab in existing Settings surface — out of scope for this file's redesign; flagged for a follow-up spec). Rationale: managing connectors is a configuration concern, not a browsing concern. The user manages sources rarely; seeing them on every browser open is chrome they don't need. Ingest status and connector health surface on Dashboard (`DASHBOARD-REDESIGN §4.3`) where they belong as "what's happening to your corpus." |

The left-rail is, in effect, **deleted**. This is the biggest structural change in the spec and the one that earns the most surface space back for the documents.

---

## 4. Corpus identity band (top of surface)

The first thing the user sees. Roughly `140px` tall, full-width, no frame.

### 4.1 Structure — three horizontal blocks, baseline-aligned

Sub-grid, three columns at 2:3:2 ratio, `48px` column gap, `40px` top padding. Wraps to stacked at <720px.

**Block 1 — "What it is" (2 units):**

- Lede sentence: `--font-serif`, `text-2xl` (24px — a step below Dashboard's `text-3xl` because this surface has more below it to read), weight 600, tracking `-0.015em`, `--text-primary`. Dynamically composed:
  - `1,247 documents.` (just the count, with period)
  - Below, a second serif line at `text-lg`, weight 400, italic, `--text-secondary`: `PDFs, web clippings, and notes, mostly.` — or whatever two-to-three categorical nouns dominate the corpus. (Derived from `DocumentMetadata.category` top-3.) Empty corpus: `An empty vault — waiting.`
  - Domain-morphic handling: exact argument as Dashboard (§6). The nouns are generic (`PDFs`, `web clippings`, `notes`) — never `recipes` or `papers`. The composition emerges from the user's data.

**Block 2 — "Shape" (3 units):**

- Horizontal type-breakdown bar, `10px` tall, spanning the column width, divided into proportional segments. Same monochromatic treatment as `DASHBOARD-REDESIGN §4.2.1` — no accent, no rainbow. Segments at descending tonal opacity: largest at `--text-secondary`, descending to `--text-muted`. Hairline between segments (`1px --bg`) so adjacent segments don't blur into one tone.
- Below the bar, inline legend: `--font-sans`, `text-xs`, `--text-tertiary`, `8px` top margin. Five max, interpunct-separated: `PDFs 612 · Web 398 · Notes 142 · Code 78 · Other 17`.
- Tooltip on hover per segment: `{category} · {count} documents · {percentage}%`. (Radix `Tooltip`.)

**Block 3 — "Motion" (2 units):**

- Three stacked counter pairs:
  1. Value: `text-lg`, weight 600, tabular-nums, `--text-primary`: `4 this week`. Sub-label: `text-xs`, `--text-tertiary`: `added`.
  2. Value: `text-lg`, weight 600, tabular-nums, `--text-primary`: `4h ago`. Sub-label: `text-xs`, `--text-tertiary`: `last ingested`.
  3. Value: `text-lg`, weight 600, tabular-nums, `--text-primary`: `38 folders`. Sub-label: `text-xs`, `--text-tertiary`: `source paths`.

Block 3 is tight — three small data points, stacked. Not a counter row (which would compete with Block 1's typography). Stacked gives it a quiet, reference-chart quality.

### 4.2 Visual weight

The band should feel *substantial but settled* — editorial-magazine masthead, not dashboard card. All three blocks share a common baseline via `align-items: baseline` on the sub-grid. No borders, no backgrounds, no icons. The identity of the corpus is carried entirely by typography and one thin monochrome bar. The rest of the canvas rewards that restraint by being nothing but the corpus itself.

### 4.3 When the corpus is empty

The identity band collapses to a single serif line + a single action button:

- `--font-serif`, `text-2xl`, weight 600, `--text-primary`: `An empty vault — waiting.`
- `--font-sans`, `text-sm`, `--text-tertiary`, `8px` top margin: `Add your first folder or import a URL to begin.`
- `16px` below, a single `Button` (shadcn primary variant): `Add files` — routes to `/ingest`.

The body below is hidden (no rows to render); the surface reads as a single, calm empty-state.

---

## 5. Command bar (above the body)

A single horizontal strip, `48px` tall, `--bg` background with a `--border-subtle` hairline bottom. Not sticky by default — scrolls with the canvas. (Can be made sticky as a follow-up polish; out of scope.)

### 5.1 Elements, left to right

1. **Search input.** Flex-grow, max-width `420px`. Same treatment as Chat composer textarea (`CHAT-REDESIGN §4.2`): `32px` tall, `--surface` bg, `--border-default` border, `--radius-sm`. Placeholder: `"Search documents or content…"`. Content-search still fires on non-filename matches per the existing `contentCacheRef` scheduling.
2. **Type filter chips** — horizontal row, wraps below at narrow widths. Replace the smart-collections sidebar. Chips: `All` · `PDFs` · `Web` · `Notes` · `Code` · `Media` · `Other`. Inactive: `--text-tertiary`, no background, no border. Active: `--text-primary`, `2px` underline in `--accent`. Same treatment as Chat sidebar filter chips (`CHAT-REDESIGN §5.1`).
3. **Source filter** — segmented control (`All` / `Local` / `Web`). shadcn-style tabs; active tab `--surface` fill, `--text-primary`; inactive transparent `--text-tertiary`.
4. **Group-by Select** — shadcn `Select`. Options: `None` / `Date` / `Type` / `Folder` / `Source`. Default: `Date`. See §6.3.
5. **Sort Select** — shadcn `Select`. Options: `Recent first` / `Oldest first` / `Name A–Z` / `Name Z–A` / `Largest first` / `Smallest first`. Default: `Recent first`.
6. **View density** — icon button toggle (`List` / `Detail`). Two options only (see §6). `--text-tertiary` default, `--text-primary` active, no borders.
7. **Overflow `…` menu** — Radix `DropdownMenu`. Contains: `Refresh`, `Save this view…`, `Apply saved view ▸` (submenu listing stored views), `Export CSV…` (follow-up if wanted — out of scope for now), `Manage sources ▸` (link to Settings).

No saved-searches pin rail. No view Select dropdown at rest. No save-as-collection button in chrome (see §8). The command bar is ~8 affordances; any more and we're back in toolbar hell.

### 5.2 Active filter visibility

When any filter is active (chip selected, source != All, search query set), a small **clear-all** text-link appears right-aligned on the command bar: `Clear filters` at `text-xs` `--text-muted` hover `--text-secondary`. No icon, no pill.

The active state is visible through the chips' own active styling — we do not show a separate "Showing 42 of 1,247" count by default. Count appears at the top of the body instead (§6.1).

---

## 6. Body — the corpus

### 6.1 Body header (result count + group heading context)

A `32px` tall row below the command bar, inside the body padding. Contains:

- Left: `text-sm`, `--text-tertiary`. Format: `Showing 42 of 1,247 documents` (when filters are active) or `1,247 documents` (unfiltered). Tabular-nums.
- Right (when selection N > 0): `text-sm`, `--text-primary`: `N selected` + two inline text buttons: `Assign to space…` (opens Radix `Popover` with the existing space list) + `Delete` (at `--danger-fg` on hover). No tinted panel, no separate bar. The selection state is woven into the body header, not a distinct affordance.

### 6.2 Row — the document

**Two density modes, one list layout**:

**List (default, dense):**

- Row height: `44px`.
- Padding: `10px 16px`.
- Layout (left-to-right):
  - Selection checkbox (12px, `--border-default` idle, `--accent` checked). Visible on hover only at rest; visible always when any selection is active.
  - File type icon (14px, `--text-tertiary`). Reuse `FileIcon.tsx` (token-clean).
  - **Title** — `--font-sans`, `text-sm`, weight 500, `--text-primary`, truncate. The filename with extension. This is the primary signal. Takes remaining horizontal space.
  - **Category chip** (inline, right of title, `--text-tertiary` `text-xxs` uppercase tracking +loose, no bg, no border): `PDF` / `WEB` / `CODE`. Helps the eye identify type at a glance when filtering is off.
  - Right: modified-time, `text-xs`, `--text-muted`, tabular-nums, `right: 0`.
- Hover: `--surface` background, no border. Hover-revealed row actions on the right (absolute-positioned, replacing the timestamp in the same zone): `Rename` / `Delete` / `Context…` (the ContextMenu trigger). Icon-only, 14px, `--text-tertiary`, hover `--text-primary`.
- Active (selected) row: `--accent-muted` background + `2px` left-bar in `--accent` (sibling pattern). Layered `framer-motion` `layoutId="files-active-bar"` when keyboard-navigating between rows.
- Click: opens the `ContentViewer` modal (existing) with the document.

**Detail (expanded):**

- Row height: `64px`.
- Same as List, but below the title, a second line: `text-xs`, `--text-tertiary`, interpunct-separated: `{category} · {fileType upper} · {wordCount} words · {relative path — 1 ancestor}`. Truncates at the right.
- Still no thumbnails. Thumbnails turn FileBrowser into a gallery; this surface is a list.

**Deleted:** the current `GridView.tsx` (grid of thumbnail tiles). Grid view is an unspoken admission that List wasn't scannable enough; the real fix is making List scannable, which the new row layout does. (Grid for documents is a Finder idiom — but Recall is not a file manager, it's a knowledge substrate.) See §11 open question 2.

**Preserved:** the `ListView.tsx` multi-column-sortable table — with modifications. Columns shown in Detail mode; hidden in List mode. We're not deleting the sortable-column affordance; we're making it an opt-in not a default.

### 6.3 Grouping

Default group: `Date` — `Today` / `Yesterday` / `This week` / `This month` / `Apr 2026` / `Mar 2026` / older, in descending order. Same sibling-consistent vocabulary (see `JOURNAL-REDESIGN §4.3`).

Group heading style:

- Sticky to viewport top while scrolling through that group (native CSS `position: sticky`).
- Styling: `--font-sans`, `text-xxs`, uppercase, tracking +loose, weight 500, `--text-muted`, `4px` vertical padding, hairline bottom in `--border-subtle`, `--bg` background.
- Trailing count at right: tabular-nums `{N} documents`.

Other group-by options:

- **Type**: `PDFs / Web / Notes / Code / Media / Other`. Headings show count.
- **Folder**: grouped by the `extractProjectName` logic from current `TreeView.tsx:174-197` — path-derived project clusters. Local-only; web docs grouped under `Web Imports`.
- **Source**: grouped by `SourceConnection.name` via existing `matchDocumentToSource` logic. Docs unassigned to any source (typical during transition) group as `Unsourced`.
- **None**: flat list, no group headings.

### 6.4 Empty-state (within body, when filters narrow to zero)

Centered in body:

- `text-sm`, `--text-tertiary`: `"No documents match."` (when a filter is active)
- `text-xs`, `--text-muted`, max-width 320px: `"Try clearing a filter or adjusting your search."`
- `Button` ghost variant: `Clear filters` (same action as the right-aligned `Clear filters` on command bar).

For a truly empty corpus (not filter-empty), see §4.3 — the identity band handles it.

---

## 7. Collections strip (conditional)

When custom collections exist (user has created any), render a small strip between the identity band and the command bar:

- `--font-sans`, `text-xs`, uppercase tracking +loose, weight 500, `--text-muted`: `COLLECTIONS` label on left.
- Horizontal scrollable row of collection chips. Each chip:
  - Border `1px --border-default`, `--radius-sm`, `6px 12px` padding.
  - Icon: `FolderPlus` (manual) / `Snowflake` (frozen — Lucide doesn't have Snowflake; reuse `Layers` for frozen until an icon is chosen). 12px, `--text-tertiary`.
  - Name, `text-xs`, `--text-secondary`.
  - Count: tabular-nums `text-xxs`, `--text-muted`, inline.
  - Active (applied as a filter): `--surface` fill, `--text-primary`, `--border-strong` border.
  - Click: filters the body to that collection.
- Trailing `+ New collection` inline text button (visible only when selection is active, to promote the "save selection as collection" action).

When no custom collections exist: the strip is not rendered.

Collection management (rename, delete, edit) is available through right-click context menu on each chip. No dedicated collection-editor UI — collection management is quiet, inline.

---

## 8. Bulk actions

The tinted selection-bar pattern at `FileBrowser.tsx:846-893` is deleted. Bulk actions consolidate into:

1. **Inline in body header (§6.1):** `N selected · Assign to space… · Delete`. Single line, no panel.
2. **Inline in collections strip:** when selection is active, `+ New collection` appears at the tail of the collection strip.

No tinted accent panels. No separate fixed bar. Selection is a mode; it changes the body-header row, nothing more.

---

## 9. Motion

Sibling vocabulary. Specifically:

**Animates:**

- Selection checkbox appearance on row hover: `opacity 0 → 1`, `--duration-fast`.
- Group header sticky entry: native CSS sticky; no transition required.
- Command bar filter chip active state: instant color + underline appearance, no fade.
- Active-row left-bar: `framer-motion` `layoutId="files-active-bar"`, `{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }`.
- Collection strip chip active state: instant.
- Empty-state body content: appears with `opacity 0 → 1` over `--duration-fast`, no rise.

**Does NOT animate:**

- Row appearance on sort/filter change: rows just reposition. The virtualizer handles it; do not add enter/exit animation to individual rows (would fight the virtualizer and create jank at scale).
- Group-by change: re-render is instant.
- Density toggle: instant row-height change. No easing transition on `height` (would be expensive at 1,000+ rows).
- Collection chip selection: instant.
- Bulk-action reveal: instant.
- Type-breakdown bar segment changes: instant (rare event — only when data updates).

At 1,000+ rows, motion discipline matters more than anywhere else in the app. Every animated property is a per-row cost. Instant transitions win by default.

---

## 10. Component ownership

### Decomposition strategy: **decompose while redesigning**

Same playbook as Journal and Reference specs. The 3,400 combined lines of `FileBrowser.tsx + TreeView.tsx` cannot be restyled in place safely; every structural change risks regressing a different concern.

### Files to create (in `components/FileBrowser/`)

| File | Responsibility | Approx LOC |
|---|---|---|
| `FileBrowser.tsx` | Shell. Reads `useFileBrowserStore`, composes identity band / command bar / collections strip / body. No JSX beyond the layout. | 150-200 |
| `CorpusIdentityBand.tsx` | Top strip: lede, type breakdown bar, counters. Same data-shape as `DashboardCorpusShape` — potential cross-surface reuse; see §10 cross-surface. | 200 |
| `CorpusCommandBar.tsx` | Search + type chips + source segmented + group-by + sort + density + overflow menu. | 280 |
| `CorpusBody.tsx` | The scrolling virtualized list with group headings. Orchestrates virtualizer + grouping + row rendering. | 240 |
| `CorpusRow.tsx` | Single document row. List density + Detail density variants. Selection checkbox, hover actions, active-state left-bar. | 180 |
| `CorpusGroupHeader.tsx` | Sticky group heading with count. | 50 |
| `CollectionsStrip.tsx` | Horizontal collection chips + new-collection affordance. | 150 |
| `BodyHeader.tsx` | Result count + inline bulk actions (when selection). | 100 |
| `EmptyStates.tsx` | The few empty states: corpus-empty, filter-empty, error. | 80 |
| `useCorpusBrowser.ts` | Hook that composes the surface's derived state: filters, sort, grouping, virtualization scaffolding. Pulls from `useFileBrowserStore` and exposes a narrower API to the components. | 260 |
| `useCorpusIdentity.ts` | Hook that derives the identity-band data: lede composition, type breakdown, counter values. | 140 |

### Files to modify

| File | Change |
|---|---|
| `stores/fileBrowserStore.ts` | Drop or deprecate: `savedSearches` + all related actions (confirm with Josh — §12 Q1), `activeSavedViewId` (keep `savedViews` storage but remove UI treatment), `sourceConnections` UI paths (storage stays; UI moves to Settings). `leftMode` state removed entirely. |
| `components/ContentViewer/...` | **KEEP unchanged.** The file-preview modal is orthogonal to this redesign. |
| `components/FileBrowser/FileIcon.tsx`, `FileTypeBadge.tsx` | Keep; small and token-clean. |
| `components/FileBrowser/LibraryToolbar.tsx` | **Delete.** Replaced by `CorpusCommandBar.tsx`. |
| `components/FileBrowser/SavedSearchControls.tsx`, `SavedSearchNameDialog.tsx` | **Delete** (conditional on Josh's call to deprecate saved searches — §12 Q1). |
| `components/FileBrowser/GridView.tsx`, `ListView.tsx`, `TreeView.tsx` | **Delete.** Replaced by the `CorpusBody.tsx` + `CorpusRow.tsx` pairing. |
| `components/FileBrowser/RenameDialog.tsx`, `ContextMenu.tsx` | Keep; both are small and isolated. Minor touch-ups to reference new row actions. |

### Files to delete (net)

- `GridView.tsx` (thumbnail grid — deprecated, §6.2)
- `ListView.tsx` (merged into `CorpusRow.tsx` with two density modes)
- `TreeView.tsx` (the 2,442-line beast — its responsibilities split across `CorpusBody`, `CollectionsStrip`, `CorpusCommandBar`, and moved-out surfaces)
- `LibraryToolbar.tsx` (replaced by `CorpusCommandBar.tsx`)
- Potentially: `SavedSearchControls.tsx`, `SavedSearchNameDialog.tsx` (contingent on Q1)
- Tests accompanying deleted files

### Cross-surface reuse

- **`CorpusIdentityBand` and `DashboardCorpusShape` share a data-shape.** The Dashboard block is narrower (three columns in 760px) and uses smaller typography; the FileBrowser band is wider (three columns in up to 880px) and uses `text-2xl` vs Dashboard's `text-3xl`. They're close enough that a shared `useCorpusIdentity` hook feeds both, but the presentational components stay separate (different scales, different spacing).
- **`useCorpusIdentity` hook — create in `components/Dashboard/` first** (per Dashboard spec) or in a shared `hooks/useCorpusIdentity.ts` location. Recommend the latter; both surfaces consume it.

### shadcn primitives used

- `Tooltip` — type-breakdown bar segment hover.
- `Select` — group-by, sort.
- `DropdownMenu` — overflow menu, row context menu.
- `Checkbox` — row selection, command bar type-chips when they need a third-state (defer).
- `Popover` — space-assignment picker.
- `Button` — actions.
- `ScrollArea` — horizontal collection strip scroll.

All present in codebase. No new primitive additions.

---

## 11. Scope boundaries

This spec is **NOT**:

- Changing backend APIs, indexing behavior, document metadata shape, or search logic. `listAllDocuments`, `readFileContent`, `deleteDocument`, `setDocumentsSpaceMembership` are unchanged.
- Changing the source-connection UI (moves to Settings — flagged as a separate spec).
- Implementing thumbnail rendering for any file type.
- Changing the `ContentViewer` modal behavior.
- Adding full-text search across the backend (we continue to do client-side batched content reads for content-search).
- Implementing a real "folder tree" navigation. The `Folder` group-by option approximates it; a true hierarchical tree is a follow-up if users demand it.
- Adding multi-select drag-and-drop between collections.
- Adding export-to-CSV / JSON (flagged as follow-up; mentioned in §5.1 overflow menu but not implemented here).
- Implementing smart-collection auto-derivation beyond what's already encoded (`isRecentDocument`, `isWebDocument`, category-bucketing). Those are preserved and reused.
- Adding a file-upload drop zone to the FileBrowser surface. Ingest happens in `/ingest`; dropping files on FileBrowser is reasonable UX but deferred — see IngestHub redesign spec.

---

## 12. Risks

1. **Deleting the three-mode tree is the most structurally aggressive change in any Recall redesign so far.** Recovery: the Collections mode's smart buckets become filter chips; projects become a group-by option; custom collections become the Collections Strip. Presets mode's saved-views survive as a quiet dropdown in the overflow menu. Sources mode moves to Settings (a follow-up). No capability is lost; every capability is re-homed. If review finds that users relied on a specific three-mode behavior (e.g., browsing by source path as the primary navigation), we re-add it as a sidebar-narrow variant — but default to the new flat layout.

2. **Virtualization + sticky group headers is non-trivial.** `@tanstack/react-virtual` can coexist with CSS `position: sticky` via careful offset math, but the integration has sharp edges (measuring, reflow on resize). Budget implementation time accordingly. If sticky headers don't land cleanly in the first pass, ship inline (non-sticky) headings and mark sticky as polish.

3. **Content search batching.** The existing scheduling at `FileBrowser.tsx:320-406` is subtle (sequence refs, cache, failure set). Must be preserved exactly when moving into `useCorpusBrowser`. Do not rewrite the scheduling logic — lift it as-is.

4. **`TreeView.tsx` deletion.** 2,442 lines of code that include subtle state interactions (collapsed collection IDs, saved-view draft state, source reconciliation). Each responsibility must have a new home verified before deletion. Implementation should delete in stages: first extract `CorpusBody` + `CorpusRow` and prove the list works without TreeView; then deprecate the left rail; then delete the file.

5. **Saved Searches deprecation.** If the answer to §12 Q1 is "keep," the command bar's overflow menu gets a second submenu and the store keeps its `savedSearches` slice. Acceptable fallback; doesn't break the surface.

6. **Grid view deletion.** Real risk if any user has a workflow that depends on thumbnail scanning (e.g., image-heavy vaults). Mitigation: detail-density rows show enough metadata for image files that a flat scan still works, and the `ContentViewer` click-to-preview closes the gap. If Josh wants Grid preserved, it becomes a third density option (`List / Detail / Grid`) — small scope expansion.

7. **The identity band's width-vs-content balance.** At the default 1280px viewport, the band looks rich. At 1920px+, it may feel sparse (~880px content on a 1920px canvas is center-aligned with significant whitespace). That's arguably correct — a knowledge substrate at scale should feel calm — but review may call for widening. Easy adjustment.

---

## 13. Tokens and patterns needed beyond TOKENS-SPEC

None required. TOKENS-SPEC is sufficient.

Patterns introduced here (not sibling-inherited):

- **Full-canvas (no sidebar) surface shape.** Distinct from Chat/Journal/References. Defensible because the content IS the navigation. First Recall surface to take this shape.
- **Corpus identity band.** Shared data shape with Dashboard; distinct presentation.
- **Collections Strip (conditional).** A horizontal scrollable chip row between identity and command bar. Only renders when data warrants.
- **Sticky group headings inside a virtualized list.** New for this surface; if it works, candidate for future re-use in long Chat threads (virtualization follow-up).

---

## 15. Oracle decisions (final — supersedes earlier defaults)

Two rounds of Oracle consultation locked in the following. Where this section conflicts with §3–§10 above, **this section wins.** Implementation must follow these:

### 15.1 Question resolutions

| # | Question | Decision |
|---|---|---|
| Q1 | Saved Searches | **DELETE ENTIRELY.** No pin rail, no naming dialog, no submenu. Remove `SavedSearchControls.tsx`, `SavedSearchNameDialog.tsx`, and the `savedSearches` slice from `useFileBrowserStore`. |
| Q2 | Grid view | **DELETE.** Thumbnails break typographic rhythm. Detail-density row metadata is enough. `ContentViewer` modal handles "look at it" intent. |
| Q3 | Auto-projects (path-derived clusters) | **KEEP as the `Folder` group-by option** in the Display menu. Reuse `extractProjectName` heuristic from old `TreeView.tsx:174-197`. |
| Q4 | Sources management | **MOVE to Settings.** Follow-up spec will cover the Settings "Sources" tab. `sourceConnections` storage stays in the store; UI paths on this surface are deleted. |
| Q5 | Identity band visibility | **Always visible, scrolls with body.** No fixed position. No collapse toggle. It sits at top and scrolls naturally. |

### 15.2 Saved Views — DELETED (Oracle round 1 correction)

The original spec (§3.5, §5.1) preserved Saved Views in the overflow menu. **Oracle: "Features hidden in overflow menus die."** The Command Bar is already fast enough that composing a view on-demand is not friction. `savedViews` slice stays in the store (so restore-after-migration doesn't break) but has **no UI surface.** Remove the Select from `LibraryToolbar` equivalents. No "Save this view…" item in the overflow menu.

### 15.3 Collections Strip — REPLACED WITH DROPDOWN (Oracle round 1 correction)

`CustomCollection` supports nesting via `parentId`. A horizontal strip cannot render a tree. **Replace the "Collections Strip" (`CollectionsStrip.tsx`) with a Collection filter dropdown** in the Command Bar, alongside Type and Source. Indented rendering handles nesting. Scales infinitely. Collection management (rename/delete/edit) via right-click on a dropdown row.

`CollectionsStrip.tsx` is **not created**. The component list in §10 is updated accordingly.

### 15.4 Command Bar — COLLAPSED TO 5 GROUPS (Oracle round 2)

The original §5.1 layout had 8 affordances (search + chips + source + group-by + sort + density + overflow, plus the new Collection dropdown from §15.3). That is toolbar hell.

**Final Command Bar composition (left → right):**

1. **Search input** (flex-grow, max 420px)
2. **Type chips** (All · PDFs · Web · Notes · Code · Media · Other — wraps at narrow widths)
3. **Source segmented control** (All / Local / Web)
4. **Collection filter dropdown** (replaces the old Strip; indented for nesting)
5. **Display dropdown** — single shadcn `DropdownMenu` containing the **Group-by**, **Sort**, and **Density** controls. Linear-style composition. Trigger label: `"Display"` with a `Settings2` icon. Inside: three labeled sections with radio groups.
6. **Overflow `…` menu** — now only contains `Refresh` and `Manage sources ▸` (links to Settings). No Save-view. No Apply-view.

Active filter indicator (`Clear filters` link) and row-right-aligned `N selected · Assign · Delete` remain unchanged from §5.2 / §6.1.

### 15.5 Identity Band — DROP BLOCK 3 (Oracle round 2)

The original §4.1 had three blocks: "What it is" (lede), "Shape" (type-breakdown bar), "Motion" (added-this-week / last-ingested / source-paths counters).

**Delete Block 3 entirely.** Dashboard owns temporal signal; FileBrowser owns spatial/structural signal. Redundancy kills both surfaces.

**Final Identity Band composition:**

- **Block 1 — What it is** (lede sentence + categorical subline). Unchanged from §4.1.
- **Block 2 — Shape** (type-breakdown bar + legend). Unchanged from §4.1.
- Grid ratio is now **1:1** (was 2:3:2). Two blocks, equal visual weight.
- At viewports <720px, the blocks stack vertically.

### 15.6 Adaptive Identity Band by corpus size (Oracle round 2)

Fixed thresholds for what the band renders:

| Corpus size | Band behavior |
|---|---|
| 0 docs | Per §4.3 — collapsed "empty vault" state with CTA button. |
| 1–9 docs | **"Nascent vault" state:** lede becomes `"A nascent vault. N documents."` Categorical subline + type-breakdown bar are **hidden** (percentages are meaningless at tiny N). No Block 2 at all. |
| 10+ docs | **Full band.** Lede with count + categorical subline, full Block 2 type-breakdown bar. |

Rationale: the type-breakdown bar at N=3 renders as "PDFs 67% · Notes 33%" which is pure noise. Withhold the signal until it has meaning.

### 15.7 FileBrowser stays read-only (Oracle round 2)

**Do NOT add a "New Document" button anywhere on this surface.** Browsing ≠ authoring. The pathways into the substrate are:

- Journal (text authoring)
- Ingest (/ingest — batch import, URL capture, folder mounts)
- OS-level drag-and-drop onto the canvas (existing behavior, preserved)

FileBrowser's job is to render what IS, not to create.

### 15.8 "Chat with this" — new row affordance (Oracle round 2)

The original spec treated FileBrowser like a Finder clone. Oracle flagged this: in an AI knowledge substrate, *finding* a document is the preamble to *querying* it. Click → ContentViewer is a dead end.

**Add a "Chat with this" action:**

- **On row hover:** in the same absolute-positioned right zone that contains Rename / Delete / Context (§6.2 List density), insert a `MessageCircle` icon button as the **first** of those hover actions. 14px, `--text-tertiary`, hover `--text-primary`. Tooltip: `"Start a chat about this document"`.
- **In ContentViewer header:** a primary-variant `Button` labeled `"Chat with this"` in the viewer's top-right, alongside any existing close/download controls.
- **Behavior:** opens a new conversation in Chat (via `useCreateConversation`) **pre-seeded with the document as a linked document** (via `addConversationLinkedDocument`). Routes to `/chat/:id` with the seed set.
- **Why:** threads the corpus-scanning surface directly into the app's primary throughput. Finding → asking becomes one click, not two navigations.

### 15.9 Updated file list (supersedes §10)

Remove from creation list:
- `CollectionsStrip.tsx` (superseded by Command Bar dropdown, §15.3)

Add a new responsibility:
- `CorpusDisplayMenu.tsx` — the combined Group-by / Sort / Density dropdown from §15.4, as a standalone component to keep `CorpusCommandBar.tsx` readable. ~120 LOC.

Total creations: **11 files, ~1,830 LOC target** (unchanged in aggregate — net-zero from removing Strip + adding Display menu).

---

## 14. Questions for Josh

1. **Saved Searches — keep, simplify, or delete?** The current `SavedSearchControls` is a pin rail + naming dialog + rename/duplicate/delete submenu. It's the most-visible legacy feature of the FileBrowser and the least-used based on typical information-tool telemetry. Spec default: **delete entirely**. Fallback: move to a `Saved…` submenu in the overflow menu. If pinned searches are important to your workflow, they survive as a strip below the command bar (similar to the Collections Strip). Confirm.

2. **Grid view — keep or delete?** Removing it saves ~500 lines and one view-mode toggle. The cost is any user who has an image-heavy vault and uses Grid for visual scanning. Spec default: **delete**. Fallback: add as a third density mode, not a separate view mode. Confirm.

3. **Auto-projects (path-derived clusters) — preserve as a group-by option?** The `extractProjectName` heuristic at `TreeView.tsx:174-197` creates pleasantly-named groups from file paths (`Users/josh/Documents/Recipes/...` → `Recipes`). Useful when the vault is organized by local folder. Spec default: **preserve as the `Folder` group-by option**. Alternative: delete the heuristic (if it's noisy in practice for non-Documents-organized vaults). Confirm.

4. **Sources management — Settings tab or right here?** Spec default: **move to Settings**. The management is rare; keeping it on the FileBrowser keeps the mental model heavier. Alternative: keep on FileBrowser as an overflow-menu entry that opens a dialog, not a sidebar mode. Confirm.

5. **Identity band — band stays always visible, or collapsible when the user scrolls the body?** Spec default: **always visible; scrolls with the body** (i.e., not fixed). Alternative: make the band collapsible via a chevron so users who want max list space can hide it. The collapse state persists per-session. Confirm whether the band is always-on or collapsible.
