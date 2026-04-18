# Journal Redesign — Implementation Specification

**Status:** proposed
**Sibling pilot:** `CHAT-REDESIGN-SPEC.md`. This spec inherits its design language; differences are stated explicitly.
**Paired with:** `AESTHETIC-GUIDE.md`, `TOKENS-SPEC.md`, `CHAT-REDESIGN-SPEC.md`.
**Scope:** `src/app/websrc/components/DailyNotes/**` and a tightly bounded set of supporting files (Chat sidebar journal-mode rendering already exists; we do not retouch it). No backend, no API, no data-model changes.
**Route:** `/journals` (canonical). `/daily` is a `Navigate` redirect that stays.

---

## 1. Map of the current surface

`DailyNotesWorkspace.tsx` is a single 3,062-line component that conflates two completely different products behind one shell:

1. **Notes mode** (`?` no params): a free-form pages workspace, one Tiptap entry per "page," sidebar lists pages, no concept of a journal.
2. **Journal mode** (`?journalSpaceId=...`): a date-anchored notebook bound to a `ConversationJournalDto` (a "space" of type journal). The sidebar swaps to journal-management UI; the main panel becomes a four-tab notebook (Pages / Highlights / Sources / Entries).

The route layer currently sends users to `journals` regardless of which they're after — there is no UI entry point to "Notes mode" anywhere in the app shell. The `!isJournalMode` branches throughout the file are vestigial. Confirmed by reading the route auto-redirect logic at lines 472–586 (it always coerces to a `journalSpaceId`).

### 1.1 Major regions (JSX)

