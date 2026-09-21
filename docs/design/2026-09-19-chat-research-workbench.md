# Chat as a research workbench

2026-09-19. Branch `ui/craft-pass`, uncommitted. Four parallel tracks plus an integration pass.

## 1. Why

The chat screen was a transcript. The things that make Lattice a research tool rather than a chat
app were either collapsed behind a chip or dropped before they reached the UI: per-sentence
grounding verdicts, the retrieval trace, 40+ timing fields, which tools ran, which model answered.

The direction, already started: **an answer is a checkable document, not a chat bubble. The claim,
the evidence for it, and the process that produced it are visible together.**

Slice 1 is built and is the pattern to extend, not replace:

| File | What it does |
| --- | --- |
| `src/components/TiptapEditor/extensions/claimMarks.ts` | Draws each verified sentence's verdict on the answer text (ProseMirror decoration, `data-claim`). Only doubt is drawn at rest. |
| `src/components/Chat/ClaimHoverCard.tsx` | Verdict, how it was reached (judge vs lexical), evidence quote, cited files. |
| `src/components/Chat/EvidenceMargin.tsx` | Cited passages as margin notes beside the answer when `.chat-panel` is ≥1100px wide and the thread is non-empty. Hover links notes ↔ chips ↔ sentences via `.is-lit`. |
| `src/index.css` (`.chat-panel`, `.chat-column`, `.chat-beside-margin`, `.evidence-*`, `.claim*`) | `.chat-panel` is a CSS container; the margin layout is a container query, so anything that narrows the panel (a docked reader) folds the margin away by itself. |

## 2. Rules that apply to every track

1. **One owner per file.** The ownership table in §3 is binding. Never edit a file you do not own,
   not even to fix a type error or a lint error in it. If another track's file blocks you, work on
   a later step, retry in a few minutes, and if it is still blocked say so in your final report.
2. **Shared working tree.** All four tracks edit the same checkout at the same time. Whole-project
   `tsc` and the dev server will sometimes show another track's half-finished state. Judge your
   work by errors in files you own. Never `git stash`, `git checkout`, `git restore`, `git reset`,
   `git add` or commit. Never revert a change you did not make.
3. **No legacy support.** Pre-release; no compatibility shims, no dual code paths, no deprecated
   aliases. Schema changes go into the single squashed migration
   `src-tauri/migrations/20260916000000_init_schema.sql`.
4. **Space isolation is a hard invariant.** Any path that hands documents or document names to a
   model or to the user as "what this chat can use" derives its allow-list from
   `ConversationRepository::retrieval_document_scope` / `space_document_scope`. It may narrow that
   scope. It may never widen it. It fails closed.
5. **Two design rules are encoded as tests. Do not edit those tests.** Message actions stay visible
   without hover (`MessageActions.test.tsx`). Search rows show raw `vec · bm25` scores
   (`search-flow.integration.test.tsx`).
6. **Match the codebase.** Tokens and primitives from the craft pass: `--chrome < --bg < --surface
   < --surface-raised`, `--surface-overlay` for floating things; border tokens already carry alpha
   (never `hsl(var(--border-x) / n)`); `.surface-pop` for Radix floating surfaces; `text-ui` (13px)
   is the chrome size; `.kbd`, `.pressable`, `.row-hover`. Comments say why, in the register of the
   surrounding code. Tests are named as sentences about behaviour.
7. **Tailwind purges class names it cannot read.** A class defined in `@layer components` and
   assembled at runtime (`` `claim-${verdict}` ``) is deleted from the build. Spell names out.
   `block` and `line-clamp-*` both set `display`; do not combine them.
8. **CSS.** Only Track B edits `src/index.css`. Every other track puts its CSS in a file beside its
   component (`import './turn-record.css'`, as `TiptapViewer` does with `tiptap.css`), plain CSS
   using the `hsl(var(--token))` variables, no `@layer`.
9. **Only Track A runs `cargo` and `npm run bindings:generate`.** `src/lib/bindings.ts`,
   `src/lib/api.ts` and `src/types/` belong to Track A.
