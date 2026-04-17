# Chat Redesign — Implementation Specification

**Status:** proposed
**Pilot:** first screen rebuilt against the new design system. Patterns established here are precedent for Dashboard, Reference Inbox, Daily Notes.
**Paired with:** `AESTHETIC-GUIDE.md`, `TOKENS-SPEC.md`, `TOKEN-AUDIT.md`.
**Scope:** `src/app/websrc/components/Chat/**` only. No backend, no hooks, no state, no transport changes.

---

## 1. Product soul of Chat

Chat is a **reading surface first, a composing surface second**. The user spends far more time reading assistant output — prose, citations, retrieved context — than typing prompts. The current design inverts that priority: gradient avatars, glowing bubbles, and a visually loud composer fight the content for attention. In the new design, the canvas recedes so the user's material (their question, the assistant's answer, the cited excerpts from their own files) is what looks good. Per AESTHETIC-GUIDE §1, "the interface is a lens onto the user's own material" — Chat is the clearest test of that principle in the app. Closer to a text editor with a prompt rail than to a chatbot UI.

---

## 2. Layout

### 2.1 Tri-pane, sidebar-inset

Keep the current tri-pane shape — `ConversationSidebar` | `ChatPanel` | (spotlight as overlay). Drop the `ConversationLinkedDocumentsPanel` from being a header strip; relocate as described in §3.

- **Sidebar:** fixed width `280px` at default viewport, collapsible to `56px` icon rail. No `clamp()` (TOKENS-SPEC §4 — density is a fixed grid on a desktop app). Collapse state persists in `localStorage` under `chat.sidebar.collapsed`.
- **Main panel:** flex-1, min-width `640px`. Contains a single vertical scroll region with the message thread; composer is pinned to the bottom of this panel (see §4.1).
- **No header chrome** on the main panel. The current error banner and linked-documents panel are removed from the header rail. Errors surface as toasts (existing `toastStore`); linked documents move into the empty-state placeholder and a trailing "Sources in this conversation" region below the last message (see §3.6).

### 2.2 Reading column

The message thread is **centered** within the main panel with a `max-width: 760px` reading column (~72ch at 16px body), regardless of panel width.

Reasoning: prose readability research puts the optimal measure at 50–75 characters for sustained reading; serif body at 16px / 1.6 line-height places 72ch around 720–760px. Current full-bleed messages with 16–24px horizontal padding push lines to 120+ characters — editorial disaster.

Padding inside the column: `24px` left/right, `32px` top/bottom between messages. Vertical gap is the primary separator; no borders or dividers between messages (see §3).

### 2.3 Sidebar behavior

