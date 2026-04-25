# Command Palette — Upgrade Plan

**Status:** plan, not spec. ~900 words. Phase 5.x.
**Sibling documents:** `CHAT-REDESIGN-SPEC.md`, `FILEBROWSER-REDESIGN-SPEC.md`, `PRODUCT-THESIS.md`.

---

## 1. Current state — four parallel surfaces, two keyboard collisions

The palette/search area is a mess. There are four live surfaces doing overlapping work:

- **Global `CommandPalette`** (`components/CommandPalette/CommandPalette.tsx`, 615 LOC) — bound to ⌘K globally via `hooks/useCommandPalette.ts:70`. Runs live hybrid `searchDocuments` as you type (min 2 chars), plus Recent Searches / Recent Documents / Search / Upload / Navigate / Settings / Help groups. Uses `cmdk`. Styling is hand-rolled CSS in `styles/command-palette.css` (314 LOC) — not yet on the shadcn/token system.
- **`/search` route → `SearchInterface`** (`components/SearchInterface/SearchInterface.tsx`, 225 LOC) — full-page search via `useSearchQuery` (TanStack), semantic/keyword/hybrid mode buttons, virtualized results, in-page `ContentViewer`. Wired in `routes.tsx:74`.
- **`SearchView`** (`components/SearchView/SearchView.tsx`, 339 LOC) — a *second* full-page search that does the same thing with `searchHybrid`, `QueryRewritePanel`, `DocumentViewer`. **Exported from `components/index.ts:31` but not routed anywhere I can find.** Dead or zombie code.
- **`ConversationSpotlight`** (`components/Chat/ConversationSpotlight.tsx`, 335 LOC) — Chat-only palette over conversations + message bookmarks. **Hijacks ⌘K locally** in `ChatView.tsx:31` — same-key collision with the global CommandPalette. `config/shortcuts.ts:19` separately claims `Mod+P` = `commandPalette` but nothing actually binds `Mod+P`.

Three different "searches" (hybrid, hybrid-via-TanStack, hybrid-via-searchHybrid). Two components bound to ⌘K. A dead `SearchView`. A `/search` route that duplicates what the palette already does live. The corpus itself (documents, folders, journal entries, references) is addressable only through the global palette's one collapsed `Search Results` group — there's no grouping, no type awareness, no filter scope.

## 2. The core design move — one palette, grouped corpus, sibling register

**One palette. One keybind. Grouped corpus.** Unify the global CommandPalette and ConversationSpotlight into a single spotlight (name it `Spotlight`) bound to ⌘K globally, available from any route. Results are grouped by corpus type, not by action shelf. When empty/short query → render action shelves (Navigate, Capture, Upload, Settings, Help). When querying → render grouped results.

Groups, in order: **Documents · Conversations · Journal entries · References (bookmarks) · Folders · Actions · Navigate**. Only the non-empty groups render. `cmdk` does this naturally via `Command.Group`. Enter activates; arrow-keys navigate; `⌘1…5` jumps focus to the Nth group; `Esc` closes. The palette becomes the single answer to "I know the thing exists, take me to it."

Visual register: opens at `top: 120px`, `max-width: 640px`, matches the `ConversationSpotlight` dialog styling (radix `Dialog` + tokens: `bg-surface-raised`, `border-subtle`, `shadow-md`, `text-xxs` kbd chips, 2px `--accent` selection bar with `layoutId="spotlight-selection"`). **Retire the hand-rolled `styles/command-palette.css`** — tokens only. Motion: reduced-motion-aware, 180ms ease-out fade+zoom on open, no bounce, per `AESTHETIC-GUIDE`.

## 3. Corpus awareness — the new thing this phase adds

This is the Gemini-identified gap. Today the palette knows about documents (via `searchDocuments`) and nothing else. The upgrade teaches the palette the four primitives from `PRODUCT-THESIS.md`:

1. **Documents** — hybrid `searchDocuments` (existing). Min 2 chars. 8 results max. Show filename, file-type chip, 2-line excerpt, distance score. Enter opens via `openFileById`.
2. **Conversations + Message bookmarks** — `listConversationsExplorer` + `listMessageBookmarks` (existing, used by `ConversationSpotlight`). 6 + 6 max. Enter selects conversation / scrolls to bookmarked message — lift the `scrollToMessage` helper out of ConversationSpotlight.
3. **Journal entries** — backend gap. No `searchJournals` / `listJournalEntries` today; `listJournals` returns conversation-linked journals only. Filename + title fuzzy match on the client as a start; flag a backend task for true journal-body search.
4. **References** — covered by `listMessageBookmarks` above (they *are* the bookmark list).
5. **Folders** — `listAllDocuments` already loads folder paths client-side; derive distinct folders, fuzzy-match on path. No backend work.

No-query default view (empty input): Recent documents · Recent searches · Navigate shelf · Actions shelf. Keep `useCommandPalette`'s localStorage recency (`vault-command-palette`).