10. **Seeing your work.** One vite dev server is already running on `127.0.0.1:5173`. Do not start
    another, and never kill anything listening on 5173. The screenshot rig is at
    `~/.claude/projects/-Users-josh-Code-lattice-temp/ui-rig/` (`shots.mjs`, `tauri-mock.js`,
    `typeof.mjs`, `contact-sheet.mjs`). Copy those four files into your own folder
    `<scratchpad>/rig-<track letter>/`, symlink `node_modules` beside them
    (`ln -s /Users/josh/Code/lattice-temp/node_modules .`), and run
    `node shots.mjs both <filter>` from there (`SHOTS_WIDTH=1728 SHOTS_HEIGHT=1000` for the wide
    layout). Add fixtures and shots for your feature to your copy; do not copy it back, and list
    what you added in your final report. Look at the images in both themes before calling anything
    done. Fixture gotchas: message `status` is `completed`; metadata is a JSON string.
11. **Done means:** `npx tsc --noEmit -p .` has no errors in your files; `npx eslint <your files>`
    is clean; your new tests and the existing tests for the files you touched pass
    (`npx vitest run <paths>`); rig shots of your feature are clean in both themes and you have
    looked at them. Track A also: `cargo test --lib` for the modules touched, `cargo clippy
    --all-targets -- -D warnings`, `npm run bindings:check`.
12. **Final report** (this is what the integrator reads): what you built, file by file; what you
    verified and how; what you could not do and why; the exact JSX and props needed to mount your
    components in files you do not own (§4); rig fixtures and shots you added.

## 3. Tracks and file ownership

| Track | Scope | Owns (may edit) |
| --- | --- | --- |
| **A — Turn record** | Step events, persisted per-turn provenance, the timeline UI; backend contracts the other tracks need | All of `src-tauri/`; `src/lib/bindings.ts`, `src/lib/api.ts`, `src/types/**`; `src/hooks/useConversationsController.ts`; `src/stores/conversationsStore*`; `src/components/Chat/RetrievalTrace.tsx`, `ActivityNote.tsx`; new `src/components/Chat/turn/**` |
| **B — Docked reader** | The source reader becomes a resizable third pane | `src/components/Chat/ChatView.tsx`, `Message.tsx`, `FilePreviewModal.tsx`, `CitationRail.tsx`, `viewers/**`; `src/index.css`; new `src/stores/chatReaderStore.ts`, `src/components/Chat/reader/**`; `__tests__/Message.test.tsx` |
| **C — Composer** | `@` documents, `/` commands, visible modes, attach button | `src/components/Chat/ChatPanel.tsx`, `ComposerControls.tsx`, `ChatDropStaging.tsx`; new `src/components/Chat/composer/**`; their tests |
| **D — Out of chat** | Export, compare/journal/flashcard from an answer or a claim, branch lineage | `src/components/Chat/MessageActions.tsx`, `EvidenceMargin.tsx`, `ClaimHoverCard.tsx`, `ConversationSidebar.tsx`, `sidebar/**`; new `src/components/Chat/actions/**`, `src/utils/conversationExport.ts`; their tests |

Nobody owns `Message.tsx` except B. A and D deliver components with the contracts in §4 and the
integrator mounts them.

### Cross-track dependencies

Track A does step **A0 first**, before anything else, because C and D compile against it:

- `ToolPreferences.focusDocumentIds` (C)
- `VaultAPI.listSpaceDocuments` (C)
- `Conversation.forkedFromConversationId` / `forkedFromMessageId` (D)

C and D do the steps that need these **last**. If the binding is still missing when you get
there, do not stub it: finish everything else, and report the step as blocked.

## 4. Contracts

### 4.1 A0 — backend contracts (Track A, first)

```ts
// src/types/conversation.ts — ToolPreferences
focusDocumentIds?: string[];   // documents this chat is pinned to; [] or absent = the whole space

// src/lib/api.ts
listSpaceDocuments(spaceId: string | null, query: string, limit: number): Promise<SpaceDocument[]>
interface SpaceDocument { documentId: string; fileName: string; category: string | null; modifiedAt: string | null }

// Conversation (and the explorer list row)
forkedFromConversationId: string | null;
forkedFromMessageId: string | null;
```