Fixed, always visible at expanded default. Collapsible via a keyboard shortcut (`⌘\`) and a hit target on the sidebar edge. Not hover-revealed — hiding primary navigation behind hover is a confidence failure in a thinking tool. When collapsed, shows a single rail of icons: new conversation, space picker, search, filter tabs. No tooltip animation — instant tooltip on hover per AESTHETIC-GUIDE §6 ("If animation is noticed, it's too much").

### 2.4 Composer position

**Pinned to bottom of main panel**, inside the centered reading column. Not fixed to viewport — pinned within the flex column, so it stays with the panel if other surfaces ever overlap. Subtle top hairline border (`--border-subtle`), no gradient, no blur. The composer floats inside the panel on its own canvas tone (`--bg`) — the thread above uses `--bg` as well; the composer separates via the hairline only.

---

## 3. Message system

The heart of the redesign. Current implementation violates AESTHETIC-GUIDE §3 in four ways: gradient avatars, tinted message backgrounds, heavy rounded bubbles, and action buttons with blur/gradient/opacity-border.

### 3.1 Message row — no bubbles

**Both user and assistant messages render as bare prose blocks.** No rounded rectangles, no background tints, no avatar circles.

The distinction between user and assistant is carried by:

1. **Label** — a left-aligned `text-xs` uppercase-tracking-wide label (`You` / `Assistant` / model name) at `--text-tertiary`, placed `8px` above the message content.
2. **Rule** — a `1px` top rule in `--border-subtle` extending the reading column width, above each message. The rule is the separator; background tint is not.
3. **Typography** — assistant prose uses `--font-serif` at `text-base` (16px) / 1.6; user messages use `--font-sans` at `text-base` / 1.5. The typographic distinction is the strongest signal. (AESTHETIC-GUIDE §2: "Typography carries hierarchy, not borders.")

No avatar icons. The `Bot` and `User` lucide icons at 36×36 with gradient fills are deleted. An avatar adds no information that the label and the visible turn-taking doesn't already carry, and it takes valuable horizontal space from the reading column.

**Defense of the "no bubble" call:** claude.ai, Perplexity, and ChatGPT's GPT-4 canvas mode have all converged on bare-prose assistant output over bubbled output for reading-heavy LLM UIs. Bubbles imply parity between turns (as in iMessage); in Chat, turns are deeply asymmetric — user turns are short, assistant turns are long prose. Treating the assistant turn as editorial content (with a serif) and the user turn as a prompt (with a sans label) makes the asymmetry legible at a glance. If during user testing the user turn feels lost, we add a thin left-border rule (2px, `--border-default`) to user messages only — never a bubble fill.

### 3.2 Metadata row

Below the label, on the same line where possible:

- Model name (for assistant, e.g. `Assistant · claude-opus-4.7`) — `--text-muted`, `text-xs`, no separator decoration, interpunct between label and model.
- Timestamp — `--text-muted`, `text-xs`, right-aligned in the row. Visible always (not hover-revealed).
- Verification badge (if present) — inline-flex chip at `text-xxs`, uppercase letter-spacing +loose, uses the `--success-muted` / `--success-fg` pair for "verified", `--warning-muted` / `--warning-fg` for "partially verified", `--surface` / `--text-muted` for "off". No filled backgrounds beyond the semantic `-muted` tokens. Rules out the current `bg-emerald-500/15 border-emerald-400/35` opacity-border pattern.
- Bookmark indicator (if bookmarked) — small `Bookmark` icon at `--accent`, inline, no pill.

### 3.3 Message content — prose

Rendered Markdown via `TiptapViewer` (unchanged pipeline). The styling changes:

- **Body text:** `--font-serif`, `text-base` (16px), `line-height: 1.6`, `--text-primary`. Max-width = reading column.
- **Headings inside prose (`h2`, `h3`):** `--font-serif`, `text-xl` / `text-lg`, weight 600, top-margin `1.5em`.
- **Paragraphs:** `margin-bottom: 1em`, first-child no top-margin.
- **Lists:** standard prose indentation; bullet markers at `--text-tertiary`.
- **Code blocks:** `--font-mono`, `text-sm`, `--surface` background, `--border-subtle` border, `--radius-sm`, `12px 16px` padding. Syntax highlighting uses a monochrome-plus-accent palette derived from the token system (keyword = `--accent`, comment = `--text-muted`, string = `--text-secondary`, everything else = `--text-primary`). The current rainbow `lowlight` theme is replaced. (Deferred detail: exact syntax palette is component-designer's call, but no warm hues, no rainbow.)
- **Inline code:** `--font-mono`, `text-[0.9em]`, `--surface` background at `0 0% opacity` (flat — no tint), `--border-subtle` `1px` border, `2px 4px` padding, `--radius-sm`. No background saturation.
- **Links:** `--accent` color, `text-decoration: underline`, `text-decoration-thickness: 1px`, `text-underline-offset: 2px`. No dotted, no hover color shift beyond `--accent-hover`.
- **Blockquote:** `3px` left-border in `--border-strong`, `16px` left padding, italicized, color `--text-secondary`. No background.

### 3.4 Citations

Two surfaces:

1. **Inline footnote marker** (`CitationFootnote.tsx`) — rendered as `[1]`, `[2]`. Styled as `--accent`-colored, `text-xs`, `sup` superscript, `font-mono`, no underline by default, underline on hover. Click reveals a Radix popover at `--surface-raised` with `--border-subtle` 1px border, `--shadow-md`. Popover shows filename, category, excerpt preview, and a "View source" link. Replaces current Radix Tooltip; popover gives click-to-pin behavior the tooltip can't.
2. **End-of-message cited documents list** — below the prose, separated by `16px` margin. No border rule above it (the rule would read as a divider between prose and an equal content block; the spacing alone is sufficient). Shown as a collapsed summary by default:

   ```
   3 cited sources · 5 excerpts          [chevron]
   ```

   At `text-sm`, `--text-tertiary`. Click to expand. Expanded view: a plain list (not cards) of source entries, each one:

   - Filename (bold, `text-sm`, `--text-primary`).
   - One-line metadata (category, relevance %, excerpt count) at `text-xs`, `--text-muted`.
   - Excerpt preview in `--surface`-tinted block with `--border-subtle` hairline, `--radius-sm`, `12px 16px` padding. Prose in sans (this is reference material, not editorial), `text-sm`, `--text-secondary`.
   - "View source" link aligned right at the row level, `--accent`, underlined on hover.

   **Removed:** per-source cards with hover states and separate metric badges. The current "rounded-xl border, hover:bg-white/[0.06]" cards are cut. This is a list, not a gallery.

### 3.5 Tool calls

There is **no current tool-call UI** in Chat. Tool invocations stream via the same text stream and get embedded in the assistant's prose (the backend owns the rendering). This is a known gap. Two options:

- **Option A (minimal, in scope):** continue letting the backend render tool usage in-prose. No new component. We just restyle `<details>` or fenced blocks that carry tool output so they match the new system.
- **Option B (bigger, out of scope for this pass):** new `ToolCallBlock.tsx` component that handles an explicit tool-call envelope in the message data model, with a collapsed summary row (tool icon, name, one-line input summary) and an expanded view showing structured input / output. Requires changes to the message schema (violates scope boundary §9).

**Recommendation: Option A for this pass. Flag Option B as follow-up work.** Spec below covers the Option A restyle.

When a tool call appears in prose (e.g. `web_search(...)` → results), it should render as a fenced block:

- Container: `--surface` background, `--border-subtle` 1px border, `--radius-md`, no shadow.
- Header row (always visible): tool icon (lucide, 14px, `--text-tertiary`), tool name (`--font-mono`, `text-xs`, `--text-secondary`), one-line argument summary (`--text-muted`, truncated), right-aligned chevron to expand.
- Expanded region: input as `--font-mono` `text-xs` key:value rows, output as flowing prose or code depending on tool. `12px 16px` padding. `border-top: 1px solid --border-subtle`.
- Transitions: height transition `--duration-base` `--ease-out`. No slide, no scale.

### 3.6 Retrieval display (KB results outside of citations)

When retrieval runs but the assistant hasn't yet folded results into prose — e.g. mid-stream, or explicit `Query` mode — display the raw retrieval hits as a **collapsible inline region above the assistant's streaming response**, not as a separate pane:

- Collapsed: one-line summary `Retrieved 8 excerpts from 3 documents · KB + Web` at `--text-tertiary`, `text-xs`, with a chevron.
- Expanded: same list pattern as §3.4 source expansion.

Rationale: retrieval is context for the assistant's next sentence. Pulling it into a separate side panel (as the current `ConversationLinkedDocumentsPanel` suggests) creates a parallel attention surface. Keep it inline, collapsible, close to the turn it belongs to.

**ConversationLinkedDocumentsPanel** itself is a conversation-wide aggregation of all sources touched. It moves from the top header strip into a **sticky footer region of the thread** (above composer, below last message), rendered only when there are linked documents. Same collapsed/expanded pattern. Empty when no documents have been linked.

### 3.7 Streaming

**Just-appear, no character cascade.** New tokens append to the end of the streaming assistant message with no animation. A subtle **two-character blinking cursor** (a `▍` U+258D at 0.5em width) appears at the streaming end position, color `--text-muted`, animated via `animation: blink 1s steps(2, end) infinite`. Stops when `done: true` arrives.

Defense: character-reveal (typewriter) animations are performative and slow reading. A static streaming block with a cursor is what actual IDEs do (VS Code Copilot Chat, Cursor). AESTHETIC-GUIDE §6: "If animation is noticed, it's too much."

No streaming "shimmer" effect. The `animate-pulse` opacity bounce on the avatar is deleted (there is no avatar anymore).

### 3.8 Action rail (copy, delete, bookmark)

Moves from the right-margin floating rail to the **bottom of each message**, inline, below the source-citations region. Shown at low contrast and full opacity always (not hover-revealed — hover-hidden controls fail discoverability for power users, per AESTHETIC-GUIDE §2).

- Row of `text-xs` buttons: `Copy` / `Bookmark` / `Delete`, each a text-plus-icon pair, no border, no background. `--text-muted` default, `--text-secondary` on hover. No tinted-background hover states (no `hover:bg-red-500/20`).
- Destructive "Delete" hovers to `--danger` color only, no background fill.

If always-visible feels too busy in review, fallback: hover-reveal on the row only (not individual buttons), with `opacity 0 → 1` over `--duration-fast`. Decision owner: component-designer. Default: always visible.

---

## 4. Composer

### 4.1 Container

Pinned at the bottom of the main panel (see §2.4). `--bg` background, `1px solid --border-subtle` on top edge only, `24px` horizontal padding matching the reading column, `16px` vertical padding.

### 4.2 Textarea

- Bordered box: `1px solid --border-default`, `--radius-md`, `--surface` background.
- Focus state: border becomes `--ring` (2px), no outline, no box-shadow halo. TOKENS-SPEC §8 focus pattern applied with the input-adjacent fallback (box-shadow ring 2px `--accent`).
- Font: `--font-sans`, `text-base` (16px) — not monospace. Prompts are prose, not code.
- Placeholder: `--text-muted`. Three states:
  - Default turn mode (`auto`): `"Message..."` (lowercase, no ellipsis punctuation — Linear pattern).
  - Follow-up mode: `"Follow up on this conversation..."`
  - Query mode: `"Search and answer..."`
- Min-height `48px`, max-height `240px` (up from `160px` — gives 8–10 lines of prompt room before scroll). Auto-resize on input.
- Padding: `12px 16px`.

### 4.3 Send button

- **Icon-only**, right-aligned, absolute-positioned inside the textarea container at `bottom: 12px, right: 12px`.
- `32×32`, `--radius-sm`.
- Default state: `--accent` fill, `--accent-fg` icon. 5.35:1 contrast per TOKENS-SPEC §1.4.
- Hover: `--accent-hover`.
- Disabled (empty input or sending): `--border-default` fill, `--text-muted` icon, no cursor pointer. TOKENS-SPEC §1.3 muted register.
- **No gradient, no blur, no shadow.** Replaces the current `bg-gradient-to-r from-blue-500/20 to-indigo-500/20 ... backdrop-blur-sm shadow-blue-500/10` treatment.
- During streaming, swaps to a **Stop** button: `--danger-muted` fill, `--danger-fg` icon, same geometry. No gradient.

### 4.4 Secondary controls — model picker, tools, turn mode

Current implementation surfaces a collapsible "Controls" panel with 10+ buttons via a `SlidersHorizontal` toggle. This is correct instinct (keep chrome quiet) but the collapsed state still shows visible pill badges for active flags, and the expanded panel is itself heavily styled. Redesign:

- **Active-flags row below textarea:** remove. If the controls panel is meaningful, the user opens it. If it's not, they don't need to see the active flags leaked. Retains only a `⌘⏎ to send · ⇧⏎ for new line` keyboard hint at `--text-muted`, `text-xs`, right-aligned.
- **Controls trigger:** a single icon button `Settings2` at the left of the textarea row, inside the textarea container (mirror position of Send). `24×24`, no border, `--text-muted` default, `--text-secondary` active. Opens a Radix Popover anchored above.
- **Controls popover:** `--surface-raised`, `--border-subtle` 1px, `--radius-md`, `--shadow-md`. Width `320px`, max-height `480px`. Structure:
  - **Section 1: Turn mode.** Segmented control (three tabs: Auto / Follow-up / Query) using shadcn-style tabs. Active tab: `--surface` fill, `--text-primary`. Inactive: transparent, `--text-tertiary`, hover `--text-secondary`. No colored active states.
  - **Section 2: Tools.** Checkbox list (not button pills): KB, Web, Wiki, Deep Research, plus custom tools. Each row: checkbox (shadcn `Checkbox`), tool name (`text-sm`, `--text-primary`), brief description (`text-xs`, `--text-muted`). Click-anywhere on row toggles.
  - Deep Research warning chip shows inside the popover when enabled, using `--warning-muted` / `--warning-fg`.
- **Model picker:** separate icon at the left of the controls trigger. Uses shadcn `Select` primitive. Shows current model as small text label below the icon (e.g. `opus 4.7`), `text-xxs`, `--text-muted`. Clicked, opens a Select dropdown with all downloaded models.

### 4.5 Attachments & drag-drop

Currently unsupported. **Leave unimplemented.** If we add attachments in a future pass, the pattern is: another icon button in the composer control rail, and a drag-over overlay that appears (fade in `--duration-fast`) only when files are being dragged over the composer panel. Nothing visible when idle.

### 4.6 Slash commands / cmdk

Spotlight (`⌘K`) already handles conversation search. Slash menu for in-message commands is not currently implemented. **Out of scope for this pass.** Flag as follow-up if `/` commands become a product need.

---

## 5. Conversation sidebar

Current file is `97KB / 2278 lines` and conflates: spaces picker, filters, search, selection mode, bulk move, journals, snippets/references, conversation list. That's 8+ responsibilities. This redesign does **not** split the component file (file surgery is out of scope), but it does simplify the visual treatment of each sub-region so the whole reads as one coherent sidebar.

### 5.1 Structure (top to bottom)

1. **Top rail** (48px): Recall logo/wordmark at `text-sm` `--font-serif` `font-semibold`, collapse toggle (`PanelLeft` icon) right-aligned. `--surface` background. Hairline bottom border.
2. **New Conversation button.** Full-width, `--accent` fill, `--accent-fg` text, `Plus` icon + "New Conversation". `--radius-md`. `32px` height. No gradient, no blur. Replaces the current tinted hover-gradient treatment.
3. **Space scope row.** Compact row showing current space name with a small icon and a "Spaces" button that opens the spaces management panel. `text-sm`, `--text-primary` for name, `--text-muted` for "Space Scope" label.
4. **Filter chips.** Horizontal row of 4-5 filters (All, Starred, Archived, Snippets, Journals). Inactive: `--text-tertiary`, no border, no background. Active: `--text-primary`, `2px` underline in `--accent` (not background fill). No icon-only button grid with tinted states.
5. **Search input.** Single input with `Search` icon, placeholder `"Search conversations..."`. Same styling as composer textarea (§4.2) but `text-sm` and `32px` height. No always-visible focus halo.
6. **Conversation list.** Primary area, flex-1. See §5.2.

### 5.2 Conversation list

Each row:

- Height: `48px` minimum, grows if title wraps to 2 lines (max 2).
- Padding: `8px 16px`.
- No border, no background by default.
- Title: `--font-sans`, `text-sm`, `--text-primary`, line-clamp-1.
- Subtitle: `text-xs`, `--text-muted`, line-clamp-1 — content: relative timestamp (e.g. "3h ago").
- Hover: `--surface` background, `--radius-sm`. No border.
- **Active (selected) conversation:** `--surface` background plus **`2px` left-bar in `--accent`** (flush-left, from row top to row bottom). **Not both** border and filled-tint highlight (AESTHETIC-GUIDE §6). Title becomes `--text-primary` weight 500.
- Hover-revealed actions: `Rename`, `Star`, `Delete` icons appear on the right side with `opacity: 0 → 1` over `--duration-fast`. Icons only, `14px`, `--text-tertiary`, hover `--text-primary` (or `--danger` for delete). No background pill, no blur.

### 5.3 Grouping

Replace free `formatDistanceToNow` labels with **section headings** per group:

- Today
- Yesterday
- Last 7 days
- Last 30 days
- Older

Heading style: `--font-sans`, `text-xxs`, uppercase, `letter-spacing: 0.08em` (+loose per TOKENS-SPEC §3.2), `--text-muted`, `8px 16px` padding, `16px` top-margin (except first group).

Journal mode uses date headings (`Today`, `Yesterday`, `Wed Apr 15`) in `--font-serif` italic instead — editorial distinction for journal. Weight 500, `text-xs`, `--text-tertiary`.

### 5.4 Space colors

Current implementation uses per-space accent colors applied as backgrounds, borders, and inset shadows on conversation rows. This creates rainbow chaos — AESTHETIC-GUIDE §1 says "one neutral palette, one accent."

**Revised rule:** space accent colors appear **only** in two places:
1. As a `8×8` dot indicator next to the conversation title (subtle identification).
2. As the `2px` left-bar accent for the **space picker itself** when a space is selected (replacing the universal `--accent` bar for scoping context).

All other per-space color tinting (card backgrounds, row backgrounds, bookmark backgrounds) is removed. Accent color per row is too much signal for the cost.

### 5.5 Empty state

Centered in the conversation list area:

- `MessageSquare` icon, `32px`, `--text-muted`, opacity 1 (not 0.5).
- `text-sm`, `--text-tertiary`: `"No conversations yet."`
- `text-xs`, `--text-muted`: `"Start a new conversation above."`
- 16px vertical gap between items.

No illustration, no decorative panel.

### 5.6 Spaces management panel (the large floating aside)

Currently opens as a blurred overlay with `bg-[#0b1118]/95 backdrop-blur-xl`. Redesign:

- Use shadcn `Dialog` primitive instead of hand-rolled createPortal.
- Background: `--surface-raised`, no blur.
- Width: matches sidebar width, anchored to the sidebar's right edge.
- `--shadow-md` for elevation.
- Internal rows: same pattern as §5.2.

---

## 6. Conversation Spotlight (⌘K)

Currently: full-screen blurred overlay with a centered panel using `bg-[#0f1723]` (hardcoded hex).

Redesign:

- Use shadcn `Dialog` with `cmdk` command palette patterns.
- Overlay: `--overlay` color, no blur.
- Panel: `--surface-raised`, `--border-subtle` 1px, `--radius-lg`, `--shadow-md`. Max-width `640px`. Top-anchored `120px` from viewport top.
- Search input inside: identical to composer textarea treatment (§4.2), `text-sm`, no background chip around the search.
- Results list: flat list, same row pattern as §5.2. Selected (keyboard-focused) row: `--surface` background, `--accent` 2px left-bar. No blue tint background.
- Bookmark results use an inline `Bookmark` icon in `--accent` (not the current amber tint with amber-hover — unify to one accent).
- "ESC" hint badge: `text-xxs`, `--font-mono`, `--border-default` 1px, `--radius-sm`, `4px 6px` padding. Same treatment as kbd elsewhere.

---

## 7. States

| State | Render |
|---|---|
| **No conversation selected** | Centered in main panel. Small `MessageSquare` icon at `32px` `--text-muted`. Heading `text-lg` `--font-serif` `"No conversation selected"`. Body `text-sm` `--text-tertiary` `"Pick a conversation from the sidebar, or start a new one."` Button to create new (same treatment as sidebar button). No gradient, no glowing square, no icon in a gradient box. |
| **Empty thread (conversation exists, no messages)** | Same pattern as above but with message `"This conversation is empty. Ask a question to begin."` Composer visible and ready. |
| **Streaming in progress** | Assistant message renders with accumulating content + blinking cursor (§3.7). Composer Stop button visible (§4.3). No other UI state change. |
| **Network error mid-stream** | Error appended inline as a `--danger-fg` `text-sm` line with `AlertCircle` icon (12px) at the end of the streaming message. Retry affordance: `"Retry"` link in `--accent` next to the error. No full-width red banner. |
| **Tool call pending** | Tool-call block (§3.5) renders with `Loader2` spinning icon (14px, `--text-muted`) in the header, argument summary shown. |
| **Tool call running** | Same as pending; timer in the header row if running >3s: `"5s"` at `text-xxs` `--text-muted`. |
| **Tool call failed** | Border becomes `--danger-muted`, header prefix icon switches to `AlertCircle` in `--danger-fg`. Output region shows error text. |
| **Retrieval returned 0 results** | Inline collapsible region (§3.6) shows `"No results from KB + Web"` at `--text-tertiary`, collapsed by default. Does not render if retrieval didn't run. |
| **Very long response** | Do not cap scroll. The reading column scrolls naturally. |
| **Very long thread (>200 messages)** | Consider `@tanstack/react-virtual` integration. **Flagged as risk — see §10.** Default: no virtualization in first pass; revisit if measured performance is poor. |

---

## 8. Motion

Per TOKENS-SPEC §7 and AESTHETIC-GUIDE §2.5, motion confirms causality. It does not perform.

**Animates:**

- New message arriving (fade-in only, `opacity: 0 → 1`, `--duration-fast`, `--ease-out`). No `translateY` rise. AESTHETIC-GUIDE §3 bans staggered cascades; this is a single-element fade.
- Tool-call block expand/collapse: height transition, `--duration-base`, `--ease-out`.
- Source-citations expand/collapse: same.
- Streaming cursor: `1s steps(2, end) infinite`, `--text-muted`.
- Popover/modal open (Spotlight, Controls, Spaces): fade + `scale(0.98 → 1)`, `--duration-base`, `--ease-out`. Radix defaults preserved; TOKENS values swapped in.
- Hover-revealed icons on sidebar rows: `opacity`, `--duration-fast`.
- Error retry link hover: color only, no motion.

**Does NOT animate:**

- Page mount (no staggered `fadeInUp` on children).
- Message list on initial load (messages just appear).
- Composer focus (focus ring appears instantly per TOKENS-SPEC §8).
- Send button hover (color state change, no transform).
- Sidebar row hover (instant `--surface` background, no transform).
- Avatar pulse (no avatar).
- Scroll-into-view of deep-linked message: uses native `scrollIntoView({ behavior: 'smooth' })`; **remove** the current `ring-2 ring-cyan-300/70` flash highlight — replace with a `--accent` 2px left-bar that persists for 1.5s then fades (`--duration-base`). Cleaner than a glowing ring.

---

## 9. Component ownership

### Files under `components/Chat/`

| File | Action | Notes |
|---|---|---|
| `ChatView.tsx` | **MODIFY** | Remove `ring-cyan-300/70` from scroll-into-view effect (§8). Minor — preserves structure. |
| `ChatPanel.tsx` | **REWRITE** | Core file for the redesign. Composer, error banner, empty states, thread container all rebuilt. Internal composer controls move to a Popover. |
| `MessageBubble.tsx` | **REWRITE** | Rename to `Message.tsx` (no more bubble). Full restyle per §3. Remove all gradient avatars, tinted backgrounds, action-button opacity-borders. Verification panel restyled; source-citation cards become list. |
| `ConversationSidebar.tsx` | **MODIFY** (heavy) | No file split (out of scope). Restyle all sub-regions per §5. Remove hardcoded `bg-[#0b1118]/95`, replace with `--surface` / `--surface-raised`. Replace spaces panel portal with shadcn Dialog. Remove per-space row accent tinting (reduce to 8×8 dot). |
| `ConversationSpotlight.tsx` | **REWRITE** | Replace hardcoded `bg-[#0f1723]` and the hand-rolled dialog with shadcn Dialog + cmdk patterns. |
| `ConversationLinkedDocumentsPanel.tsx` | **MODIFY** | Restyle per §3.6. Relocate out of header, into trailing footer region of thread. Remove opacity-suffix border/bg treatment. |
| `CitationFootnote.tsx` | **MODIFY** | Replace `var(--accent-primary)` / `var(--accent-hover)` / `var(--accent-light)` with `--accent` / `--accent-hover` (flattened). Replace Radix Tooltip with Radix Popover for click-to-pin. Update classes to use `--surface-raised` / `--border-subtle`. |
| `FilePreviewModal.tsx` | **MODIFY** | Replace bespoke dialog styling with shadcn Dialog primitive consumption. Token cleanup. |
| `viewers/ImageViewer.tsx` | **KEEP** | Small; no known token debt. |
| `viewers/MarkdownViewer.tsx` | **KEEP** | Small. |
| `viewers/PDFViewer.tsx` | **KEEP** | Small. |
| `viewers/TextViewer.tsx` | **KEEP** | Small. |
| `viewers/__tests__/*` | **KEEP** | Tests unchanged. |
| `index.ts` | **MODIFY** | Update export name if `MessageBubble` → `Message`. |

### New files

- `components/Chat/Message.tsx` — replaces `MessageBubble.tsx` (rename).
- `components/Chat/MessageActions.tsx` (small, optional) — extracts the `Copy` / `Bookmark` / `Delete` row.
- `components/Chat/SourceCitations.tsx` (optional) — extracts the end-of-message cited-sources list for readability.
- `components/Chat/ComposerControls.tsx` — the Popover body for turn mode + tools + model picker.
- `components/Chat/ToolCallBlock.tsx` — **out of scope** for this pass (see §3.5). File listed for future reference only; do not create yet.

### shadcn primitives to adopt

- `Dialog` — used by `ConversationSpotlight`, `FilePreviewModal`, spaces panel.
- `Popover` — used by controls popover in composer, citation footnote on click.
- `Tabs` — used by turn-mode segmented control in composer controls popover.
- `Checkbox` — used by tools list in composer controls popover.
- `Select` — used by model picker.
- `ScrollArea` — used inside long source lists / composer controls popover.
- `Button` — used for all buttons (replace hand-rolled buttons; AESTHETIC-GUIDE §3 audit finding 5).
- `Tooltip` — kept for non-click hover hints (kbd hints, icon-only action hover).

All new shadcn primitives should be added to `components/ui/` via the shadcn CLI if not present.

---

## 10. Scope boundaries

This spec is **NOT**:

- Changing any API calls, hooks, or Zustand store shape. `useConversationsStore`, `VaultAPI`, `useFileContent`, `useDebounce` etc. are untouched.
- Changing conversation persistence, backup, or restore logic.
- Changing the streaming transport (Tauri `llm-stream` event) or tool execution pipeline.
- Changing the markdown rendering pipeline. `TiptapViewer` and `normalizeAssistantMarkdown` stay as-is — we restyle rendered output via CSS, not change what gets rendered.
- Touching any file outside `components/Chat/` unless it is a direct dependency that must be updated:
  - `components/ui/*` — adding new shadcn primitives if needed.
  - `components/TiptapEditor/tiptap.css` — prose output styling (AFFECTED: this is where the serif-in-prose rule, code-block styling, link styling will live).
- Implementing a tool-call data model (Option B in §3.5). Current in-prose rendering continues.
- Adding attachments or drag-drop (§4.5).
- Adding slash-command / cmdk in-message commands (§4.6).
- Virtualizing long threads (§10 — flagged as risk, not addressed).
- Rewriting `ConversationSidebar.tsx` as separate components. That split is tracked as tech debt; this pass restyles in place.

---

## 11. Risks and unknowns

1. **Serif font loading.** `Source Serif 4` is not currently loaded in the app (flagged in TOKENS-SPEC §9.2). Assistant prose will fall back to Charter/Georgia until the font loads — acceptable but not the designed default. Implementation must add the font via `@font-face` or Google Fonts link **as part of this pass**, otherwise the core editorial-prose principle (§3.3) degrades on first paint.

2. **Tool-call rendering.** §3.5 Option A defers building a real tool-call component. If the product direction is to make tool usage more visible (a recurring request), this pass will be judged insufficient. We flag now that Option B (new `ToolCallBlock.tsx` with message-schema changes) is significant follow-up work and likely a second spec.

3. **Long-thread virtualization.** `react-virtual` is installed but not wired into Chat. Threads >200 messages are rare but not impossible. This pass does not virtualize. If performance is poor after restyle, we add virtualization in a targeted follow-up (risk: shadcn `ScrollArea` does not directly compose with `react-virtual`; may need a custom scroll container).

4. **Model picker placement.** The spec tucks model picker inside the composer controls popover (§4.4). If users need to see the current model at a glance during long sessions (a legitimate concern — model choice affects quality), we could surface it as a persistent `text-xxs` label just above the composer instead. Decision point for Josh during review.

5. **`ConversationSidebar.tsx` at 2,278 lines.** Restyling in place (per §9) means editing a file of that size without regressions. Test plan should include manual verification of spaces picker, filters, selection mode, bulk move, journals, and snippets to confirm no visual regressions outside the redesigned surface. File split remains tech debt.

---

## 12. Tokens and patterns needed beyond TOKENS-SPEC

TOKENS-SPEC is sufficient for every decision in this spec **except** the following. These are items implementation may need to resolve as small additions or conventions, not full token additions:

- **Code-block syntax-highlight palette** — TOKENS-SPEC §10 explicitly defers this. For this pass, use a monochrome mapping: keyword/accent-green/accent-purple collapse to `--accent`; comment/literal collapse to `--text-muted` and `--text-secondary`; identifier stays `--text-primary`. If the result is too flat for code readability, component-designer owns adding a small `--code-*` semantic group. Do not invent without review.

- **Space accent dot** — the `8×8` space-identity dot (§5.4) uses a raw user-chosen hex. This is acceptable because space color is user data, not chrome — same principle as folder tags in Finder. No new token.

- **`--overlay` for in-panel dialogs** — currently `--overlay` (TOKENS-SPEC §1.6) is scoped to modal scrim. The spaces-management panel (§5.6) reuses it for a half-opacity scrim over the sidebar region. Should work; flag if opacity feels wrong in context.

- **Streaming cursor color** — `--text-muted` is used. If it disappears against `--bg` at 3.4:1 contrast, bump to `--text-tertiary` (5.3:1). Decide during implementation.

- **Per-message `--accent` left-bar on deep-linked message (§8)** — a 2px `--accent` bar that persists for 1.5s then fades. Same token, new usage pattern. No new token.

All other visual decisions resolve cleanly against TOKENS-SPEC.