Ranking: client-side fuzzy (cmdk's `shouldFilter` default) for folders / actions / navigation. Server-side hybrid for documents. No attempt at unified cross-group ranking — groups stay visually separated, each internally ranked. Semantic-vs-keyword-vs-hybrid is a user's *search* concern (stays in the full-page `/search` if kept); the palette always uses hybrid.

## 4. Consolidation — what merges, what dies

- **Delete `components/SearchView/`** (339 LOC, unrouted, duplicate). Remove export from `components/index.ts:31`.
- **Delete `components/Chat/ConversationSpotlight.tsx`** (335 LOC). Fold conversations + bookmarks into the unified `Spotlight`. Remove `ChatView.tsx:29-40` ⌘K handler. Lift `scrollToMessage` into `utils/scrollToMessage.ts`.
- **Delete `hooks/useCommandPalette.ts`** or rename → `useSpotlight`. The global palette's recent-items state lives here.
- **Keep `/search` route → `SearchInterface`** as the escape-hatch surface for deep structured search (mode buttons, virtualized results, content viewer). Per `PRODUCT-THESIS §6`, SearchInterface stays as the "boolean-and-filename surface." The palette handles "take me to a thing"; `/search` handles "let me sit in results." **Open question in §7.**
- **Absorb the 615-LOC `CommandPalette.tsx`** into a leaner `Spotlight/` module (target ~250 LOC split across `Spotlight.tsx`, `SpotlightGroups/` per group, `useSpotlightQuery.ts` for the batched fan-out).
- **`styles/command-palette.css` deleted.** Token classes only.

## 5. Implementation sketch

```
components/Spotlight/
  Spotlight.tsx               (root: Dialog + cmdk + query state)
  groups/
    DocumentsGroup.tsx        (hybrid searchDocuments)
    ConversationsGroup.tsx    (listConversationsExplorer)
    BookmarksGroup.tsx        (listMessageBookmarks)
    JournalsGroup.tsx         (client filter over listJournals)
    FoldersGroup.tsx          (derived from listAllDocuments)
    ActionsGroup.tsx          (navigate/upload/settings/help)
  useSpotlightQuery.ts        (debounced 180ms, fan-out Promise.all, abort)
  index.ts
hooks/useSpotlight.ts         (open/close/toggle, recents)
utils/scrollToMessage.ts      (lifted from ConversationSpotlight)
```

`RootLayout.tsx` replaces `<CommandPalette />` with `<Spotlight />`. One global ⌘K. ChatView loses its local ⌘K handler. `cmdk`'s `shouldFilter={false}` when query ≥ 2 (we own ranking via API calls); `shouldFilter={true}` when query < 2 (client filter over action shelves).

## 6. Motion + visual register

Sibling to `ConversationSpotlight` today (which stays the canonical styling — we're extending it, not replacing). Fade+zoom 180ms, `duration-slow` in, `duration-base` out. Selection bar: 2px `--accent`, `top-2 bottom-2` (not full height — see `CHAT-POLISH-COMPONENTS.md:145`). Empty state: `"No results for '<query>'"` (`text-sm --text-tertiary`) + hint (`text-xs --text-muted`). Kbd chips: `text-xxs font-mono`. Footer: `↑↓ Navigate · ↵ Select · Esc Close` — same chrome as current palette, tokenized.

## 7. Questions for Josh

1. **Kill the `/search` route?** The palette covers ~90% of "find a document" traffic once it's corpus-aware. Keeping `/search` gives semantic/keyword/hybrid mode toggling + virtualized browsing. Drop it, fold mode-toggle into palette, or keep as power-user surface? My lean: **keep** (per `PRODUCT-THESIS §6` — escape hatch for structured search). Low cost.
2. **Include Journal entries** in the palette before backend full-body search exists? Client-side title-match is weak. Ship without Journal group and add when backend lands, or ship with a filename-only fallback? My lean: **ship with fallback**, mark TODO.
3. **Should the Spotlight replace the in-Chat spotlight entirely, or live alongside it?** My strong lean: **replace.** One ⌘K for everything. Conversations + bookmarks are just groups.
4. **Keybind for References only?** `Mod+P` is claimed but unbound in `config/shortcuts.ts:19`. Reuse for "Spotlight scoped to Conversations+References" (Chat power-user gesture) or drop it entirely? My lean: **drop** — one palette, scope via typing.
5. **`SearchBar` component** (`components/SearchBar/SearchBar.tsx`) — used only inside the zombie `SearchView`. Delete with `SearchView`? Confirm nothing else imports it.

---

**Biggest backend gap:** no journal-body search endpoint. `listJournals` returns conversation-linked journals only; journal entries as first-class corpus members have no search API. Blocks true Journal group in the palette. Client-side title filter is a temporary compromise.