- `list_space_documents` answers from `space_document_scope(space_id)` (null/blank → General) and
  nothing else, filtered by a case-insensitive file-name match, newest first, `limit` clamped to
  50. It is what `@` offers, so it must be exactly what retrieval can reach.
- Fork columns: `forked_from_conversation_id TEXT`, `forked_from_message_id TEXT` on
  `conversations`, in the squashed migration, written by `ConversationRepository::fork`. No
  foreign key: a deleted parent leaves a dangling id and the UI shows no link. **Find out how the
  app copes with an edited squashed migration on an existing database** (sqlx checksums) by
  looking at how the last schema change was rolled out; follow that, and say in your report what a
  developer with an existing database has to do.
- Regenerate bindings, confirm `bindings:check`, then move on to A1.

### 4.2 A — the turn record

One structure is streamed while the turn runs and persisted when it ends, so what the reader
watched is what the record says.

```ts
type TurnStepKind =
  | 'route' | 'plan' | 'search_documents' | 'sufficiency' | 'corrective_search'
  | 'web_search' | 'read_page' | 'wiki' | 'open_document' | 'tool'
  | 'generate' | 'verify' | 'retry';

interface TurnStep {
  id: string;                 // stable within the turn; a finish event carries the id of its start
  kind: TurnStepKind;
  label: string;              // human sentence, as today's activity labels: "Searching your documents"
  detail?: string | null;     // the query, the host, the tool's argument summary (≤200 chars)
  state: 'running' | 'done' | 'failed';
  startedAtMs: number;        // offset from the start of the turn
  durationMs?: number | null;
  result?: string | null;     // "8 passages from 3 files", "not enough support: low term coverage"
}

interface TurnRecord {
  model: { id: string; name: string } | null;   // the model that answered THIS turn
  steps: TurnStep[];                            // capped at 200
  timing: { totalMs: number; routerMs: number; retrievalMs: number; generationMs: number; verificationMs: number; toolMs: number };
  tokens: { completion: number | null; contextUsed: number | null };
  router: { action: string; confidence: number; rationale: string | null } | null;
}
```

- **Stream:** `llm-stream` gains `status: "step"` with `step: TurnStep`. It replaces
  `status: "activity"` and `status: "retrying"` (no legacy: remove both, the heartbeat included;
  the UI shows an elapsed clock on the running step). A retry is a `retry` step and **must no
  longer overwrite the answer bubble's text**.
