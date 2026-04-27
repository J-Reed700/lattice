# References Redesign — Implementation Specification

**Status:** proposed
**Sibling pilots:** `CHAT-REDESIGN-SPEC.md`, `JOURNAL-REDESIGN-SPEC.md`. This spec inherits their design language; differences stated explicitly.
**Paired with:** `AESTHETIC-GUIDE.md`, `TOKENS-SPEC.md`.
**Scope:** `src/app/websrc/components/ReferenceInbox/**` only. No backend, no API, no data-model, no hooks, no store changes.
**Route:** `/references` (`Layout.tsx` nav already labels this "References" — confirmed).

---

## 1. Map of the current surface

### 1.1 Files

| File | LOC | Purpose |
|---|---|---|
| `ReferenceInbox/index.ts` | 1 | Re-exports `ReferenceInbox`. |
| `ReferenceInbox/ReferenceInbox.tsx` | 791 | **Entire feature in one component.** State, data loading, list, detail, annotation editor, capture pipeline, filtering, search, navigation — all inline. No sub-files, no hooks, no separation. |

One file, 791 lines, every responsibility. For comparison: the Journal redesign took a 3,062-line file down to a shell + 15 leaf files; ReferenceInbox is smaller but equally monolithic relative to its scope.

### 1.2 UI regions (current)

| Region | Lines | What it is |
|---|---|---|
| Page background | 499 | Radial-gradient mesh on the root `<div>`. |
| Header | 501–578 | Crowded band: kicker + title + Refresh/Capture-Pending buttons + search + two filter pill rows + 3-count stat line. All on one visual ribbon. |
| Sidebar list (`References`) | 582–637 | Left column 22rem wide. Panel with label row, scrolling list, loading/empty states, cyan-bordered active row. |
| Detail panel | 639–785 | Right column. Metadata pills row, Capture Destination card, title/note drafts, 5-button action grid, captured-note link, rendered message content viewer. |

### 1.3 Data flows the user can drive

Verified by reading the component end-to-end:

1. **List bookmarks** (`listMessageBookmarks`) with optional text query, filter by role (`all/assistant/user/system`) and status (`all/pending/captured`).
2. **Select a reference** — shows detail. Cache payload fetch (full message body via `resolveBookmarkPayload` → `getConversationMessages`).
3. **Annotate** — edit title and note (in-memory drafts → `bookmarkConversationMessage` upsert on Save; same endpoint used by Chat's bookmark action).
4. **Capture one** — write the message body into a WorkspaceNote (journal page if the bookmark's space is a journal, else a daily `Research Inbox · YYYY-MM-DD` note). Via `captureChatReferenceToWorkspaceNote`. Records a `capture_…` snapshot so future renders know it's captured.
5. **Capture pending (bulk)** — captures up to 24 non-captured bookmarks in the current filtered list. Serial loop with per-item error catch.
6. **Open in Chat** — navigates to `/chat?conversationId=…&messageId=…`.
7. **Open captured note** — navigates to `/journals?noteId=…&snapshotId=…[&journalSpaceId=…]`.
8. **Copy** — copies the full message content to clipboard.
9. **Remove** — `unbookmarkConversationMessage` (deletes the bookmark; does not touch any captured note already written).
10. **Refresh** — re-runs all four loads (bookmarks, notes, spaces, journals).

### 1.4 Decorative sins (cite `file:line`)

Every item below violates TOKENS-SPEC or AESTHETIC-GUIDE. Confirmed by reading the source:

- **Radial-gradient mesh canvas background** — `ReferenceInbox.tsx:499`. Two radial gradients (emerald + cyan) layered on the page body. AESTHETIC-GUIDE §3: "No gradient mesh backgrounds."
- **Glass-tinted panels via `hsl(var(--overlay))` as a panel surface** — 501 (header), 582 (list), 639 (detail). `--overlay` is defined in TOKENS-SPEC §1.6 as a modal scrim, not a panel bg. Using it here fakes the old glass look while nominally touching a token. Needs `--surface`.
- **Rainbow accent system (two accents on one screen: cyan + emerald + amber + rose)**:
  - Cyan for "selected" and filter active — 550 (`text-cyan-100`, `bg-cyan-500/20`), 606–608 (`border-cyan-300/45 bg-cyan-500/10`), 613 (`text-cyan-200/90`), 705/712 (`focus:ring-cyan-300/65`), 720 (`border-cyan-300/35 text-cyan-100`), 740 (arrow nav).
  - Emerald for "captured" / primary action — 504 (`text-emerald-100/70`), 521/527 (`border-emerald-300/40 bg-emerald-500/15`), 565 (`bg-emerald-500/20 text-emerald-100`), 617–620 (captured pill), 662–664 (captured pill in detail), 685/728/746/762 (capture buttons, checkmark, captured-note button).
  - Amber for "pending" — 667–669 (`border-amber-300/40 bg-amber-500/14 text-amber-100`), 691 (warning text).
  - Rose for destructive — 752 (`border-rose-300/35 text-rose-200`).
  - TOKENS-SPEC §1.4: **one accent** (violet). State colors come from `--success-*`, `--warning-*`, `--danger-*`. No other hues.
- **`text-white/XX` everywhere instead of text tokens** — 507, 513, 535, 540, 551, 555, 566, 570, 576, 588, 593, 622, 624, 629, 631, 650, 653, 656, 658, 670, 675, 678, 686, 691, 710 (and more). Direct opacity on white; TOKENS-SPEC §1.3 mandates `--text-primary/secondary/tertiary/muted/disabled`.
- **`border-white/XX`, `bg-white/[0.0X]`** — 501, 513, 541, 558, 582, 592, 605, 608, 637, 639, 645, 674, 701, 705, 713, 737, 744. TOKENS-SPEC §1.2 mandates `--border-subtle/default/strong`.
- **Tinted card panel for rendered message content** — 769 (`rounded-lg border border-white/10 bg-black/25 p-3`) with a nested tinted box at 776 (`border border-white/10 bg-[hsl(var(--overlay))]`). AESTHETIC-GUIDE §3: bento cards banned.
- **Ad-hoc focus halo via `focus:ring-emerald-300/60`** — 540, 705, 712. TOKENS-SPEC §8 mandates a single crisp `--ring` outline; no halo.
- **Hand-styled `<input>` and `<textarea>` elements** — 536–541 (search), 701–706 (title), 707–713 (note). Each with its own classes instead of a shared input treatment. Chat uses shadcn primitives; Journal uses a repeated shared treatment pattern; here, each input is bespoke.
- **Hand-rolled filter pill group with tinted active state** — 544–556 (role), 559–571 (status). Chat pattern is border-bottom underline, not tinted pill.
- **Uppercase kicker `Integrated Retrieval`** — 504. The "Phase 4 voice pass" already retired feature-category kickers on other surfaces; this is a stale instance.
- **"Detail" / "References" small-caps panel labels** — 584, 641. Inside-panel labels are dead chrome. Sibling specs put no label on panels (the panel's position says what it is).
- **Stat counter strip** — 576 (`{total} total · {pending} pending · {captured} captured`). Bento-stat aesthetic. AESTHETIC-GUIDE §3: "no bento-card theatrics."

Net count: ~40 direct raw-palette / opacity-white / glass / gradient usages in 791 lines.

### 1.5 Conflated responsibilities

In one file:

1. Data loading orchestration (bookmarks, notes, spaces, journals — all in one `Promise.all`).
2. In-memory payload cache (`payloadCache`).
3. Captured-reference index building + maintenance.
4. Query debounce.
5. Filter/sort/derive stats.
6. Destination resolution (journal vs. daily inbox).
7. Annotation draft state + save.
8. Single-capture and bulk-capture flows.
9. Remove-bookmark flow.
10. Clipboard copy.
11. Navigation side-effects (`navigate('/chat…')`, `navigate('/journals…')`).
12. List rendering.
13. Detail rendering (metadata, destination, editor, actions, viewer).

Same class of problem as pre-redesign Journal: the whole surface is one state soup. Decomposition is part of the redesign, not a follow-up.

---

## 2. Product soul of References

References is **the reading room for the fragments you chose to keep**. It is a commonplace book, not an email inbox. When the user reaches for a reference, they are not triaging — they are *revisiting*. They are asking "what did I want to remember, again?" and answering it by reading, not by sorting.

The current product tries to be both a commonplace book *and* a processing queue — the "Pending / Captured" status, the "Capture Pending" button, the "Capture Destination" breadcrumb. That inbox paradigm fights the editorial one. But capture-to-journal is a real, used feature (snapshots written into WorkspaceNotes, surface-linked to `/journals` routes, covered by unit tests). We can't remove it without losing the bridge from Chat's bookmark gesture to the Journal's notebook page.

**Resolution:** References keeps capture as a *quiet affordance*, not the organizing metaphor. The primary experience is reading. Capture is one action among several (copy, open in Chat, open captured note, delete) available on the current reference. No "Capture Pending" batch button. No prominent "pending vs captured" filter. A small captured-dot indicator in the list and a one-line capture status in the detail, and that's it.

Per AESTHETIC-GUIDE §1: "the interface is a lens onto the user's own material." In References, the material is short excerpts — individual messages, sometimes a few paragraphs, rarely more — that the user saved. Serif body at 16px / 1.6 is therefore the central typographic choice, inherited from Chat (§3.3 of `CHAT-REDESIGN-SPEC.md`) so that a referenced assistant message reads *exactly* as it did in Chat. Cross-surface typographic consistency is the point: a reference is a message, lifted out of its conversation and placed in a reading room.

---

## 3. Layout

### 3.1 Two-pane shell, mirrored from Chat and Journal

The shell is a **left sidebar (reference list) + main pane (reader)**. Same column proportions as Chat and Journal.

- **Left sidebar:** fixed `280px`. Collapsible to `56px` icon rail via `⌘\` (same shortcut as Chat/Journal; the user is in only one view at a time). Persists collapse state in `localStorage` under `references.sidebar.collapsed`.
- **Main pane:** flex-1, min-width `640px`. Single vertical scroll region. Contains: reference header (origin, timestamp) → referenced message body → annotation strip → action rail.
- **No top-of-page header chrome.** The current Refresh / Capture Pending / kicker / title / stats band is deleted. Refresh is implicit (polled on mount; manual refresh via `⌘R`-equivalent is a sidebar-footer affordance if explicitly needed, otherwise omitted). Capture Pending is deleted (see §10 open questions). The visible title "Reference Inbox" is also deleted — the sidebar's wordmark says "References" and that is the title.

### 3.2 Reading column

The referenced-message body uses **`max-width: 760px`**, identical to Chat's and Journal's reading column. Same reasoning: serif at 16px / 1.6 lands a 72ch line at ~720–760px. Centered within the main pane.

Padding inside the column: `24px` left/right, `40px` top, `24px` bottom. Top is bigger than Chat's per-message padding because the reference header (origin, timestamp) lives there and wants editorial breathing room, identical to Journal's date-title treatment (JOURNAL-REDESIGN §3.2).

### 3.3 Sidebar behavior

Identical to Chat (§2.3 of `CHAT-REDESIGN-SPEC.md`) and Journal (§3.3 of `JOURNAL-REDESIGN-SPEC.md`). Always visible at expanded default, instant tooltips on the collapsed rail, `⌘\` to toggle. When collapsed, the rail shows: new-reference-from-clipboard (deferred — see §10), search, origin-filter, captured-filter.

### 3.4 What we are NOT doing

- **Not** a grid/gallery layout. References are short text excerpts, not visual artifacts.
- **Not** a three-column layout (sidebar + list + detail). The Chat precedent is two-pane; we match it. The original three-region header band (global header, list, detail) becomes two regions.
- **Not** a top-bar stats ribbon. Stats ("total · pending · captured") are not a read-oriented need.

---

## 4. Reference list (sidebar)

### 4.1 Structure (top to bottom)

Mirrors the Journal `EntryList` pattern almost exactly (JOURNAL-REDESIGN §4.1) so the three surfaces — Chat, Journal, References — read as a family. Differences are spelled out per-region below.

1. **Top rail** (48px). `--surface` background, hairline bottom border. Left: serif "References" wordmark at `text-sm` weight 600. Right: `PanelLeft` collapse toggle. (No calendar icon — References are not date-navigated; see §7.)
2. **Origin picker.** A single `Select` (shadcn) showing the current origin filter. Options: `All references` / `From Chat` / `From Journal entries`. The origin filter replaces the current role-tab filter (`All / AI / You / System`) because origin is the primary way a user thinks about a saved fragment, not its message role. Role is a secondary signal at best — the user rarely thinks "I want to find that *user* message I starred." Width: full sidebar minus 16px padding. Mirrors the Journal picker's position and treatment.
3. **Search input.** `text-sm`, 32px height. Same treatment as Chat sidebar search (CHAT-REDESIGN §5.1) and Journal sidebar search (JOURNAL-REDESIGN §4.1.4). Placeholder: `"Search references..."`. Debounced 250ms (existing pattern).
4. **Filter chips.** Three chips with the same underline-on-active treatment as Chat/Journal: `All` · `Recent` · `Captured`. Active: `--text-primary`, 2px underline in `--accent`. Inactive: `--text-tertiary`, no background.
   - `All` — every reference (default).
   - `Recent` — jumps the list to the "This week" group and selects its first item. A navigational filter, not a true filter.
   - `Captured` — shows only references that have already been captured to a note. The pending/captured distinction is preserved here, but demoted from the primary filter axis (role) to a tertiary chip. See §10 open questions.
5. **Reference list** (flex-1). See §4.2.
6. **Footer** (32px, hairline top border): reference count on the right at `text-xxs` `--text-muted`. No stat breakdown, no capture-pending action. (Jump-to-today removed — references aren't date-anchored in the way journal entries are.)

### 4.2 Reference row

Each row represents one bookmarked message. Layout:

- **Height**: 56px minimum, allows title to grow to 2 lines if needed (cap at 2 lines, ellipsized).
- **Padding**: `10px 16px`.
- **Top line — origin + date prefix**: `text-xxs`, weight 500, `--text-muted`, uppercase tracking +loose, on its own line above the title. Format: `CHAT · TODAY` or `JOURNAL · MON · APR 17`. The origin comes first because the user is often scanning for "the thing from chat yesterday." For today's references: literal string `TODAY`. For yesterday's: `YESTERDAY`. Older: `EEE · MMM d` (e.g. `MON · APR 17`). Same date-prefix convention as Journal (JOURNAL-REDESIGN §4.2) so users don't context-switch.
- **Title**: `--font-sans`, `text-sm`, `--text-primary`, line-clamp-1. Derived as in current code: `bookmark.title || bookmark.conversationTitle`. If the reference has no title and the conversation title is also falsy, render `Untitled reference` at `--text-muted`.
- **Preview line**: `text-xs`, `--text-secondary`, line-clamp-1. Content: `bookmark.messagePreview`. Serves as the quick-scan affordance — this is what the user reads to decide whether to click.
- **Time**: `text-xxs`, `--text-muted`, right-aligned next to the title baseline. Hours:minutes only (e.g. `4:32 PM`). The date already lives above.
- **Captured indicator**: if the reference is captured, a single `BookmarkCheck` glyph (or `Check`) at 12px in `--accent`, inline-leading on the title row. Replaces the current "CAPTURED" pill badge. The pill took pixels a dot can do with less shouting.
- **Hover**: `--surface` background, no border change. Hover-revealed action row on the right (opacity 0 → 1 `--duration-fast`): `Open in origin` (arrow-up-right icon), `Copy` (copy icon), `Delete` (trash icon). Icon-only, 14px, `--text-tertiary`, hover `--text-primary` (or `--danger` for delete). No background pills.
- **Active (selected) state**: `--surface` background **plus** a 2px left-bar in `--accent` flush with row top/bottom, animated via `framer-motion`'s `layoutId="references-sidebar-active-bar"`. Mirrors Chat (`ConversationSidebar.tsx:1707`) and Journal (`EntryListItem.tsx:89`). Title color upgrades to `--text-primary` weight 500.

### 4.3 Grouping

Section headings between groups, same vocabulary as Journal:

- `Today`
- `Yesterday`
- `This week`
- `This month`
- Then `Apr 2026`, `Mar 2026`, … by month, indefinitely backward.

Heading style: **`--font-serif` italic, weight 500, `text-xs`, `--text-tertiary`**, `8px 16px` padding, `16px` top-margin (except first group). Serif italic — identical to Journal (§4.3) because references are editorial content too, and different from Chat (which uses sans uppercase for conversation rows). This keeps References visually paired with Journal while distinct from Chat.

Sort order within a group: `createdAt DESC` (most recent first), matching current behavior.

### 4.4 Empty state

Centered in the reference list area, no card frame. **Teach the mental model.** This is the single most important piece of copy on the surface:

- `Bookmark` icon at 32px, `--text-muted`.
- `text-sm` `--text-tertiary`: `"Nothing saved yet."`
- `text-xs` `--text-muted` (max-width 240px, centered): `"Reference any message in Chat using the bookmark icon — it'll collect here so you can come back to it."`
- 16px vertical gap between items.

For filter-empty states (search returns no results, filter chip narrows to zero):

- `text-sm` `--text-tertiary` centered: `"No references match."` No illustration.

---

## 5. Reference reader (main pane)

This is where the user actually reads. The reader is the point.

### 5.1 Container

- Centered reading column at `max-width: 760px`.
- `--bg` background (no panel tone, no border, no rounded card). The reader is prose on the app canvas, same as Chat and Journal.

### 5.2 Reference header

The "moment the reference earns its editorial feel." Three stacked elements at the top of the column, before the message body:

1. **Origin title.** `--font-serif`, `text-2xl` (24px — a step below Journal's `text-3xl` because a reference is a single fragment, not the day's journal), weight 600, tracking `-0.015em`, `--text-primary`. Format:
   - If the reference has a user-set title: that title.
   - Else: the conversation title.
   - Else: `Untitled reference`.
   Bottom margin: `4px`.
2. **Origin context + timestamp.** `--font-sans`, `text-sm`, `--text-tertiary`. Format: `From {conversationTitle} · {friendlyDate}`. Interpunct separator. `friendlyDate` uses `EEEE, MMMM d` ("Monday, April 17") for dates within the last year, else `MMM d, yyyy`. The conversation title, if the reference itself has a title, is clickable link text — `--accent` color, underlined on hover — and routes to `/chat?conversationId=…&messageId=…`. (If the user's title *is* the conversation title, there's no clickable link — the title already is the conversation.) Bottom margin: `8px`.
3. **Metadata row.** `--font-sans`, `text-xs`, `--text-muted`. Three items separated by interpuncts: message role (e.g. `Assistant` / `You`), capture status (`Captured to {noteTitle}` if captured; omitted if pending), and the referencing space if relevant (`Space: {spaceName}`). If a captured-note link is present, its `{noteTitle}` is clickable — `--accent` color, routes to `/journals?noteId=…&snapshotId=…[&journalSpaceId=…]`. Bottom margin: `32px` before the message body begins.

The origin title is large and serif because it is the title of the reference. Subordinate metadata stays small and sans because it is chrome. Same hierarchy pattern as Journal (JOURNAL-REDESIGN §5.2).

### 5.3 Referenced message body

This is the body of the bookmarked message as-rendered. **Reuse Chat's `Message` pattern exactly** — serif prose for assistant messages, sans for user messages, so references read as they did in Chat.

- **Assistant-role references** render in `--font-serif`, `text-base` (16px), `line-height: 1.65`, `--text-primary`. Identical to `components/Chat/Message.tsx:249`.
- **User-role references** render in `--font-sans`, `text-base` (16px), `line-height: 1.5`, `--text-primary`. Identical to Chat's user message.
- **System-role references** are shown in `--font-sans`, `text-sm`, `--text-secondary`, slight left-indent — they are chrome from the model, not editorial content. (Rare in practice; the current role filter offers `system`; we preserve rendering but de-emphasize.)
- Markdown pipeline: `normalizeAssistantMarkdown` for assistant; raw for user and system. Renders via existing `TiptapViewer` — same pipeline as Chat, zero data transform changes.

**Do not reuse Chat's `Message` component itself.** `Message.tsx` couples to `useConversationsStore` for live bookmark / delete actions on the *active* conversation, which isn't applicable here (References is cross-conversation, read-only from the message's perspective). Same reasoning as Journal's `InsightMessage` (JOURNAL-REDESIGN §6.1 / `InsightMessage.tsx` comment block). Build a lean read-only `ReferenceBody.tsx` that applies the same classes.

**Resolving payload:** the preview displayed in the body uses the full message content from `resolveBookmarkPayload`, cached per-reference. Loading state: before payload resolves, render `bookmark.messagePreview` (the sidebar preview string) in place so the reader has something to read. Swap to full content when it arrives, with no animation. If resolution fails: keep the preview and render a `text-xs` `--text-muted` line below: `"Full message content couldn't load — showing preview."`

### 5.4 Citations / sources (if present)

If the referenced message is an assistant message with associated sources (`message.sources` or `lastMessageSources`), render the same `SourceCitations` component used by Chat (`components/Chat/SourceCitations.tsx`), **reused unchanged**. Listed below the body, separated by `24px` margin.

Rationale: a reference lifted from Chat should preserve its source attribution. The user bookmarked an assistant claim — the receipts matter. Same cross-surface reuse argument as Journal's `InsightMessage` using `SourceCitations`.

Payload caveat: the current `resolveBookmarkPayload` returns `content` + `sourceReferences` (a lean shape, used for capture). It does **not** return the full `SourceWithMetadata[]` shape `SourceCitations` expects. To make this work we have two options:
- **Option A (in scope):** extend `resolveBookmarkPayload` to also return full source metadata from the resolved message (the original `getConversationMessages` response already includes it). Low risk.
- **Option B (out of scope):** add a new payload-loader API. Defer.

**Recommendation: Option A.** `resolveBookmarkPayload` becomes slightly larger but stays in `utils/chatBookmarks.ts`. Spec assumes this extension.

### 5.5 Annotation strip

Below the body + citations, separated by `32px` margin, hairline top border in `--border-subtle`, full reading-column width:

Two rows, each quiet:

1. **Title row.** Label `Title` in `text-xxs` uppercase tracking +loose `--text-muted` (left). Inline-edit affordance: click the label or the existing title text to edit; renders as `text-base` `--font-sans` `--text-primary` when not editing; `input` treatment (shadcn-style: `--surface` bg, `--border-default` 1px, `--radius-sm`, `--ring` focus) when editing. If the reference has no title, shows `Add a title…` in `--text-muted` italic.
2. **Note row.** Label `Note` in the same micro-style (left). Inline-edit affordance as above; renders as a `text-sm` `--font-sans` `--text-secondary` paragraph when not editing; `textarea` treatment (same chrome as the title input, multi-line) when editing. If no note: `Add context or a takeaway…` in `--text-muted` italic.

Save is **implicit on blur** — when the user commits the edit (blur or `Enter` for title, blur or `⌘Enter` for note), the draft flushes to `bookmarkConversationMessage`. No Save button. (The current always-visible Save button is the same anxiety pattern that was removed from Journal per JOURNAL-REDESIGN §5.6 — "nothing about a desktop note app justifies a save button in 2026.") Pending/saved state shows as a single quiet word to the right of the row being edited: `Saving…` / `Saved` / `Save failed — retry?`. Same pattern as Journal's `EntryAutosaveIndicator`.

### 5.6 Action rail (bottom of reader)

A quiet bottom rail, `24px` below the annotation strip, hairline top border in `--border-subtle`, full reading-column width. Mirrors Journal's `EntryActionRail` (JOURNAL-REDESIGN §5.5):

- Left side: three ghost-variant buttons. Each a text-plus-icon pair, `text-xs`, `--text-muted` default, `--text-secondary` on hover. No borders, no backgrounds.
  - `Open in Chat` (`ArrowUpRight` icon). Routes to `/chat?conversationId=…&messageId=…`.
  - `Copy` (`Copy` icon). Copies full content to clipboard; icon swaps to `Check` in `--accent` for 1.3s on success (current behavior preserved).
  - `Capture` (`Sparkles` icon). Runs the existing capture flow. **If already captured**, the button label is `Re-capture` and a secondary ghost button `Open captured note` (with `NotebookPen` icon) appears inline to its right, routing to `/journals?noteId=…&snapshotId=…`.
- Right side: one destructive ghost button.
  - `Delete` (`Trash2` icon). `--text-muted` default, `--danger-fg` on hover. No confirmation dialog (existing code does no confirm; a toast on success gives undo context).

Destination awareness: the current "Capture Destination" card (lines 673–698) — which previews "Journal notebook: {space}" or "Daily Research Inbox" — is **deleted from the main pane**. Instead, the `Capture` button's hover-title (native tooltip `title=`) shows the destination: `Capture to "{noteTitle}"` if a preferred note exists, else `Capture to today's Research Inbox`. This is a classic "quiet information" move — the data is there when the user reaches for it, but it doesn't take visual budget when they aren't asking.

If the user absolutely needs to see destination at rest (flag in review), fallback: a single `text-xs` `--text-muted` line below the action rail: `Captures to "{noteTitle}"`. No card, no border.

The "Open Journal Notebook" affordance and the "Open once to initialize notebook mapping" warning (currently lines 683–695) are **removed**. The warning was a leaked implementation detail (localStorage journal-note mapping); the core capture flow already falls back to a daily Research Inbox, which is acceptable behavior when mapping is missing. If the user wants to open the journal, they use the nav.

### 5.7 Empty state

If no reference is selected (e.g., after delete that empties the list, or first-paint with no references in the user's vault):

- Centered in the main pane (sidebar still renders):
  - `Bookmark` icon at 40px, `--text-muted`.
  - Heading `text-xl` `--font-serif` weight 600 `--text-primary`: `"Your reference shelf."`
  - Body `text-sm` `--text-tertiary`, max-width 400px, centered: `"Saved messages live here — the fragments you wanted to keep from a conversation. Open one from the sidebar to read it, or save a new one from Chat by clicking the bookmark icon on any message."`
  - No button — the user can't "create" a reference from this surface; they create one from Chat. The copy teaches them where to go.

Single reference in list, selected: render the reader normally.

---

## 6. Filtering and search

Scope: use the existing `listMessageBookmarks` API (`conversationId?`, `query?`, `limit`, `offset`). No new filter backends.

### 6.1 Sidebar search

- Single input (§4.1.3), `text-sm`, 32px height.
- Placeholder `"Search references..."`.
- Debounced 250ms. Wires to the existing `query` field on the API.
- Client-side filter for role/origin is layered on top of the debounced API query (same approach as current code, which does `listMessageBookmarks` with `query` plus in-memory filter by role).

### 6.2 Filter chips

Three, per §4.1.4: `All` · `Recent` · `Captured`.

- `All` — no additional filter.
- `Recent` — navigational; jumps the list to the `This week` group. No API change.
- `Captured` — in-memory filter on the `capturedIndex`, same as current code (`statusFilter === 'captured'` branch).

**Removed from chips:** role filter (`All / AI / You / System`). Role is demoted out of the primary filter rail. Rationale: the user thinks about the reference by subject and by origin, not by the arbitrary fact that an answer came from an assistant. If role filtering proves essential, it reappears as a Select in the sidebar below the search (like the origin picker at §4.1.2). Default: removed.

**Removed from chips:** `Pending` status filter. Captured-vs-pending is still visible as an individual indicator on each list item (the captured dot) and on the reference detail, but it is not a *primary* navigation axis. The inbox-as-queue paradigm doesn't apply (§2).

### 6.3 Origin picker (§4.1.2)

Not a chip — a Select. Three options: `All references` / `From Chat` / `From Journal entries`. Client-side filter joining `bookmark.spaceId` against the `journalsById` map: if the bookmark's space is in the journal map, it's "From Journal entries"; otherwise "From Chat." Uses the existing data already loaded via `listJournals`.

### 6.4 Filtered-empty state

Per §4.4.

---

## 7. Date navigation

**Deferred.** References don't benefit from calendar navigation the way Journal does — a reference's primary identity is its *content*, not its *date*. Group headings (§4.3) give enough chronological orientation.

If users report needing date jumps, we add a `JournalCalendarPopover`-equivalent in a follow-up. For now: the filter chips give `All / Recent / Captured`; grouping gives `Today / Yesterday / This week / This month / {month yyyy}`; that's sufficient.

---

## 8. States

| State | Render |
|---|---|
| **No references exist** (first run) | Main pane §5.7 "Your reference shelf." Sidebar §4.4 empty state. |
| **Filter returns zero** | Sidebar shows `"No references match."` §4.4. Reader shows §5.7 (no selection possible). |
| **Reference selected** | §5.2–5.6. |
| **Reference payload resolving** | Body renders `messagePreview` in-place (§5.3). No spinner, no skeleton, no cascade. |
| **Reference payload failed** | Body keeps preview + `text-xs` `--text-muted` note below body: `"Full message content couldn't load — showing preview."` |
| **Captured reference, captured note still exists** | Header metadata row (§5.2.3) shows `Captured to "{noteTitle}"` as a clickable link. Action rail (§5.6) shows `Re-capture` + `Open captured note`. |
| **Captured reference, captured note deleted** (broken origin — note was deleted in Journal without unlinking) | Header metadata row reads `Previously captured · note removed`. Action rail shows `Capture` (fresh capture will create a new note/snapshot). No broken link. |
| **Reference with broken origin** (source conversation deleted) | Header "Origin context" reads `From a deleted conversation · {friendlyDate}` at `--text-muted`. `Open in Chat` button is `--text-disabled`, not clickable. Tooltip: `"Source conversation no longer exists."` The reference itself is still readable because the bookmark DTO carries `messagePreview` and the cached payload, if resolved, is still valid. |
| **Long reference body** | Reading column scrolls naturally inside the main pane. No scroll cap. Trailing citations and annotation strip sit after the body in the prose flow (not sticky). |
| **Reference with rich content** (lists, code, tables, blockquotes) | Rendered by `TiptapViewer` + existing `tiptap.css`. No change from Chat's rendering. Code blocks, lists, tables inherit Chat's prose treatment (CHAT-REDESIGN §3.3). |
| **Saving annotation — in progress** | `Saving…` at `text-xs` `--text-muted` right of the edited row. |
| **Saving annotation — failed** | `Save failed — retry?` at `text-xs` `--danger-fg` right of the edited row; click retries. Plus `toast.error` first occurrence. |
| **Capture — in progress** | `Capture` button shows `Sparkles` with `animate-spin` replaced by a subtle static `Loader2` in `--text-muted`. Button is disabled; label stays `Capture`. (No full-width progress bar.) |
| **Capture — success** | `toast.success("Captured to '{noteTitle}'.")` with action button `"Open"` that routes to the captured note. No inline status change; the list row gains its captured dot. |
| **Capture — failed** | `toast.error("Capture failed", { message })`. No inline status change. |
| **Delete — success** | Row removed optimistically from list. Next reference selected automatically. `toast.success("Reference removed")`. |
| **Very many references (>200)** | Same approach as Chat/Journal: do not virtualize in this pass. Flag if performance suffers (risk §11). |

---

## 9. Motion

Same vocabulary as Chat (CHAT-REDESIGN §8) and Journal (JOURNAL-REDESIGN §10). Explicitly:

**Animates:**

- Sidebar active-bar slide: `framer-motion` `layoutId="references-sidebar-active-bar"`, transition `{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }`. Identical to Chat's `sidebar-active-bar`. Respects `useReducedMotion`.
- Hover-revealed sidebar action icons: `opacity` `--duration-fast`.
- Citations expand/collapse: inherited from `SourceCitations` (already tokenized).
- Popover open (origin picker, if it grows to a `DropdownMenu`): Radix default fade + `scale(0.98 → 1)`, `--duration-base` `--ease-out`.

**Does NOT animate:**

- Reference switch. Body content just changes. No fade, no slide. (Same defense as Journal §10: fade adds latency, switching is deliberate.)
- Page mount. No staggered entrance.
- Payload swap (preview → full body): instant.
- Capture / delete state transitions: instant.
- Annotation save state: instant.
- Row hover: instant `--surface` background.
- Copy-success checkmark swap: instant icon change (no spin, no pulse). The 1.3s reset is a timer, not a transition.

---

## 10. Component ownership

### Decomposition strategy: **decompose while redesigning**

Same reasoning as Journal (JOURNAL-REDESIGN §11). We are not refactoring a 791-line single component in place; we are replacing it with a shell + leaves, each born in the new design language.

### Files to create (in `components/ReferenceInbox/`)

| File | Responsibility | Approx LOC |
|---|---|---|
| `ReferenceInbox.tsx` | Shell. URL params (selected reference id via search param), data loading via a new `useReferenceInbox` hook, renders `<ReferenceList>` + `<ReferenceReader>`. No direct JSX beyond the two-pane layout. | 120–150 |
| `ReferenceList.tsx` | Left sidebar: top rail, origin picker, search, filter chips, grouped list, footer. Mirrors `Journal/EntryList.tsx` structure. | 220–260 |
| `ReferenceListItem.tsx` | Single reference row with origin+date prefix, title, preview line, time, captured dot, hover actions, active state with `layoutId` bar. Mirrors `Journal/EntryListItem.tsx`. | 160 |
| `ReferenceReader.tsx` | Main pane container — header orchestration, body wrapper, citations, annotation strip, action rail. | 180–220 |
| `ReferenceHeader.tsx` | Origin title (serif, 24px) + origin-context + metadata row. Pure presentation. | 90 |
| `ReferenceBody.tsx` | Read-only prose viewer. Role-aware serif vs sans. Reuses `TiptapViewer`. Handles payload-resolution fallback to preview string. | 90 |
| `ReferenceAnnotationStrip.tsx` | Inline-edit Title + Note, blur-to-save via prop callbacks. Includes quiet save-state indicator (reuse `Journal/EntryAutosaveIndicator` if its prop shape generalizes — if not, duplicate as `ReferenceSaveIndicator`). | 140 |
| `ReferenceActionRail.tsx` | Bottom rail: Open in Chat / Copy / Capture / Open captured note / Delete. Ghost buttons. | 110 |
| `ReferenceOriginPicker.tsx` | The sidebar origin Select. shadcn `Select` primitive. | 70 |
| `useReferenceInbox.ts` | Hook that owns the data-loading lifecycle: `listMessageBookmarks` + `listWorkspaceNotes` + `listConversationSpaces` + `listJournals`, payload cache, capture/remove action methods, selection state, filter/search derivations. Pulled out of the workspace so the shell is thin and the testable surface is narrow. Directly mirrors `Journal/useJournalEntries.ts` in shape. | 220 |
| `index.ts` | Re-exports `ReferenceInbox`. | 1 |

### Files to modify (outside ReferenceInbox/)

| File | Change |
|---|---|
| `utils/chatBookmarks.ts` | Extend `resolveBookmarkPayload` to also return full `SourceWithMetadata[]`, threaded through from the underlying `getConversationMessages` response. Backward-compatible: callers that don't need sources (e.g. current capture flow) just don't read the new field. |
| `components/Chat/SourceCitations.tsx` | **KEEP.** Imported into `ReferenceReader.tsx` unchanged. |
| `types/` | No changes. `ConversationMessageBookmarkDto` and related shapes are already sufficient. |

### Files to delete

- Nothing outside `components/ReferenceInbox/`. The old `ReferenceInbox.tsx` is replaced by the new files above.

### shadcn primitives to adopt

All present in codebase (verified via Chat and Journal specs):

- `Select` — origin picker.
- `Popover` — if role becomes a filter Select per §6.2 fallback, or if capture-destination hint becomes a richer popover. Default: not used.
- `Button` — every button (ghost variant for action rail).
- `ScrollArea` — sidebar list.
- `Tooltip` — kbd hints, icon-only action hover, Capture destination hover-title.

No new primitive additions required.

### Cross-surface reuse

- `components/Chat/SourceCitations.tsx` — imported into `ReferenceReader.tsx` when the referenced message has sources.
- `components/Journal/EntryAutosaveIndicator.tsx` — if its prop shape generalizes (takes three states: idle `Saved` / pending `Saving…` / error `Save failed — retry?`), reuse directly. If it's tied to a specific hook, create a local `ReferenceSaveIndicator` that mirrors it. Flag during implementation.
- `components/TiptapEditor/TiptapViewer` — unchanged, inherits serif from `ReferenceBody.tsx` wrapper.

---

## 11. Scope boundaries

This spec is **NOT**:

- Changing the `listMessageBookmarks` / `bookmarkConversationMessage` / `unbookmarkConversationMessage` APIs or response shapes.
- Changing `captureChatReferenceToWorkspaceNote`, its snapshot-id convention, its marker syntax, or any capture output. The capture pipeline is preserved exactly.
- Changing the `WorkspaceNote` / `ConversationSnapshot` / `SnapshotMessage` data model.
- Changing `VaultAPI` call sites outside the single `resolveBookmarkPayload` extension.
- Changing the Chat bookmark gesture. The user still saves references by clicking the bookmark icon in Chat; only the surface that *shows* them changes.
- Changing the `/references` route. It stays.
- Adding tags on references. No tag concept exists in the data model. Flag as follow-up.
- Adding export of references. No current export flow; flag as follow-up.
- Adding multi-select / bulk actions. The current "Capture Pending" batch is removed (§10 open q), no replacement built. If bulk operations prove necessary, they return in a follow-up with a proper selection model — not glued onto a single-select reader.
- Adding full-text search across message bodies (the current `query` searches title/note/conversation; Phase 5 may add body search backend-side).
- Implementing a "trash" or soft-delete for references. Delete is immediate and matches current behavior.
- Touching the Chat sidebar's References filter chip (Chat `ConversationSidebar.tsx:922`). That renders bookmarks scoped to the current Chat space inside the Chat sidebar — a different surface with a different purpose.

---

## 12. Risks

1. **The 791-line file.** Strategy is decompose-while-redesigning. Smaller than Journal's 3,062 but the soup is equally dense. Mitigation: each PR ships fully-redesigned leaves; no half-state merges. Manual smoke test before each PR: load references, switch selection, annotate + blur, capture, re-capture, open captured note, open origin, delete, filter, search, switch origin, collapse sidebar.
2. **`resolveBookmarkPayload` extension.** Adding full sources to the payload shape is mechanical but touches a utility used by both References and Chat's bookmark-UI flows. Verify both call sites still work. Unit test coverage exists for the capture path (`utils/__tests__/chatReferenceCapture.test.ts`, `chatReferenceIndex.test.ts`) — extend as needed.
3. **SourceCitations cross-surface reuse.** Same risk as Journal's `SourceCitations` reuse. Verify the component does not hard-depend on Chat context (it takes `sources` + `onViewSource` as props; appears clean — verified by reading the file). If a shared context dependency is discovered, fallback is to extract a leaner `SourceCitationsView` wrapper.
4. **Captured-note broken link.** If a user captures a reference, then deletes the captured note in Journal, the `capturedIndex` on the next inbox load will not show the reference as captured (because the snapshot no longer exists on any note). This matches current behavior. But if the user does a mid-session navigation after capturing, the link in the metadata row might route to a deleted note and show an empty Journal view. Mitigation: the state §8 "Captured reference, captured note deleted" handles the re-load case; the in-session case is bounded by a quick toast + re-load on focus. Flag if users complain.
5. **Route parameter for selected reference.** Currently the component tracks selection only in React state. If we want deep-linking (`/references?referenceId=…`), that's a small addition to the new `useReferenceInbox` hook. Spec'd as in-scope; cost is low. Cross-reference Journal's `noteId` / `entryId` URL params.
6. **Bulk capture removal.** "Capture Pending" is a real, present feature. Removing it with no replacement is a product call (§14 open question 2). If that call is "keep it," it reappears as a sidebar footer affordance or a per-selection multi-select — both require small spec additions.
7. **Performance with large reference sets.** No virtualization (§8). Unlikely to bite: a power user probably has tens to low-hundreds of references, not thousands. Flag if it does.

---

## 13. Tokens and patterns needed beyond TOKENS-SPEC

None required. TOKENS-SPEC is sufficient for every decision in this spec. Patterns inherited from sibling specs:

- The `--accent` 2px left-bar active state (Chat + Journal) — used for the reference row.
- The ghost-button action rail treatment (Journal) — used for §5.6.
- The inline-edit annotation pattern (Journal's note title uses a similar blur-to-save, but for the page title, not arbitrary annotations — minor variation, same primitives).
- The serif-prose editorial body (Chat + Journal) — used for §5.3.

---

## 14. Open questions requiring Josh's input

These are the decisions the spec made but would benefit from a "yes, that" or "no, keep the old."

1. **Capture Pending (bulk capture) — delete, keep, or redesign?**
   The current "Capture Pending" button bulk-captures up to 24 non-captured bookmarks in the current filter. It reinforces the email-inbox paradigm the product soul §2 argues against. My recommendation: **delete**. If a user wants to capture many references into a journal, they do it one-by-one from the reader, where the act is deliberate. If deletion is wrong, the fallback is a sidebar-footer "Capture all pending" affordance (text link, not prominent) that respects the current filter. Default if no answer: delete.

2. **Role filter (`All / AI / You / System`) — remove entirely?**
   Spec removes this from the primary chip rail. My argument: users think about a reference by its *subject* and *origin*, not by whether the sentence was uttered by an AI or a human. If removal is wrong, fallback: move role to a Select below the origin picker in the sidebar (same treatment, less prominent). Default: remove. Watch telemetry if we have it; re-add on demand.

3. **Pending status chip — include as a filter chip, or only as the captured-dot indicator on rows?**
   Spec keeps `Captured` as a filter chip (one chip, positive framing) and removes `Pending` (its complement is `All` minus `Captured`). My argument: you filter *for* something, not *against* it. Default: keep only `Captured`. If users report "I want a view of everything I still need to process" (which violates §2 soul but is a real gesture some users want), re-add `Pending` with parity with `Captured`.

4. **Capture Destination preview — keep at rest, or hide behind hover-tooltip?**
   Spec hides behind the `Capture` button's native `title=` tooltip. My argument: destination is implementation detail the user doesn't need until the moment of capture. If that's wrong, fallback: a single `text-xs` `--text-muted` line below the action rail: `Captures to "{noteTitle}"`. Default: hide behind hover.

5. **Deep-linking via URL param?**
   Spec notes this as in-scope (§12 risk 5) because it's cheap and consistent with Journal's `noteId`/`entryId`/`snapshotId` params. Confirm whether to wire `?referenceId=…` into `useReferenceInbox`. Default: yes.

6. **Origin picker semantics — `From Chat` / `From Journal entries`, or simpler?**
   Spec splits origin by whether the bookmark's space is a journal (a journal entry is a conversation in a journal space; currently scannable via `journalsById.has(bookmark.spaceId)`). Alternative: `All references` / `By space…` with a deeper Select. Default: keep the simpler two-way split; if spaces proliferate and users have multiple journals, revisit.

7. **`Untitled reference` fallback copy** — acceptable, or do you want `"No title"` / `"—"` / something else?
   Spec'd as `Untitled reference`. Trivial to change.

---

## 15. Verification checklist for Josh's final review

- [ ] Every list region uses `--surface` backgrounds (no `--overlay`, no `bg-white/XX`).
- [ ] Every color reference resolves to a token; no raw Tailwind palette classes (`emerald-*`, `cyan-*`, `amber-*`, `rose-*`, `white/XX`).
- [ ] No `backdrop-blur` anywhere.
- [ ] No radial-gradient, linear-gradient, or mesh backgrounds.
- [ ] Only one accent (violet) on the surface — captured indicators, active-row bar, link color, focus rings.
- [ ] Serif is scoped to the referenced message body (§5.3) and the origin title (§5.2.1). Everything else is sans.
- [ ] Active sidebar row uses the 2px `--accent` left-bar pattern (`layoutId="references-sidebar-active-bar"`), not a tinted fill plus ring shadow.
- [ ] No stat ribbons, no bento cards, no double-treatment of weight+color on chrome labels.
- [ ] Cross-surface siblings (Chat, Journal, References) use identical sidebar proportions (280px fixed, `⌘\` collapse), identical active-bar motion, identical grouping conventions (with the Journal/References difference on heading style — serif italic — explicit in §4.3).