| Region | Lines | What it is |
|---|---|---|
| Page background | 1926 | Radial-gradient mesh on `<div>` body wrapper. |
| Left sidebar (`<aside>`) | 1928–2127 | Header (logo + new-page) → journal switcher (`<select>`) + new-journal button → scrolling middle (notebook page card / sections grid / stat counters) → footer (delete-page in notes mode). |
| Main `<main>` header | 2129–2235 | Title input (acts as journal name in journal mode, page title in notes mode) + crowded button rail (Delete Journal, Synthesize Page, Synthesize Pinned/Deck, Highlight Selection, Save Now), under that: subtitle paragraph, save-state line, synthesis status line, save error, action notice. |
| Tab strip (duplicate of sidebar's tab grid) | 2237–2244 | Editor / Annotations / Documents / Chats. |
| Tab body | 2246–3057 | Branches on `activePanel`. |
| `editor` tab | 2247–2323 | Edit/Preview/Split control + TiptapEditor + TiptapViewer side-by-side. |
| `annotations` tab | 2325–2563 | Highlights/Stickies subviews. Inside Highlights: pinned-bookmarks list, captured-references list, notebook text highlights list. Stickies: 4-color sticky note cards. |
| `documents` tab | 2565–2793 | In journal mode: two-column source browser (citation aggregation across recent entries) + selected-source detail. In notes mode: file picker + checkbox link + preview pane. |
| `chats` tab | 2795–3055 | In journal mode: entry list + research-insights reader (assistant-only message stream from the selected conversation) with prev/next paging. In notes mode: chat picker + linked-checkbox + preview. |

### 1.2 Modals / dialogs / popovers used

- **One** Tauri-native confirm dialog (`tauriConfirm`) for "Delete journal." That's it. Everything else is inline UI. No Radix Dialog, no Popover, no Tooltip, no Tabs anywhere in this file.

### 1.3 Data flows the user can drive

- **Journals**: list, switch, create new, rename, delete (with destructive Tauri confirm).
- **Notebook page**: there is exactly one notebook page per journal in journal mode (a `WorkspaceNote` whose title defaults to `Journal · {spaceName}`); user can edit title and tiptap body. In notes mode: create / delete / switch any number of pages.
- **Highlights**: in journal mode there are *two* parallel highlight systems — (a) "Captured References" pulled from `listMessageBookmarks` filtered to journal entries, and (b) "Notebook Text Highlights" created from `window.getSelection().toString()` on the editor. Each can be pinned independently.
- **Sticky notes**: 4-color quick-thought cards stored on the WorkspaceNote.
- **Sources**: in journal mode, scans up to 24 recent entry conversations, parses assistant message metadata for `sources[]`, aggregates by `documentId|filePath|fileName`, ranks by reference count + score. User can open the source (file or web URL).
- **Entries** (= research conversations attached to the journal space): list, search, pin, select; render assistant-only messages as "insights"; "New Chat from Sources" creates a fresh conversation pre-linked to the entry's web sources; prev/next paging through the entry list.
- **Synthesis**: three scopes — Current Entry / Pinned (or Deck) — call `VaultAPI.synthesizeJournalEntries`, get back markdown synthesis, append it to the active notebook page with a `## Journal Synthesis · {Scope}` heading, switch view to Preview.
- **Auto-save**: 450ms debounced per-note, with `beforeunload` and `visibilitychange` flush handlers.
- **URL params**: `journalSpaceId`, `panel` (entries/pages/highlights/sources), `entryId`, `noteId` — drives initial selection; `appliedRequestedEntryKeyRef` ensures URL-driven selection only fires once per journal.
- **Linked-documents/conversations** (notes mode only): per-page checkbox-style attachment lists.

### 1.4 Decorative sins (with line numbers)

These are the violations we are correcting. Every single hex literal, gradient, and `bg-white/[0.0X]` is a TOKENS-SPEC violation. Sample, not exhaustive — the file has 169 raw-palette / gradient / blur usages by pre-sweep count:

- **Radial-gradient mesh canvas background** (1926): `radial-gradient(circle_at_5%_0%,rgba(16,185,129,0.16),transparent_35%),radial-gradient(circle_at_90%_8%,rgba(56,189,248,0.14),transparent_42%),linear-gradient(180deg,rgba(2,10,24,0.98),rgba(2,6,18,1))`. AESTHETIC-GUIDE §3 forbids gradient mesh backgrounds. ("If a screen needs a gradient, the layout is wrong.")
- **Glass-blur on sidebar** (1928): `bg-black/25 backdrop-blur-md`. AESTHETIC-GUIDE §3 forbids `backdrop-blur` as decoration. Default surfaces are opaque.
- **Glass-blur on header strip** (2130): `bg-black/15 backdrop-blur-sm`.
- **Cyan + amber gradient logo tile** (1932): `bg-gradient-to-br from-cyan-400/40 to-amber-300/40` — gradient identity tile. Forbidden.
- **Rainbow accent button system**: cyan for primary actions (1943, 2000, 2154, 2203), emerald for synthesis (2173, 2181, 2311), amber for highlight (2195), rose for delete (2120, 2165), sky/rose/mint/amber sticky color palette (101–105), amber for pin states (2380, 2398, 2848, 2910). TOKENS-SPEC §1.4 mandates one accent. The amber sticky palette uses literal `bg-amber-200/85`, `bg-sky-200/85`, etc. — direct Tailwind palette banned by TOKENS-SPEC §9.3.
- **Glow-style focus ring** (2154): `focus:shadow-[0_0_0_1px_rgba(34,211,238,0.25)]`. TOKENS-SPEC §8 mandates a single-color crisp outline; no halo box-shadow.
- **Bento-card lift treatment** (2099): `border-cyan-400/60 bg-cyan-500/15 shadow-lg shadow-cyan-900/20` for the active page card. AESTHETIC-GUIDE §3: no bento-card theatrics, no colored shadows.
- **Tab pill chip-with-glow** (1913–1923): `border-cyan-300/65 bg-cyan-500/20 text-cyan-100 shadow-[0_0_0_1px_rgba(34,211,238,0.25)]`. The active tab is given a tinted fill *and* a 1px ring shadow — duplicate elevation per AESTHETIC-GUIDE §6. Same pattern duplicated for `subviewButtonClass` (1918–1923).
- **Notebook section grid card** (2025) + **stat counter cards** (2056, 2060, 2066, 2072): `border-white/10 bg-white/5` rounded panels with uppercase micro-labels and big numbers. Bento-stat aesthetic. Each repeats `text-[10px] uppercase tracking-wide text-white/45` plus `text-sm font-semibold text-white/85` — the redundant double-treatment of weight and color that the aesthetic guide explicitly bans (§3 "bento-card-enhanced double-treatment").
- **Tinted insights container** (2938): `rounded-lg border border-white/10 bg-black/20 p-4`. Each "insight" is a card with a top-margin metadata strip (NotebookPen icon, "Insight N", timestamp). Reading-pane content rendered as cards instead of prose blocks (§3.1 of CHAT-REDESIGN — same sin we corrected for assistant messages).
- **Hardcoded `text-white/XX` everywhere** (2014, 2019, 2021, 2058, etc.): every text color in the file is `text-white/XX` instead of `--text-primary/secondary/tertiary/muted`. Ditto borders: `border-white/10`, `border-white/15`, `border-white/20`. TOKENS-SPEC §1.2 mandates `--border-subtle/default/strong`.
- **Ad-hoc native `<select>` styling** (1957–1973, 2519–2528): hand-styled with `appearance-none` and bg-tinted classes instead of the shadcn `Select` primitive that's already in use elsewhere.

### 1.5 The conflated responsibilities

Counting: even narrower than `ConversationSidebar`, but the conflation is across the WHOLE component, not just one region. `DailyNotesWorkspace.tsx` is responsible for:

1. Routing/redirect orchestration between notes mode and journal mode.
2. Notebook-page CRUD and auto-save lifecycle (debounce timers, beforeunload, visibilitychange).
3. Tiptap editor + markdown viewer container.
4. Highlights from text selection.
5. Captured-bookmark display and pin state.
6. Sticky-notes mini-app.
7. Linked-documents picker (notes mode).
8. Document preview (notes mode).
9. Chat picker (notes mode).
10. Journal entry list with pin/sort.
11. Research-insights reader (assistant-only message stream).
12. Source aggregation across entries (the `journalSourceSummaries` reducer).
13. Source detail panel + open-in-OS / open-in-browser.
14. "New Chat from Sources" creator (calls into `useConversationsStore`).
15. Synthesis pipeline (current/pinned/deck) + appending markdown blocks into the page.
16. Journal switcher / create-journal / rename / delete.
17. URL state management (panel, entryId, noteId, journalSpaceId).
18. localStorage persistence (last-journal, last-note-per-journal, three pin sets per journal).

Roughly 18 responsibilities. The Chat redesign called out 8+ for `ConversationSidebar.tsx`; this is over twice as bad in scope and coupling. The decomposition (§10) is therefore not optional — the redesign cannot land safely in a single 3,062-line file because every change risks regressing a different concern.

---

## 2. Product soul of Journal

A Journal in Recall is **a long-running notebook of your own thinking — a place where research conversations become entries, and where the editorial voice is your own, not the assistant's**. Where Chat is reading-first because the assistant produces most of the words, Journal inverts: the user produces most of the words, and the assistant's contribution arrives as already-distilled "insights" attached as entries. The reader is always you-later, scanning back through prose you wrote, looking for what you thought.

The current implementation has the right instinct — a notebook bound to a date-anchored conversation space, with synthesis as a way to fold many entries into one page — but it spends its visual budget on stat counters, sticky notes, and four parallel pin systems. The redesign keeps the powerful ideas (notebook page, entries-as-conversations, synthesis-into-page) and demotes everything else to either deletion or quiet panels.

Per AESTHETIC-GUIDE §1: "the interface is a lens onto the user's own material." In Journal, the user's material is overwhelmingly **editorial prose**. Serif body type at 16px / 1.6 is therefore not a flourish; it's the central typographic choice. Sans-serif is reserved for chrome — sidebar, metadata, controls. (CHAT-REDESIGN §3.3.)

---

## 3. Layout

### 3.1 Two-pane shell, mirrored from Chat

The shell is a **left sidebar (entry list) + main pane (editor with date header)**. Same column proportions as Chat. The four-tab structure (Pages / Highlights / Sources / Entries) is **deleted**. Each was a different problem masquerading as a different view of the same thing; the redesign breaks them apart spatially so each one lives where it belongs.

- **Left sidebar:** fixed `280px` (matching Chat). Collapsible to `56px` icon rail via `⌘\` (mirroring Chat — same shortcut is fine, the user is in only one view at a time). Persists collapse state in `localStorage` under `journal.sidebar.collapsed`.
- **Main pane:** flex-1, min-width `640px`. Single vertical scroll region. Contains: date/title header → editor body → optional trailing strips for entries/sources (see §6 and §8).
- **No header chrome on the main pane.** The current toolbar of Save / Highlight / Synthesize / Delete / etc. is gone. Save is automatic (it always was — the visible Save Now button just made the user anxious about it). Highlight and Synthesize move into a quiet inline action rail at the bottom of the editor (see §5.5). Delete moves into a sidebar-row hover action (§4.3).

### 3.2 Reading/writing column

Editor body uses **`max-width: 760px`**, identical to Chat's reading column. Same reasoning: 50–75 character measure, serif at 16px / 1.6 lands a 72ch line at ~720–760px. Centered within the main pane regardless of pane width.

Padding inside the column: `24px` left/right, `40px` top, `24px` bottom. (Top is bigger than Chat's because the date header lives there and wants the editorial breathing room.)

### 3.3 Sidebar behavior

Same as Chat (§2.3 of `CHAT-REDESIGN-SPEC.md`). Always visible at expanded default, instant tooltips on the collapsed icon rail, `⌘\` to toggle. When collapsed, the rail shows: new entry, journal picker, search, today-jump.

### 3.4 What we are NOT doing

- **Not** a calendar grid metaphor. The current product treats entries as conversations attached to a journal *space*, not as one-entry-per-day. Forcing a calendar grid would break that model. Calendar is a *navigator*, not the primary spatial metaphor (§7).
- **Not** a four-tab notebook anymore. Pages/Highlights/Sources/Entries are decomposed (§3.5).
- **Not** keeping the dual notes/journal mode. See §3.5.

### 3.5 Where the four tabs go

| Old tab | New home |
|---|---|
| **Pages** (`editor` tab) | The main pane is always the editor. There is no longer a tab for it. |
| **Entries** (`chats` tab) | The left sidebar — entries become first-class items in the entry list (§4). |
| **Highlights** | A trailing collapsible strip below the editor body, plus inline highlight-from-selection action (§5.4). The "captured references" subset moves into the entry detail (a chat-style citation footer per entry). |
| **Sources** | A trailing collapsible strip below the editor body, same treatment as Chat's "Sources in this conversation" footer (§3.6 of CHAT-REDESIGN). The "scan all 24 entries and aggregate" view is preserved as the *expanded* version of that strip. |
| **Sticky notes** | **Removed.** Stickies are an unfinished mini-app inside the journal that violates "content is the interface" — they are decoration with text in them. Highlights cover the same use case. (See §13 open question — confirm with Josh.) |
| **Notes mode** (the entire `!isJournalMode` branch) | **Removed.** No route currently delivers a user to non-journal mode; the auto-redirect at lines 472–586 always coerces to a journal. The dead branches account for ~600 lines of UI and the reason linked-documents / linked-conversations exist on `WorkspaceNote`. (Data model untouched; we just stop rendering the unused UI.) |

---

## 4. Entry list (sidebar)

### 4.1 Structure (top to bottom)

1. **Top rail** (48px). `--surface` background, hairline bottom border. Left: serif "Journal" wordmark at `text-sm` weight 600. Right: `PanelLeft` collapse toggle.
2. **Journal picker.** A single-line shadcn `Select` showing the current journal's icon + name. Opens a dropdown listing all journals + "New journal…" at the bottom. Replaces the bare `<select>` at lines 1957–1973 *and* the `New Journal` button at 1976–2005. Width: full sidebar minus 16px padding.
3. **New Entry button.** Full-width, `--accent` fill, `--accent-fg` text, `Plus` icon + "New Entry" label. Identical treatment to Chat's "New Conversation" button. `32px` height, `--radius-md`.
4. **Search input.** `text-sm`, 32px height. Same treatment as Chat sidebar's search (§5.1 of CHAT-REDESIGN). Placeholder: `"Search this journal..."`.
5. **Filter row.** Two filter chips, plus `All` default: `All` · `Pinned` · `Today`. Inactive state: `--text-tertiary`, no background. Active: `--text-primary`, 2px underline in `--accent`. The `Today` chip jumps the list to today's group and highlights it.
6. **Entry list** (flex-1). See §4.2.
7. **Footer** (32px, hairline top border): `Today` button on the left (jump-to-today), entry count on the right at `text-xxs` `--text-muted`.

### 4.2 Entry row

Each row represents one journal-entry conversation. Layout:

- **Height**: 56px minimum (slightly taller than Chat conversation rows because entries carry a date prefix).
- **Padding**: `10px 16px`.
- **Date prefix**: `text-xxs`, weight 500, `--text-muted`, uppercase tracking +loose, on its own line above the title. Formatted as `MON · APR 17` (`EEE · MMM d` via `date-fns`). For today's entries: literal string `TODAY`. For yesterday's: `YESTERDAY`. (This replaces the `formatWhen` "Apr 17, 4:32 PM" pattern.)
- **Title**: `--font-sans`, `text-sm`, `--text-primary`, line-clamp-1.
- **Time**: small `text-xxs` `--text-muted`, right-aligned next to title baseline. Just hours:minutes (e.g. `4:32 PM`) — the date already lives above.
- **Pin indicator**: if pinned, a 12px `Pin` glyph in `--accent`, inline-leading on the title row.
- **Hover**: `--surface` background, no border change. Hover-revealed action row: `Rename`, `Pin/Unpin`, `Delete`. Icon-only, 14px, `--text-tertiary`, hover `--text-primary` (or `--danger` for delete). Same pattern as Chat sidebar (§5.2 of CHAT-REDESIGN).
- **Active (selected) state**: `--surface` background **plus** a 2px left-bar in `--accent` flush with the row top/bottom, animated via `framer-motion`'s `layoutId="journal-sidebar-active-bar"`. Mirrors Chat's `sidebar-active-bar` pattern (`ConversationSidebar.tsx:1707`). Title color upgrades to `--text-primary` weight 500.

### 4.3 Grouping

Section headings between groups:

- `Today`
- `Yesterday`
- `This week`
- `This month`
- Then `Apr 2026`, `Mar 2026`, … by month, indefinitely backward.

Heading style: `--font-serif` italic, weight 500, `text-xs`, `--text-tertiary`, `8px 16px` padding, `16px` top-margin (except first group). Serif italic — not the sans uppercase used in Chat sidebar groups — to give the journal sidebar a quiet editorial accent that distinguishes it visually from Chat's sidebar without breaking the system. (Chat's grouping uses sans because conversations are not editorial; journal entries are.)

### 4.4 Pinned entries

Pinned entries surface as their own group at the very top, **above `Today`**, with heading `Pinned` in the same serif italic style. Pinning is a single-click on the row's hover-revealed pin icon, with optimistic UI; persistence stays in the existing `journal.pinnedEntries.{spaceId}` localStorage key (data path unchanged).

### 4.5 Empty state

Centered in the entry-list area, no card frame:

- `NotebookPen` icon at 32px, `--text-muted`.
- `text-sm` `--text-tertiary`: `"No entries yet."`
- `text-xs` `--text-muted` (max-width 240px, centered): `"Start a conversation in this journal — it'll show up here as a dated entry. Or write directly in the editor."`
- 16px vertical gap.

This is "teach the mental model" (CHAT-REDESIGN §5.5). The user needs to know that entries are conversations, not notes — that's the single most confusing thing about the product, and the empty state is the only place to teach it without drag.

### 4.6 Multiple journals

The journal picker (§4.1.2) handles switching. There is no "all journals" view — journals are scoped notebooks, and looking at all of them at once mixes contexts. (Confirmed: current product has no "all journals" view either.)

---

## 5. Editor surface (main area)

This is the heart of the redesign. Today, the editor is squeezed into one quarter of the screen behind a tab and three view-mode buttons (Edit/Preview/Split). The user's prose deserves the whole pane.

### 5.1 Container

- Centered reading column at `max-width: 760px`.
- `--bg` background (no panel tone, no border, no rounded card).
- The editor body fills the column. No frame, no drop-shadow, no double-pane split-view chrome.

### 5.2 Editor header

The "moment the journal earns its editorial feel." Three stacked elements at the top of the column, before the editor body:

1. **Date title.** `--font-serif`, `text-3xl` (30px per TOKENS-SPEC §3.2 — the ceiling), weight 600, tracking `-0.02em`, `--text-primary`. Format: `Friday, April 17` (`EEEE, MMMM d`). Bottom margin: `4px`. This is the only `text-3xl` element on the page; nothing else gets that scale.
2. **Year + journal name.** `--font-sans`, `text-sm`, `--text-tertiary`. Format: `2026 · {Journal Name}`. Bottom margin: `8px`. Interpunct separator (matching Chat metadata pattern, CHAT-REDESIGN §3.2). The journal name here is **not editable inline** (rename happens via the sidebar Journal picker's `…` menu).
3. **Metadata row.** `--font-sans`, `text-xs`, `--text-muted`. Three items separated by interpuncts: word count (`1,247 words`), last-edited timestamp (`edited 4m ago`), autosave indicator (see §5.6). Bottom margin: `32px` before the editor body begins.

The date is large and serif because it is the title of the entry. Subordinate metadata stays small and sans because it is chrome.

### 5.3 Editor body

- The existing `TiptapEditor` component is reused unchanged. We restyle the container only.
- Body wrapper applies `font-serif` so Tiptap inherits serif throughout. `tiptap.css` already handles internal prose styling (matches Chat — same pipeline). No change to tiptap.css needed; the serif is inherited from the wrapper.
- Min-height: `60vh`. The editor never collapses below that, even on a near-empty entry, so the writing surface always feels available rather than cramped.
- No edit/preview/split toggle. The editor IS the surface. Live preview is unnecessary because Tiptap renders inline as you type (it's WYSIWYG-ish); the existing tri-mode toggle is a relic of when the editor was thought of as a markdown box.
- **Removal note**: the entire `editorView` state (lines 431, 2253–2272), the `TiptapViewer` second pane (2295–2320), and all three Edit/Preview/Split buttons are deleted.

### 5.4 Highlights — inline, then trailing strip

Highlights happen in two places:

- **Inline highlight-from-selection** (replacing the current "Highlight Selection" button at lines 2192–2199). On editor focus, when the user has a non-empty text selection, a **floating mini-toolbar** appears just above the selection — `--surface-raised` background, `--border-subtle` 1px, `--radius-sm`, `--shadow-md`, `4px 8px` padding, single button: `Highlighter` icon + "Highlight" label at `text-xs`. Click adds the selection to highlights. This is editor-native: no need to click out to a tab, then back.
- **Trailing highlights strip**, below the editor body, separated by `48px` margin. Collapsed by default:

  ```
  3 highlights from this entry          [chevron]
  ```

  At `text-sm`, `--text-tertiary`. Click to expand. Expanded view: a flat list (not cards) of highlights, each one a serif blockquote (3px left-border in `--border-strong`, italic, `--text-secondary`, 16px left padding) with a footer row showing `formatWhen(createdAt)` + a `Pin` toggle + `Remove` text-button. Same blockquote pattern as `CHAT-REDESIGN §3.3` so prose excerpts read consistently across the app.

The "captured references" set (the `messageBookmarks` filtered subset) is **removed** from this strip — those belong to the entry conversation they came from, not the notebook page. They appear in the per-entry citation footer instead (see §6).

### 5.5 Inline action rail (bottom of editor)

A quiet bottom rail, `24px` below the highlights strip, hairline top border in `--border-subtle`, full reading-column width:

- Left side: `Synthesize` button. shadcn `Button` variant `ghost`, icon `Sparkles` 14px + label `Synthesize…`. Click opens a Radix `Popover` with three radio options: `This entry only` / `All pinned entries (N)` / `Recent deck (12)`. Submit button at the bottom of the popover triggers the existing synthesize flow.
- Right side: kbd hint `⌘S to save now · ⌘⇧H to highlight selection` at `text-xs` `--text-muted`. (Save still happens on every keystroke via debounce; the explicit shortcut just satisfies users who want the Linear-style key-press affirmation.)

No other buttons. Delete-entry lives on the sidebar row hover (§4.2). Delete-journal lives in the journal picker's `…` menu (§4.1).

### 5.6 Autosave indicator

A single quiet word, in the §5.2 metadata row. Three states cycle:

- **Idle** (no pending changes, last save >2s ago): `Saved`, `--text-muted`, `text-xs`. No icon.
- **Pending** (debounce timer active): `Saving…`, `--text-muted`, `text-xs`. No spinner — the word itself is the indicator. AESTHETIC-GUIDE §6: if animation is noticed, it's too much.
- **Error** (`saveError` set): `Save failed — retry?`, `--danger-fg`, `text-xs`. Click triggers `saveAllNow(true)`.

The current `Unsaved changes pending` (amber) / `All changes saved` (emerald) lines at 2215–2217 are replaced by this single line. The current always-visible `Save Now` button is removed; nothing about a desktop note app justifies a save button in 2026.

### 5.7 Empty state

If the entry's `content.trim()` is empty:

- Editor renders normally (Tiptap with placeholder).
- Tiptap placeholder text is overridden to: `"Start writing — just yourself, today."` Italic, `--text-muted`, serif (inherited from editor wrapper), `text-base`.
- The trailing strips (highlights, sources) are hidden until there's content.

---

## 6. Per-entry detail (the "Entries" tab, reborn)

When a user has selected an entry from the sidebar, the main pane is the editor for that entry's notebook page (the `WorkspaceNote` linked via the existing `journal.noteBySpace.{spaceId}` storage key — data path unchanged). But the entry is also a conversation with its own assistant insights and its own cited sources. Where do those go?

**Answer: a trailing "From this entry" region below the editor body, between the highlights strip and the action rail.** This region surfaces what the assistant contributed to *this* entry, separated from the user's own writing above it.

Layout:

- Hairline top border in `--border-subtle`, `48px` margin above.
- Header row: `text-sm` `--text-tertiary`, format `From this conversation · 5 insights · 8 cited sources`. Right-aligned chevron to expand the whole region.
- Collapsed by default. Expanded shows two sub-strips:

### 6.1 Insights (collapsed by default within the region)

A flat list of assistant messages from the entry's conversation, rendered with the **same Message component as Chat** (`components/Chat/Message.tsx`). Same serif body, same metadata row, same source citations footer. No "Insight 1 / Insight 2" cards. Replaces the current bento card list at lines 2937–2950.

This is a deliberate cross-component reuse: an "insight" is just an assistant message, and the Chat redesign already nailed how those should look. Importing `Message` into the Journal surface keeps both surfaces in sync forever.

Above the list, a single button: `Open in Chat`, `Button` variant `ghost`, `ArrowUpRight` icon. Navigates to `/chat?conversationId={entryId}` — gives the user the full chat affordances (composer, retry, branching, etc.) when they want them.

### 6.2 Sources for this entry

Same component as Chat's `SourceCitations` (`components/Chat/SourceCitations.tsx`), reused unchanged. Lists every `documentId|filePath|fileName` referenced by any assistant message in this entry. Click-to-open behavior preserved from current `openJournalSource` logic.

The "scan all 24 entries and aggregate" cross-entry source view from the current Sources tab is **preserved as a separate panel**, accessed via a header link on this section: `View sources across all entries →`. Clicking opens an inline expansion of the existing `journalSourceSummaries` list (the reducer at lines 1022–1090 stays — only the rendering changes). Two-column source-list + source-detail layout (current lines 2568–2698) is replaced by a single flat scrollable list that uses Chat's `SourceCitations` row pattern. The "Referenced in N entries" affordance becomes an inline `--text-muted` line below each source.

### 6.3 New chat from sources

The "New Chat from Sources" feature (current `startNewChatFromEntrySources` at lines 1745–1790) is preserved. It moves into a Radix `DropdownMenu` accessed from the `…` icon next to `Open in Chat`. Same icon, less prominent — it's an advanced affordance, not a primary one.

---

## 7. Date navigation

The user already has a chronological list in the sidebar (§4). The question is whether they need a calendar on top of that. Verdict: **a small, popover-only calendar accessed from a single icon in the sidebar top rail.** Not a persistent calendar.

- **Location**: a `CalendarIcon` button in the sidebar top rail (§4.1.1), right side, next to the collapse toggle.
- **Trigger**: click opens a Radix `Popover` anchored beneath the icon. Popover body uses the existing `react-day-picker` integration (`components/ui/DatePicker/DatePicker.tsx` is already in the codebase — reuse).
- **Behavior**: dates with at least one entry are dot-marked; click a date jumps the entry list to the first entry from that date and selects it. Today is pre-highlighted. Out-of-range dates (no entries before/after) are dimmed but still clickable (selects the closest entry).
- **Style**: `--surface-raised` background, `--border-subtle` 1px, `--radius-md`, `--shadow-md`, `8px` padding. Day cells follow the design tokens (no rainbow — just `--text-primary` for available dates, `--text-muted` for empty dates, `--accent` filled circle for today, 2px `--accent` outline for selected).

**Keyboard navigation between entries:**

- `J` / `↓`: next entry in list (older).
- `K` / `↑`: previous entry (newer).
- `T`: jump to today.
- `⌘⇧K`: open the date-picker popover.
- `⌘N`: new entry.

Mirrors Chat's keyboard-first stance (AESTHETIC-GUIDE §2.7).

---

## 8. States

| State | Render |
|---|---|
| **No journals exist** (first run) | Centered in the main pane (sidebar still renders). `NotebookPen` icon 40px `--text-muted`. Heading `text-xl` `--font-serif` `--text-primary`: `"Start a journal."` Body `text-sm` `--text-tertiary`, max-width 400px: `"A journal is a notebook of your own thinking. Conversations attached to it become entries you can return to."` Single button: `--accent` fill, `Plus` icon + `New journal`. Triggers the existing `createJournal` flow with default name. (Replaces current auto-create flow at lines 519–557 which silently creates a journal — visible empty state is honest.) |
| **Journal exists, no entries** | Editor pane renders with today's date in the header (a fresh notebook page is created via existing `defaultJournalTitle` flow — unchanged data path). Editor body shows the empty-state placeholder from §5.7. Sidebar empty state §4.5. |
| **Empty entry** | §5.7. |
| **Very long entry** | The reading column scrolls naturally inside the main pane. No scroll cap. The trailing highlights/from-this-entry strips stay at the bottom of the prose flow (not sticky). |
| **Many entries (>200)** | Same approach as Chat: do not virtualize in this pass. Flag if performance suffers. |
| **Entry with no insights yet** (conversation has only user messages) | "From this conversation" strip header reads `From this conversation · waiting for assistant`, `--text-muted`, no chevron, not expandable. |
| **Save error** | §5.6 inline `Save failed — retry?`. Plus a `toast` (existing `toastStore`) for the first occurrence. |
| **Synthesis in progress** | The Synthesize popover's submit button shows `Synthesizing…` with no spinner; popover stays open. On success: popover closes, the page editor scrolls to the appended synthesis block, brief toast `Synthesis added.` On failure: popover stays open, error message inline at the bottom of the popover in `--danger-fg`. |
| **Renaming an entry** | Inline rename in sidebar row, identical pattern to Chat's conversation rename (`ConversationSidebar.tsx:1744`-style inline input). |
| **Multiple entries on the same day** | Confirmed from code: an entry IS a conversation in the journal's space, and the user can create as many as they want per day. The date group simply lists them; the date prefix per row remains so the relative position is unambiguous within a long day. |

---

## 9. Search + filter

In scope, modest:

- The sidebar search input (§4.1.4) drives a client-side filter over entry titles, debounced 250ms. (The existing `conversationSearch` state and `filteredConversations` derived value are reused — same logic, restyled input.)
- The filter chips (§4.1.5): `All` · `Pinned` · `Today`. Three is enough; more chips are weight without payoff.
- Filtered/empty result state in the sidebar list area: `text-sm` `--text-tertiary` `"No entries match."` Centered, no card frame.

Out of scope: full-text search across entry bodies, tag filters (no tag concept in current data model — see §13), date-range filters (the calendar §7 covers that need).

---

## 10. Motion

Same vocabulary as Chat (CHAT-REDESIGN §8). Specifically:

**Animates:**

- Sidebar active-bar slide: `framer-motion` `layoutId="journal-sidebar-active-bar"`, transition `{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }`. Identical to Chat's `sidebar-active-bar`. Respect `useReducedMotion`.
- Hover-revealed sidebar action icons: `opacity` `--duration-fast`.
- Highlights / From-this-entry / Sources strip expand/collapse: height transition `--duration-base` `--ease-out`.
- Date-picker popover open: Radix default fade + `scale(0.98 → 1)`, `--duration-base` `--ease-out`.
- Synthesize popover open: same.
- Floating highlight mini-toolbar appearance: fade + `translateY(4px → 0)` (the only translateY in the spec — justified because it tracks the user's selection cursor), `--duration-fast` `--ease-out`.

**Does NOT animate:**

- Entry switch. The editor body just changes content. No fade, no slide. (Spec call defended: a fade adds latency between thought and word, and the user is always switching deliberately. Instant-swap matches code editors and Linear.)
- Page mount. No staggered entrance.
- Pin/unpin toggle: instant color/icon change, no scale or pulse.
- Save state transitions: instant.
- Sidebar row hover: instant `--surface` background.

---

## 11. Component ownership

### Decomposition strategy: **decompose while redesigning**, not before, not after

I considered all three orderings and chose this one. Defense:

- **Restyle in place first**: would require touching 3,062 lines of soup, restyling each region in situ, then a second pass to extract. The first pass would be high-risk because every visual change ripples through state held by the parent. The second pass would be redundant (we'd have re-touched everything).
- **Decompose first, restyle after**: pure refactoring without redesign carries no immediate user value and is the kind of change that's hard to justify shipping. Also, decomposing without knowing the new design risks extracting the wrong boundaries (e.g., extracting an `EditorTabs` component we'd then delete in the redesign).
- **Decompose while redesigning** (chosen): each new file is born in the new design language. We extract a region, restyle it as we extract, and delete the old region from the parent. Risk is lowest because the parent shrinks monotonically as new files appear. The new `DailyNotesWorkspace.tsx` (the shell) ends up at ~150–200 lines. Each leaf component is `<300` lines.

Files that are extracted but not yet rewritten remain in the old style temporarily (no surface ships in mixed-state — strict rule: each PR ships one or more components fully redesigned). Modular-builder may break this into 3–5 PRs as makes sense.

### Files to create (in `components/Journal/`)

Note: the directory rename `DailyNotes/` → `Journal/` is part of this redesign. The file currently lives at `components/DailyNotes/DailyNotesWorkspace.tsx`; the new home is `components/Journal/JournalWorkspace.tsx`. Update the route import in `routes.tsx` accordingly. Old `DailyNotes/` directory is deleted.

| File | Responsibility | Approx LOC |
|---|---|---|
| `JournalWorkspace.tsx` | Shell. Reads URL params, drives data loading, renders `EntryList` + `EntryEditor`. No JSX beyond the shell layout and a couple of error/loading guards. | 150–200 |
| `EntryList.tsx` | The whole left sidebar: top rail, journal picker trigger, new-entry button, search, filter chips, grouped list, footer. | 250–300 |
| `EntryListItem.tsx` | Single entry row with date prefix, title, time, pin, hover actions, active state with `layoutId` bar. | 150 |
| `EntryEditor.tsx` | Main pane container — header (date title, year+journal, metadata row), TiptapEditor wrapper, trailing strips orchestration. | 200–250 |
| `EntryHeader.tsx` | The serif date title + year/journal subline + autosave-aware metadata row. Pure presentation. | 80 |
| `EntryAutosaveIndicator.tsx` | The single quiet word. Reads `hasPendingChanges` / `isSavingNow` / `saveError` props. | 40 |
| `EntryHighlightsStrip.tsx` | Trailing collapsible highlights region + the floating selection mini-toolbar (see §5.4). The mini-toolbar is co-located here so the data flow stays local to highlights. | 200 |
| `EntryFromConversation.tsx` | The "From this conversation" trailing region. Uses Chat's `Message` and `SourceCitations`. Holds the cross-entry `View all sources` expansion. | 180 |
| `EntryActionRail.tsx` | Bottom rail: Synthesize popover trigger + kbd hint. | 80 |
| `JournalPickerMenu.tsx` | The sidebar journal picker. shadcn `Select` if simple list works; if we need the rename/delete-journal items in the same surface, switch to shadcn `DropdownMenu`. | 120 |
| `JournalCalendarPopover.tsx` | The date-picker popover (§7). Wraps the existing `components/ui/DatePicker/DatePicker.tsx`. | 100 |
| `SynthesizePopover.tsx` | The synthesize affordance. Three-radio popover, calls `synthesizeJournalEntries`. | 120 |
| `useJournalEntries.ts` | Hook that owns the entries-loading lifecycle: `listJournalConversations`, the URL-driven initial selection, the search/filter/group/sort derivations. Pulled out of the workspace so the shell stays small and the testable surface is narrow. | 200 |
| `useJournalNote.ts` | Hook that owns the per-journal `WorkspaceNote` lifecycle: load, debounced auto-save, `beforeunload` flush. | 150 |
| `useJournalSources.ts` | Hook that owns the cross-entry source aggregation (the existing reducer logic at 1022–1090, lifted as-is). | 100 |

### Files to modify (outside Journal/)

| File | Change |
|---|---|
| `routes.tsx` | Update import path: `./components/DailyNotes` → `./components/Journal`, export `JournalWorkspace` instead of `DailyNotesWorkspace`. The `/journals` and `/daily` route entries stay; only the lazy import changes. |
| `components/Layout/Layout.tsx` | The nav label `"Daily Notes"` (referenced by Layout test fixtures) stays as `"Journals"` (already updated for current shipped state). No change needed if already migrated; verify. |
| `components/Layout/__tests__/Layout.test.tsx` | If still asserting `"Daily Notes"`, update assertion strings. |
| `components/Chat/Message.tsx`, `components/Chat/SourceCitations.tsx` | Confirm exports are stable for cross-surface reuse. No code change expected. If `Message` requires Chat-specific store context that doesn't apply in journal mode, refactor accordingly — but verify before assuming this. |
| `components/TiptapEditor/tiptap.css` | No change. Inherits serif from parent wrapper. |

### Files to delete

- All of `components/DailyNotes/` (after the new `components/Journal/` is wired up and the route swap lands).

### shadcn primitives to use

All present in the codebase (verified: `Dialog`, `Popover`, `Select`, `Tabs`, `Checkbox`, `Tooltip`, `ScrollArea`, `Button`, plus `react-day-picker` and `framer-motion`):

- `Popover` — date picker, synthesize popover, citation footnotes (via Chat's `CitationFootnote`).
- `Select` (or `DropdownMenu` if added) — journal picker.
- `Button` — every button.
- `ScrollArea` — sidebar entry list, expanded source lists.
- `Tooltip` — kbd hints, icon-only action hover.

No new primitive additions are required.

---

## 12. Scope boundaries

This spec is **NOT**:

- Changing the data model. `WorkspaceNote`, `ConversationJournalDto`, `ConversationMessageBookmarkDto`, and the entire `dailyNotes` API surface stay. Renaming the React directory does not rename the API.
- Changing any `VaultAPI` calls or hooks signatures. `useConversationsStore` continues to be the source for live conversation state.
- Changing tiptap config or extensions. Both Chat and Journal consume the same `TiptapEditor`.
- Touching backend / Tauri commands.
- Adding tags. There is no tag concept in the current data model (verified — see §13). If tags are added later, they get a follow-up spec.
- Adding full-text search across entry bodies.
- Implementing a published-vs-draft distinction. There is none in the data model; entries are always live (verified).
- Re-styling the Chat sidebar's journal-mode rendering. The Chat sidebar already renders journal entries with its own grouping (`isJournalScope` at `ConversationSidebar.tsx:1716`); the Chat redesign shipped that. We do not re-touch it.
- Removing the sticky-notes data field on `WorkspaceNote`. We just stop rendering the UI. The field is data debt and can be cleaned up in a separate pass once we're confident no users rely on the feature.

---

## 13. Risks

1. **The 3,062-line file.** Strategy is decompose-while-redesigning (§11). Every PR ships fully-redesigned components only — no half-state in the merged tree. Manual smoke test before each PR: open journal, switch entries, edit body, save, highlight, synthesize, switch journals, create journal, delete journal. The risk is real but bounded by the PR cadence.
2. **Tiptap state preservation across entry switches.** Currently the editor remounts on every `activeNote` change, which loses cursor position and undo history. The redesign should preserve that behavior (it's actually the right call — switching entries should reset undo because each entry is conceptually a fresh document). Flag during implementation if a different approach is wanted.
3. **Cross-surface `Message` reuse.** Importing Chat's `Message.tsx` into Journal couples the two surfaces. If the Chat Message component requires a `useConversationsStore` context that doesn't apply in journal mode (e.g., for the bookmark/delete actions that target a *live* conversation), we'll need a `MessageView` variant that takes the message data as plain props without the live-conversation context. This is a real risk; verify during implementation before assuming it works clean. Fallback: extract a leaner `MessageView` wrapper for read-only contexts.
4. **The "Notes mode" deletion.** Removing the `!isJournalMode` branches deletes ~600 lines of UI that no current entry point reaches but that the data model still supports (the linked-documents and linked-conversations checkbox flows). If anyone has a workflow that depends on landing on the workspace without a `journalSpaceId`, they'll lose it. Audit before deletion: grep the codebase for any URL or navigation that builds a `/journals` URL without a `journalSpaceId`. Currently I see only the auto-redirect (which always *adds* a `journalSpaceId`), but verify.
5. **Sticky notes deletion.** Same risk class as above. Confirm with Josh that the sticky-note feature is really unused / unloved before removing the UI. If it's loved, it has to be redesigned to fit the system, not removed.
6. **Synthesis flow.** The current synthesis appends a markdown block to the active note. The redesigned flow keeps that exact behavior. But the markdown block uses `## Journal Synthesis · Current Entry` which becomes an `<h2>` in the rendered editor. Verify in the new editor that h2 renders well in serif at the prose scale (Tiptap's `ProseMirror` styling should handle it; `tiptap.css` is shared and was tuned for Chat).
7. **Layout test breakage.** The directory rename `DailyNotes` → `Journal` will cause test fixture string literals (Layout test mentions `"Daily Notes"`) to fail until updated. Trivial fix, but flag for the implementing PR.

---

## 14. Open questions requiring user input

The codebase verified some, but these need Josh's call:

1. **Sticky notes — keep, redesign, or delete?** The data model has them. The current UI is a 4-color quick-thought card that violates the system (raw amber/sky/rose/mint Tailwind palette). My recommendation is **delete the UI** (data field stays in case of future use) on the grounds that highlights cover the same use case and stickies are an unloved sub-feature. But this is the kind of call that benefits from a "yeah, kill it" or "wait, I use those." (Default if no answer: delete the UI, keep the field.)
2. **"Notes mode" — confirm dead?** I traced the auto-redirect and find no entry point that lands on the workspace without a `journalSpaceId`. Confirm before I delete the ~600 lines of dead branches. (Default if no answer: delete.)
3. **Date title format**: `Friday, April 17` (long) vs `Fri · Apr 17, 2026` (compact) vs `April 17, 2026` (Sunday Times). My pick is `Friday, April 17` because the year duplicates in the metadata row and the day-name carries warmth. Easy to flip.
4. **Multiple entries per day**: confirmed in code that an entry is a journal-space conversation and the user can create N per day. Should the date prefix on entry rows show the time too (e.g. `TODAY · 2:14 PM`), or stay `TODAY` with time on the right? Spec'd as `TODAY` with time right-aligned (§4.2); confirm.
5. **Calendar popover**: do we want this at launch, or defer? Adding it costs maybe a day. Without it, date navigation is sidebar-only (which is fine for daily use, awkward for "jump to last March"). Spec'd as in-scope; confirm.
6. **`Open in Chat` button placement**: per §6.1 it's in the From-this-entry strip. Alternative: put it in the editor header next to the journal-name subline (one click further forward). Spec'd as in-strip because the strip is where the entry's chat-side material lives; one place for everything.
7. **Cross-surface `Message` reuse**: this is the technical question from §13.3, but it's also a product question — do we want the assistant insights in the journal to look *exactly* like Chat's assistant turns (citations footer, copy/bookmark/delete actions), or a more reading-oriented variant? Default: exact reuse. Alternative: read-only variant (`MessageView`) without the actions row. Recommend exact reuse, because cross-surface consistency reduces learning load.