- **Collector:** a `TurnRecorder` owned by the turn that both emits and accumulates. Thread it
  where the stream emitter is threaded today (`tool_loop.rs` emitter, `chat.rs`, the retrieval
  pipeline). Emit from: router (`route`, with confidence/rationale kept instead of discarded in
  `resolve_router_decision`), retrieval pipeline (`plan`, `search_documents`, `sufficiency` with
  the verdict's reasons, `corrective_search`), external lookup, every tool call in the tool loop
  (start and finish, name, argument summary, ok/failed, ms), generation, verification.
- **Persist:** `metadata.turn` beside `sources`, `verification`, `retrieval` in
  `chat/persistence.rs`. Typed with specta so it reaches `bindings.ts`; mirrored by a strict zod
  schema in `src/types/conversation.ts` like `MessageVerificationSummarySchema`. `totalMs` is time
  to the persisted answer.
- **A `closed_book` turn** (journal synthesis) records `generate` only and emits nothing about
  retrieval.
- **Frontend state:** `liveSteps: Map<conversationId, TurnStep[]>` (start/finish merged by id) and
  `messageTurn: Map<messageId, TurnRecord>` in the conversations store; the controller handles
  `status: 'step'`; cleared exactly where `liveRetrieval` is cleared.
- **Component** `src/components/Chat/turn/TurnRecord.tsx`:

```tsx
<TurnRecord
  trace={retrievalTrace}              // RetrievalTrace | null — unchanged source
  record={turnRecord}                 // TurnRecord | null — null on turns persisted before this
  liveSteps={liveSteps}               // TurnStep[] | null — only while pending
  isPending={isPending}
  verification={verificationSummary}  // for "4 of 5 claims backed"
/>
```

  Collapsed: one line — what `RetrievalTrace` says today, then claims backed, model, total time —
  and a chevron. Expanded: the steps as a timeline with durations, the router's rationale, the
  sufficiency reasons (`kbSufficient` and `sufficiencyReasons` are persisted today and rendered
  nowhere), retries. While pending it is open and live: finished steps tick off, the running step
  shows an elapsed clock; when text starts streaming it folds to its one line and stays. It
  replaces `RetrievalTrace` and `ActivityNote` at their mount point in `Message.tsx`; keep every
  honesty rule in `RetrievalTrace.tsx` (no "Searched 0 documents"; a plain sufficient verdict says
  nothing). Leave `RetrievalTrace.tsx` and `ActivityNote.tsx` in place: the integrator deletes them
  when it swaps the mount.
- **A3 — focus documents:** `focus_document_ids` is intersected with the conversation's space
  scope; ids outside it are dropped silently and logged. Non-empty after intersection: KB search
  is forced and both KB retrieval and `scoped_document_tools` are confined to the intersection.
  Empty after intersection while the request named some: the turn searches nothing from the vault
  and the trace says why (fail closed — never fall back to the whole space). `RetrievalTrace`
  gains `focusedDocuments?: number`. Tests: a focus id from another space is ignored; focus
  narrows; focus never widens; `closed_book` wins over focus.

### 4.3 B — the docked reader

```ts
// src/stores/chatReaderStore.ts (zustand, same pattern as the other stores)
interface ReaderSession { ownerKey: string; citations: SourceWithMetadata[]; index: number }
interface ChatReaderState {
  session: ReaderSession | null;
  width: number;                                   // persisted: localStorage 'chat.reader.width'
  open(ownerKey: string, citations: SourceWithMetadata[], index: number): void;
  setIndex(index: number): void;
  close(): void;
  setWidth(px: number): void;
}
```

- `FilePreviewModal` today is a Radix portal `Dialog`, fixed to the right edge, mounted once **per
  message**; opening a source covers the answer it belongs to. Factor its body out so the same
  content renders two ways: `presentation="reading-pane"` (the overlay, kept as the fallback) and
  a new docked form with no Dialog and no portal.
- `src/components/Chat/reader/ChatReaderPane.tsx` is mounted **once**, as the third child of the
  flex row in `ChatView.tsx`. A drag handle on its left edge (`role="separator"`,
  `aria-orientation="vertical"`, arrow keys move it 16px, double-click resets) resizes it between
  380px and `min(60% of the row, 900px)`. Esc closes it and returns focus to the chip that opened
  it. `[` / `]` citation travel, the focus-expand toggle, "Ask about this", "Add to journal" and
  "Reference" keep working.
- When the row is too narrow for a ≥560px chat column beside a 380px reader, open the overlay
  instead. Decide with a `ResizeObserver` on the row, not the viewport.
- Docking narrows `.chat-panel`, so the evidence margin folds away through its container query.
  Verify that in the rig rather than adding code for it.
- `Message.tsx` loses `previewIndex` and its `<FilePreviewModal>`; chips, footnotes, "View source"
  and `EvidenceMargin.onOpen` call `open(ownerKey, citationSources, index)`. The citation the
  reader is showing is lit in the answer it came from (`.is-lit` on `[data-cite="n"]`, only when
  `session.ownerKey` is this message). `onLocationResolved` keeps feeding `rememberLocation` and
  the message's `resolvedLocations`.
- Switching conversation closes the reader.

### 4.4 C — the composer

- Keep the `<textarea>`. Do not move the composer to a rich editor. `@` and `/` open a
  caret-anchored popup (`composer/ComposerSuggest.tsx`): ↑/↓ move, Enter/Tab accept, Esc closes,
  typing filters. While it is open Enter must not send. Respect IME composition
  (`event.nativeEvent.isComposing`).
- `/` — `composer/slashCommands.ts`: `/deep` (deep research), `/web`, `/wiki`, `/docs` (your
  documents only), `/followup`, `/query`, `/auto`, and the existing `/compact` (today a bare regex
  on submit with no discoverability; fold it in). Each row: name, one line saying what it does,
  current state. Accepting one removes the typed token and flips the same state
  `ComposerControls` flips.
- **Modes are visible.** Any non-default mode shows as a removable chip in the composer's bottom
  bar beside the space and model labels ("Deep research ×", "Web ×"). The gear popover stays for
  the full list and custom tools. A turn that will take two hours should not be a hidden checkbox.
- **Attach button.** A paperclip in the bottom bar runs the flow the palette command "Add files
  to this conversation" runs today.
- `@` (last; needs A0) — `listSpaceDocuments(conversationSpaceId, query, 8)`. Accepting removes
  the `@query` token and adds a focus chip above the textarea ("Only: Halvorsen 2024.pdf ×"). The
  ids go out as `toolPreferences.focusDocumentIds` and persist with the conversation's other tool
  preferences until removed. With focus set, the placeholder and the space label say so ("Asking
  2 documents in Heat Island Thesis"). Moving the chat to another space clears focus.
- Rig shots: `chat-composer-slash`, `chat-composer-mention`, `chat-composer-modes`.

### 4.5 D — out of chat

- `src/utils/conversationExport.ts` — `conversationToMarkdown(conversation, messages, options)`:
  title, space, date; each turn; each answer followed by its numbered sources (file, location) and
  a verification line ("4 of 5 checked sentences backed; not found: …"); the model and time when
  `metadata.turn` exists (read it defensively — Track A may not have landed it). Pure function,
  tested like `Compare/compareMarkdown.ts`.
- Two ways out, both from the palette and the sidebar row menu: **Copy conversation as Markdown**
  and **Save conversation to Journal** (the capture API the Compare page uses). Put the hook
  beside `sidebar/useConversationSynthesis.ts` and mount it where that one is mounted.
- `MessageActions` gains, for an assistant answer with sources: **Add to journal** (the answer with
  its citations) and **Compare sources** (navigate to `/compare?ids=…` with the answer's distinct
  vault document ids; needs ≥2; web sources excluded). Read `MessageActions.test.tsx` first: verbs
  stay visible without hover. An always-visible "More" menu is acceptable; hover-only is not.
- `actions/ClaimActionsPopover.tsx` — what clicking a checked sentence offers:

```tsx
<ClaimActionsPopover
  anchor={rect}                 // DOMRect of the clicked line
  verdict={claimVerdict}        // ClaimVerdict
  citationMap={citationMap}     // Map<number, SourceWithMetadata>
  conversationId={conversationId}
  onClose={() => …}
/>
```

  Actions: **Copy with citation** (the sentence plus "— file, location"), **Add to journal**,
  **Ask why** (navigates with the existing `?quote=` parameter that `ChatView` already turns into
  composer text; for an unsupported sentence the prefill is a request for a source), **Make a
  flashcard** only if an API for a single card exists — if only deck generation exists, leave it
  out and say so. Interactive, keyboard reachable, `.surface-pop`. The integrator wires the click.
- `EvidenceMargin` footer: "Compare these N documents" when the notes span ≥2 vault documents.
- Branch lineage (last; needs A0): a forked conversation's sidebar row and the chat header say
  "Branched from <parent title>" and link to the parent at `forkedFromMessageId`. A dangling
  parent id shows nothing.

## 5. Integration (after all four tracks report)

1. Mount `TurnRecord` in `Message.tsx` in place of `RetrievalTrace` + `ActivityNote`; delete those
   two files and their tests' dead cases.
2. Wire claim click → `ClaimActionsPopover` in `Message.tsx` (hover card stays pointer-only).
3. Thread D's new `MessageActions` props through `Message.tsx`.
4. Merge every track's rig fixtures and shots into the durable rig; full `both` run.
5. Full gates: `tsc`, `npm run lint`, `vitest run`, `cargo test --lib`, clippy, `bindings:check`.
6. One real run (`npx tauri dev`): a live turn shows steps ticking, a retry does not eat the
   answer, the reader docks and resizes, `@` only offers documents of the chat's space.
