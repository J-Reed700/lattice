# Chat — Pixel-Level Polish Audit

**Status:** punch-list
**Audit date:** 2026-04-17
**Scope:** `src/app/websrc/components/Chat/**` + `components/TiptapEditor/tiptap.css`
**Against:** `AESTHETIC-GUIDE.md`, `TOKENS-SPEC.md`, `CHAT-REDESIGN-SPEC.md`

The redesign landed the macro moves correctly (no bubbles, reading column, accent-only, flat surfaces). What's incomplete is everything *inside* those moves. Grouped by category, ordered within each by impact.

---

## The five fixes that will move the needle most

These are the five changes that, done first, will take the surface from "correct but unfinished" to "considered."

1. **STRUCTURAL — `tiptap.css` is the old file.** Assistant prose is being styled by a stylesheet that uses `rgba(255,255,255,0.8)` paragraph text, `#60a5fa` links (sky-400, not `--accent`), `#93c5fd` inline code (blue-300), a `4px rgba(59,130,246,0.5)` blockquote border, and Tailwind's `@tailwindcss/typography` plugin isn't even installed — the `prose prose-invert prose-sm` classes on `Message.tsx:244` are dead strings. Every assistant response is rendering against 2024-era sky-blue prose CSS. Fix: full rewrite per §3 below.
2. **POLISH — All empty states use the same iconography with no emotional weight.** Three different empty states (`ChatPanel.tsx:406`, `ChatPanel.tsx:436`, `ConversationSidebar.tsx:1611`) all render `MessageSquare` at `h-8 w-8` with near-identical copy structure. The chat panel's "No conversation selected" is a developer placeholder masquerading as a designed state. See §4.
3. **POLISH — Composer Send button is `h-8 w-8` (32px); icon inside is `h-4 w-4` (16px).** Spec §4.3 says 32×32 button, which is right — but the lucide Send glyph at 16px with default `stroke-width:2` inside a 32px violet button reads as bold and cartoonish on a dark surface. Drop the icon to `h-3.5 w-3.5` (14px) and set `strokeWidth={1.75}`. See §2.
4. **POLISH — Reading column padding is uneven.** Messages get `px-6 py-8` (24/32) via `Message.tsx:184`. Composer gets `px-6 py-4` (24/16) via `ChatPanel.tsx:465`. Sources-in-conversation footer is inside `px-6` at `ChatPanel.tsx:457`. But the 760px column is centered inside a panel that itself has no horizontal padding (`ChatPanel.tsx:432`), so on narrow viewports the content hugs the edge. Per §2.2 of spec, column padding is `24px` L/R; the outer panel needs to ensure that padding is always reachable. See §1.
5. **POLISH — Every chrome icon is 16px (`h-4 w-4`).** Linear and Notion run chrome icons at 14px (`h-3.5 w-3.5`), and the default lucide 2px stroke at 14px looks tight and precise; at 16px it looks heavy. 82 of these across Chat — the cumulative effect is the whole surface reading 1pt bolder than it should. Spec §12 defers stroke weight to component-designer, but the call is now: chrome icons = 14px, `strokeWidth={1.75}`. Buttons-with-labels can keep 14px; decorative empty-state icons should be 20px (`h-5 w-5`) not 32px. See §2.

---

## 1. Spacing & rhythm

### Polish

- **`Message.tsx:184` — message row is `px-6 py-8` (24/32).** Spec §2.2 calls for `24px` horizontal and `32px` vertical between messages, but the 32px is on both top *and* bottom, so messages are separated by 64px of dead space. Change to `px-6 pt-8 pb-6` so the visual rhythm matches the spec's "gap between messages, not padding around them" intent. The hairline top border already carries the separation.
- **`ChatPanel.tsx:465` — composer has `px-6 py-4` (24/16).** Spec §4.1 calls for `24px` horizontal / `16px` vertical — this is correct. **But** the textarea inside at `ChatPanel.tsx:476` is `px-4 py-3 pl-11 pr-11`, so effective inner padding left/right is 44px (to clear the 24px icons at `left-3` + icon width + gap). That reads as spacious but wastes horizontal. Either move the Settings2 icon below the textarea on its own row, or drop the textarea left-padding to `pl-10` (40px) and tighten the Settings2 position to `left-2.5`.
- **`ChatPanel.tsx:542` — kbd hint row has `mt-2` (8px) below the composer box.** At 16px vertical outer padding (`py-4`), this leaves only 8px between composer border and kbd line — cramped. Spec §4.4 says the hint is right-aligned at `text-xs` `--text-muted`, implies breathing room. Change to `mt-3` (12px).
- **`ConversationSidebar.tsx:1678` — conversation rows are `px-4 py-2` (16/8).** At 48px target height per spec §5.2 "Height: 48px minimum," current actual height is `py-2` (8px×2 = 16px padding) + content (~32px for two lines) = 48px, but that's the *maximum* with subtitle. Single-line conversations (many don't have `lastMessagePreview`) are ~40px tall, making the list look ragged. Add `min-h-[48px]` to the row. Also, `px-4` (16px) on a 280px-wide sidebar eats 11% of horizontal space — drop to `px-3` (12px) to match Linear's sidebar density.
- **`ConversationSidebar.tsx:1171` — top section has `p-4` (16px all sides).** Inside this section: New Conversation button, Space Scope card, Notebook card (conditional), filter chips, Search input, Select-multiple row. That's 6 stacked regions in 16px padding with `mt-3` (12px) between each. Total vertical height before conversation list starts: ~280px. Sidebar is 100vh minus top rail (48px). On a 900px viewport, the list gets 572px — tight. Either collapse Space Scope + filters into a single inline row, or move filter chips below the search input and make them scrollable horizontally.
- **`Message.tsx:265` — verification panel has `p-4` (16px).** Inside it, `mb-3` (12px) on the chip row, `gap-3` (12px) on the claim grid, `space-y-1.5` (6px) between claim items, `px-3 py-1.5` (12/6) on each claim. That's five different paddings in one panel. Unify: panel `p-5`, chip row `mb-4`, grid `gap-4`, items `space-y-2`, each item `px-3 py-2`. Consistent 4px multiples.
- **`SourceCitations.tsx:240` — source list expand button has `mt-4` (16px).** The button is a single line of `text-sm` `text-tertiary` after the prose — but below it on expand, `ul.mt-3 space-y-4` begins. So the gap pattern is 16px → chevron → 12px → list items → 16px between items. That's three different gaps in quick succession. Unify: the row itself takes `mt-6` (24px, separates from prose), and inside the expanded list use `mt-4 space-y-3`.
- **`CitationFootnote.tsx:38` — inline `[N]` marker has `px-0.5` (2px).** On either side of a compact superscript, 2px is right, but vertical alignment is inherited from `<sup>` default (baseline-raise). Result: markers sit slightly above the prose line. Add `align-baseline` to the className to force baseline alignment (the existing `align-baseline` in Message.tsx:253 is on the cursor, not here).
- **`ConversationSpotlight.tsx:237, 276` — spotlight result rows are `px-4 py-3` (16/12).** Spec §6 uses sidebar row pattern (§5.2 = `px-4 py-2`). Unify to match the sidebar — `px-4 py-2.5` — so spotlight doesn't feel like a different component.

### Craft

- **No breathing room around the active-conversation accent bar.** `ConversationSidebar.tsx:1685-1688` places the 2px `--accent` bar at `inset-y-0 left-0`. It runs flush to the row's top and bottom edges. This is correct per spec §5.2, but inside a 48px row with `py-2`, the bar spans the full 48px — reads as a divider, not a marker. Either inset it vertically (`top-1 bottom-1`) so it reads as a highlight, or accept full-height but pair with a more saturated surface tone (`bg-surface-raised` is already applied at `:1680`, but on top of sidebar's `bg-surface`, the delta is only ~4% L — nearly invisible).
- **Inside-message spacing between citation footnotes row and source citations list.** `Message.tsx:162` citation footnotes uses `mt-2 flex flex-wrap gap-1`; then `SourceCitations.tsx:240` block starts with `mt-4`. Total gap is 6px (mt-2 below prose) + footnote line (16px) + mt-4 (16px) = ~38px of mixed spacing before "3 cited sources" appears. Consolidate: citation footnotes `mt-3`, source citations `mt-6`.

---

## 2. Icons & iconography

### Polish (top of impact list)

- **`ChatPanel.tsx:538` — Send icon is `Send h-4 w-4`.** 16px glyph inside 32px violet button with default stroke-width 2. At 16px/2px that's 12.5% stroke ratio — reads heavy. Change to `h-3.5 w-3.5` (14px) with `strokeWidth={1.75}`.
- **`ChatPanel.tsx:528` — Stop icon `Square h-4 w-4`.** Same issue. Use `h-3.5 w-3.5 strokeWidth={1.75}`.
- **`ChatPanel.tsx:420` — empty-state `Plus` is `h-4 w-4` inside a 40px button.** Correct size for button-with-label, but the button is large (`px-4 py-2 text-sm`); icon fills it well enough. Keep.
- **`ChatPanel.tsx:493` — composer Settings2 is `h-4 w-4` inside `h-6 w-6` (24px) wrapper.** 16px glyph in 24px box is 66% — cramped. Either shrink glyph to `h-3.5 w-3.5` and set `strokeWidth={1.75}`, or expand wrapper to `h-7 w-7`.
- **`ChatPanel.tsx:406`, `ChatPanel.tsx:436`, `ConversationSidebar.tsx:1611` — empty-state MessageSquare is `h-8 w-8` (32px).** At stroke-width 2, 32px icons feel illustrative. Spec §7 just says "small `MessageSquare` icon at `32px`" — but at the dark-mode `--text-muted` color (3.4:1 contrast), a 32px/2px stroke looks weak *and* bold at the same time. Drop to `h-7 w-7` (28px) with `strokeWidth={1.5}` — or elevate to a properly-designed empty-state illustration (see §4).
- **`ConversationSidebar.tsx:1716-1718` — row icons (`NotebookPen`, `MessageSquare`) are `w-3.5 h-3.5` (14px).** Good size, but `text-[hsl(var(--text-tertiary))]` at 5.3:1 contrast paired with default `strokeWidth=2` at 14px is visually heavy — at that size, stroke should be 1.5. Add `strokeWidth={1.5}` to all decorative row icons.
- **`ConversationSidebar.tsx:1820, 1836, 1852, 1868, 1881, 1884, 1897` — hover-revealed row action icons are `w-3.5 h-3.5` (14px).** These are 7 icons per row that only appear on hover. At 14px they're the right size, but the cluster feels dense — consider grouping them in a right-aligned overflow `⋯` icon that opens a mini-menu on click (Linear pattern). Structural change; out of scope for polish pass.
- **`MessageActions.tsx:40, 45, 61, 72` — message action icons are `h-3 w-3` (12px).** 12px is too small for default lucide stroke-width 2 — glyphs render muddy. Bump to `h-3.5 w-3.5` (14px) with `strokeWidth={1.75}`.
- **`CitationFootnote.tsx:38` — no icon, but the `[1]` uses `font-mono text-xs` which is 12px.** Font-mono at 12px on dark `--text-muted` color is legible but gets lost inline in serif prose. Bump to `text-xxs` (11px) — smaller but denser — with `font-weight: 500` for legibility. Or keep xs and add `tabular-nums` for consistent width across digits.
- **`ComposerControls.tsx:169` — tool-row icons are `h-3 w-3` (12px).** Too small; same fix as MessageActions — `h-3.5 w-3.5 strokeWidth={1.75}`. Also currently at `text-tertiary` (5.3:1) — paired with the checkbox icon at the same size to the left, the row feels icon-soup. The icons here add no information (each row is labeled); remove them entirely and rely on the label.
- **`ConversationLinkedDocumentsPanel.tsx:276-279, 328, 337, 428, 432, 443, 458` — mix of 14px chevrons and 12px (`h-3 w-3`) buttons icons.** Standardize to 14px with `strokeWidth={1.75}`.
- **`FilePreviewModal.tsx:501, 517, 532, 541` — explicit `size={20}` for close, `size={16}` for footer actions.** Using `size={}` prop instead of className is inconsistent with the rest of the Chat area (which uses Tailwind classes). Not a visual bug — but the 20px close glyph in a `p-2` button (32px total) reads oversized relative to the rest of the surface. Drop to `size={16}`.
- **`FilePreviewModal.tsx:306` — inline SVG for "File Too Large" state uses `strokeWidth={2}`** at `w-16 h-16` (64px). A 64px icon at 2px stroke reads as a marketing illustration. If we keep the raw SVG, drop stroke to `1.5`. Better: replace with `AlertTriangle` lucide icon at `h-8 w-8` with `strokeWidth={1.5}`, matching the empty-state pattern.

### Craft

- **Stroke-weight inconsistency across the whole surface.** Default lucide is 2px. Our design language is "quiet" (AESTHETIC-GUIDE §4). Every icon in Chat should be inspected and globally set: 14px icons = `strokeWidth={1.5}`; 16-20px icons = `strokeWidth={1.75}`; 28px+ empty-state icons = `strokeWidth={1.5}`. Zero icons should inherit default `strokeWidth={2}` in the Chat surface.

---

## 3. Prose (tiptap.css) — STRUCTURAL

This file is the single largest source of "incomplete feeling." It predates the token system and was never rewritten. Every assistant message inherits it.

- **`tiptap.css:37-44` — inline code is `background: rgba(255, 255, 255, 0.1); color: #93c5fd;`.** That's `blue-300` text on a white-alpha background. Spec §3.3 says `--surface` bg, `--border-subtle` 1px border, `font-size: 0.9em`, `--radius-sm`. Current is completely wrong.
- **`tiptap.css:101-107` — blockquote is `border-left: 4px solid rgba(59, 130, 246, 0.5)` (sky-blue, half alpha).** Spec §3.3: `3px` left-border `--border-strong`, `16px` padding, `--text-secondary` italic. Current border weight is 4px (wrong), color is sky-blue (wrong), padding is `1rem` (ok), color is `rgba(255,255,255,0.6)` not `--text-secondary`.
- **`tiptap.css:186-195` — links are `color: #60a5fa` (sky-400) hover `#93c5fd` (sky-300).** Spec §3.3: `--accent`, `text-decoration: underline`, `text-decoration-thickness: 1px`, `text-underline-offset: 2px`, hover `--accent-hover`. Current has wrong color (sky, not violet), no underline-thickness control, no offset.
- **`tiptap.css:110-132` — headings h1/h2/h3 use `font-weight: 700` and `rgba(255,255,255,0.9)`.** Spec §3.3: `--font-serif`, weight 600 (not 700 — TOKENS-SPEC §3.3 explicitly prohibits 700+), size tokens `text-xl` / `text-lg`, `--text-primary`. Current uses raw rem values (1.5, 1.25, 1.125) that don't match the token scale, wrong weight, wrong color, wrong family.
- **`tiptap.css:135-140` — paragraphs are `color: rgba(255,255,255,0.8)` line-height `1.625`.** Spec §3.3: `--font-serif`, `text-base` (16px), line-height `1.6`, `--text-primary`. Current uses 80% white (not `--text-primary` at 96% L), `line-height: 1.625` instead of `1.6`, no font-family directive (so it inherits from the parent, which is whatever the outer `font-sans`/`font-serif` class set — but see next finding).
- **The `.tiptap-viewer` container gets `font-serif` applied via `Message.tsx:245`** on the outer div, but inside tiptap.css no `font-family: var(--font-serif)` is declared on `.tiptap-viewer p` etc. It *might* inherit correctly from the outer class, but only because Tailwind's `font-serif` uses `font-family` which cascades. Explicit is better: declare `font-family: inherit` on prose elements so that when the outer wrapper specifies sans (user messages), the prose follows.
- **`Message.tsx:244` — className includes `prose prose-invert prose-sm` — these classes are dead strings.** The `@tailwindcss/typography` plugin is not in `tailwind.config.js`. Either install it and adopt the `prose` system with overrides via a `.prose` selector in tiptap.css, *or* remove the dead classes. Recommended: install `@tailwindcss/typography`, configure a custom `prose-recall` variant that maps to the token system, and drop all the loose rules in tiptap.css.
- **`tiptap.css:18-33` — code blocks (`pre`) use `background: rgba(0, 0, 0, 0.4)` and `0.5rem` radius (8px).** Spec §3.3: `--surface` bg, `--border-subtle` 1px border, `--radius-sm` (4px), `12px 16px` padding, `text-sm`. Current bg is a pure-black overlay that won't match light-mode at all (it'll be dark-on-light), border radius is 8px instead of 4px, no border at all, padding is `0.75rem 1rem` (12/16 — this part is right).
- **`tiptap.css:55-70` — tables have `background: rgba(255,255,255,0.05)` headers, `rgba(255,255,255,0.1)` bottom border, padded `0.5rem 1rem`.** Spec doesn't explicitly style tables but the principle (AESTHETIC-GUIDE §2) is "typography carries hierarchy, not borders." Current uses both: row borders *and* background tint on header. Simplify: no header bg, just `border-bottom: 1px solid --border-subtle` on `th` with `font-weight: 500`, body rows get `border-bottom: 1px solid --border-subtle`. Remove all `rgba(...)` literals.
- **`tiptap.css:148-166` — lists use `padding-left: 1.5rem` and `margin: 0.25rem 0`.** Too tight vertically for 1.6 line-height body prose — list items need breathing. Change to `margin: 0.75rem 0`, items `margin-bottom: 0.375rem`. Also bullet marker color is implicit (inherits); spec §3.3 says `--text-tertiary`. Add `&::marker { color: hsl(var(--text-tertiary)); }`.
- **`tiptap.css:169-174` — hr uses `border-top: 1px solid rgba(255,255,255,0.1)`.** Should be `border-top: 1px solid hsl(var(--border-subtle))`, margin `2rem 0` (more generous — horizontal rules are structural pauses in prose).
- **`tiptap.css:211-213` — selection highlight is `rgba(59, 130, 246, 0.3)` (blue-500 at 30%).** Should be `hsl(var(--accent-muted))` or `hsl(var(--accent) / 0.25)` for violet selection.
- **`tiptap.css:198-202` — strong is `color: rgba(255,255,255,0.95)` weight 600.** Color override is wrong — strong should inherit the paragraph's `--text-primary` (which is already 96% L in dark mode, i.e. brighter than 0.95). Remove the color override; keep `font-weight: 600`.
- **No syntax highlighting palette defined.** Spec §3.3 calls for monochrome-plus-accent: keyword = `--accent`, comment = `--text-muted`, string = `--text-secondary`, identifier = `--text-primary`. CodeBlockLowlight is imported in `TiptapEditor/extensions/index.ts:2` but the `.hljs-*` classes produced by lowlight have no CSS attached anywhere — syntax highlighting is either using the lowlight default (rainbow) or falling through to inherited color. Add a `.hljs-keyword`, `.hljs-comment`, `.hljs-string`, etc. block in tiptap.css mapped to the token system.
- **Placeholder `tiptap.css:9-15` — uses `var(--text-tertiary, rgba(255,255,255,0.35))` with a 35% white fallback.** The fallback is wrong (35% is barely visible), and the raw CSS var references predate the HSL system. Should be `color: hsl(var(--text-muted));`.
- **`tiptap.css:94-97` — task list checkbox uses `accent-color: var(--accent-primary, #3b82f6)`.** `--accent-primary` doesn't exist in the token system (it was deleted per TOKENS-SPEC §9.1). Fallback blue-500 is what's actually rendering. Change to `accent-color: hsl(var(--accent));`.

**Recommended action:** full rewrite of `tiptap.css` against TOKENS-SPEC + CHAT-REDESIGN-SPEC §3.3. This is the biggest single-file structural work item and the biggest "complete feeling" win.

---

## 4. Empty states

### Craft

- **`ChatPanel.tsx:402-426` — "No conversation selected" state.** Currently: centered `MessageSquare` 32px, h2 "No conversation selected" (serif, text-lg), p "Pick a conversation from the sidebar, or start a new one." (text-sm text-tertiary), button "New Conversation". Reads as functional but has zero emotional weight. Spec §7 says "Small `MessageSquare` icon at `32px` `--text-muted`. Heading `text-lg` `--font-serif`" — which is what we have, but it's a boilerplate state, not a considered one. Craft opportunity: add a secondary text line below the button like `"Or press ⌘N to start fresh · ⌘K to search your history"` at `text-xs` `--text-muted` with kbd chips for `⌘N` / `⌘K`. Introduces the keyboard path (AESTHETIC-GUIDE §2.7) as native behavior, not a setting.
- **`ChatPanel.tsx:434-443` — "This conversation is empty" state.** Same structure as above, different copy. Add a suggestion row with 2-3 example starter prompts as clickable chips at `text-xs` `--text-tertiary`. E.g. `"Summarize today's notes"`, `"What did I write about X?"`, `"Help me think through..."`. Each chip clicks to populate the composer. This is the difference between "empty room" and "ready workspace."
- **`ConversationSidebar.tsx:1609-1614` — "No conversations yet" sidebar state.** Correct per spec §5.5. But the icon at `w-8 h-8` in a 280px-wide column looks oversized. Drop to `w-6 h-6` (24px) and tighten vertical `mb-2 → mb-3`. Second text line "Start a new conversation above." points to the button 100px up — consider changing to "Press the + above or ⌘N" to introduce the keyboard.
- **`ConversationSpotlight.tsx:217-221` — spotlight "No results" state.** Currently: `Search` icon 16px, "No results" text-sm. Zero context. Should include what was searched for (`"No results for '<query>'"` at `text-sm` `--text-tertiary`) and a hint (`"Try a different term, or create a new conversation"` at `text-xs` `--text-muted`). Empty state with zero context is the weakest UI moment — craft opportunity.
- **Missing: "No messages match role filter" state.** `ConversationSidebar.tsx:1490-1495` has a text-only line "No references match the selected role filter." Should be the same pattern as other empty states — icon, heading, body — so the layout doesn't collapse. Currently it reads as an error message, not a state.
- **Missing: "Sidebar is loading" vs. "Sidebar has no conversations ever" distinction.** The `isLoading` branch at `ConversationSidebar.tsx:1605-1608` renders a spinner; the empty branch at `:1609` renders the same empty copy whether the user has zero conversations (first-run) or the current filter has no matches. First-run should say "Create your first conversation" with stronger emphasis than "No conversations yet." Filter-zero-match should say "No conversations match the current filter" with a "Clear filter" action. Currently conflated.

---

## 5. Typography

### Polish

- **`Message.tsx:245` — assistant prose wrapper uses `prose prose-invert prose-sm` classes.** These are plugin classes with no plugin installed (see §3). `prose-sm` sizing target is 14px — but the actual rendered size comes from inherited `text-base` (16px) from the body. Remove the dead classes; replace with explicit `text-base leading-[1.6]` for assistant and `text-base leading-[1.5]` for user.
- **`Message.tsx:189` — "You"/"Assistant" label is `text-xxs uppercase tracking-[0.08em]` `font-medium` at `--text-tertiary` (5.3:1).** Correct per spec §3.1. But visually, a medium-weight uppercase label next to the timestamp at the same color makes the two blur. The timestamp at `Message.tsx:237` is `text-xs` at `--text-muted` (3.4:1). Spec §3.2 says model name at muted, timestamp at muted — that's consistent. But visually, the label and the timestamp both feel like "sub-content." Resolution: make the label `--text-secondary` (9.1:1) so "You" / "Assistant" reads as a register-header, not metadata.
- **`ConversationSidebar.tsx:1632-1634` — group headings ("Today", "Yesterday") are `text-xxs uppercase tracking-[0.08em]` at `--text-muted`.** Correct per spec §5.3. But on a narrow sidebar (280px), uppercase text at `--text-muted` (3.4:1) reads as decoration, not a label. Either bump color to `--text-tertiary` (5.3:1) or drop uppercase in favor of sentence case + weight 500 at `--text-tertiary`. Linear uses lowercase group labels.
- **`ConversationSidebar.tsx:1627` — journal group label is `text-xs italic font-serif text-tertiary`.** Spec §5.3 explicitly calls out this differentiation — journal uses serif italic. Good. But at `px-2 py-1` the label sits flush with the conversation list; add `mt-4` so groups visually separate.
- **`Message.tsx:189-190` — label "You" / "Assistant" — no model name shown.** Spec §3.2 says "Model name (for assistant, e.g. `Assistant · claude-opus-4.7`) — `--text-muted`, `text-xs`, interpunct between label and model." Currently we only show "Assistant" — model name is missing. If `message.model` is available in the DTO, render it inline: `Assistant · <span class="text-xs text-muted">{model}</span>`.
- **`ChatPanel.tsx:407` and `:437` — empty-state headings are `text-lg font-semibold font-serif`.** Spec §7 says `text-lg font-serif` (weight not specified beyond the default). `font-semibold` = 600, which matches the spec's implicit heading weight for `text-lg` in TOKENS-SPEC §3.2 (weight 500 default for `text-lg`). Contradiction: TOKENS-SPEC sets `text-lg` default to `font-weight: 500`; chat empty state adds `font-semibold` (600). Pick one. Spec §7 implies 500 is sufficient; drop `font-semibold`.
- **No `text-2xl` or `text-3xl` usage in Chat anywhere.** Neither spotlight search nor any header uses `text-xl`+. Spec §7 and §3.1 don't require this — but the absence of any heading size larger than `text-lg` (18px) makes the whole surface read one-note. `FilePreviewModal.tsx:487` uses `text-2xl` (24px) for the file title — this is the only large-type moment in the entire Chat surface. Consider: the empty-state heading in ChatPanel (`:407`) could bump to `text-2xl` to introduce actual hierarchy. Keep `text-lg` for the secondary empty-state ("This conversation is empty").
- **`MessageActions.tsx:31` — actions row is `text-xs` (12px).** Correct for meta, but at `--text-muted` (3.4:1) the row is nearly invisible until hover. Spec §3.8 explicitly says "low contrast and full opacity always." `--text-muted` hits the spec; but consider `--text-tertiary` (5.3:1) for better affordance discoverability — the spec calls out that hover-hidden fails power users; low-visible risks the same.
- **`ConversationSidebar.tsx:1158` — sidebar wordmark "Recall" is `text-base font-semibold font-serif`.** Serif semibold at 16px is a reasonable chrome mark, but spec §5.1 says "Recall logo/wordmark at `text-sm` `--font-serif` `font-semibold`." Current is `text-base` — bigger than spec. Drop to `text-sm` (14px).
- **`ChatPanel.tsx:543` — kbd chips inline "⌘⏎" are `font-mono` with no font-size specified.** They inherit `text-xs` (12px) from the parent. Should be `text-xxs` (11px) per the kbd chip pattern in `ConversationSpotlight.tsx:207-209`. Unify.
- **`ChatPanel.tsx:543` — the kbd hint row uses `<kbd className="font-mono">⌘⏎</kbd>` — no border, no bg.** Spec §6 (spotlight kbd) requires `--border-default` 1px, `--radius-sm`, `4px 6px` padding. The composer kbd is bare while the spotlight kbd is styled. Unify: all kbd chips get the bordered-chip pattern. Current composer kbd renders as plain monospace text.

### Craft

- **Weight variation is almost nonexistent.** Per TOKENS-SPEC §3.3, weights are 400/500/600. Grep across Chat for `font-semibold`, `font-medium`, `font-normal`: semibold appears ~5 times (empty states, source filenames), medium appears ~12 times (active states), and the rest is default (400). The effect: most of the surface is uniform weight, so hierarchy defaults to color/size alone. Per AESTHETIC-GUIDE §6: "Make it smaller. Our instinct to enlarge is usually wrong; reach for weight or spacing first." The current Chat surface isn't reaching for weight. Specific opportunities: conversation row titles when active (`ConversationSidebar.tsx:1776-1781`) should be weight 500 not just color-switched; source filenames in `SourceCitations.tsx:275` are already `font-semibold` (600) — consider dropping to 500 for less emphasis; empty-state headings drop from 600 to 500.
- **Letter-spacing is applied inconsistently.** `tracking-[0.08em]` appears 18 times (uppercase labels, correct). But body prose inherits the token-scale tracking from the font-size config (`tailwind.config.js:65-73`), which is correct. However, the composer textarea at `ChatPanel.tsx:476` doesn't have explicit tracking — it inherits default. Messages render in serif at default tracking, which Source Serif 4 at `text-base` is tuned to 0 — correct. No action, but worth confirming Source Serif 4 is loaded (it's flagged as a risk in spec §11.1).

---

## 6. Micro-details

### Polish

- **`ChatPanel.tsx:466` — composer focus uses `focus-within:ring-2 focus-within:ring-[hsl(var(--ring))]`.** This renders as a `box-shadow` ring at 2px around the composer box. Per TOKENS-SPEC §8, focus is a `2px solid outline` with `2px offset`, not a box-shadow ring. The composer is an exception where outline clips — spec §4.2 allows `box-shadow: 0 0 0 2px hsl(var(--ring))`. But current uses Tailwind's `ring-2` which is `box-shadow: 0 0 0 calc(2px + var(--tw-ring-offset-width)) var(--tw-ring-color)` — offset-adjusted. Works, but consider: when composer is focused, the ring appears *inside* the border-default border (ring at `0 0 0 2px`, border at `1px`). Result: 1px border + 2px ring sit on top of each other, creating a 3px stripe effect. Drop the `border-default` on focus-within (e.g. `focus-within:border-transparent focus-within:ring-2`) so the ring is the visual border during focus.
- **`ConversationSidebar.tsx:1269` — search input uses `focus:ring-2 focus:ring-[hsl(var(--ring))]`.** Same double-border issue — `border border-default` + `focus:ring-2`. Use `focus:border-transparent` at the same time.
- **All `disabled:opacity-50` usage — lazy per spec §4.3.** Counted across Chat: `opacity-50` appears 8+ times as a disabled state (`ChatPanel.tsx:418, 533`, `ConversationSidebar.tsx:1176, 2021, 2256`, `FilePreviewModal.tsx`). TOKENS-SPEC §1.3 specifies `--text-disabled` as the intended treatment — `opacity-50` doesn't distinguish disabled-button from disabled-text from disabled-icon. Replace with explicit `disabled:bg-[hsl(var(--border-default))] disabled:text-[hsl(var(--text-disabled))]` where it matters.
- **Hover states are "goes brighter" only.** Every button in Chat does `hover:text-[hsl(var(--text-primary))]` or `hover:text-[hsl(var(--text-secondary))]` — one-color-step brighten. Linear/Arc use a paired shift: background subtle tint + text brighten. Example: sidebar row hover at `ConversationSidebar.tsx:1681` is `hover:bg-surface-raised` only. On a `bg-surface` row, the surface-to-surface-raised delta is 4% L — barely perceptible. Either strengthen the bg delta (use `hover:bg-[hsl(var(--accent-muted))]` for selected-like preview — but this conflicts with accent discipline), or add text brighten: `group-hover:text-[hsl(var(--text-primary))]` on the title.
- **Scrollbars are default OS style in the thread.** `ChatPanel.tsx:431` has `overflow-y-auto` with no scrollbar styling. Same for sidebar at `ConversationSidebar.tsx:1425` (`overflow-y-auto [scrollbar-gutter:stable]` — gutter-stable is good, but scrollbar itself is unstyled). `styles/chat.css:14-35` has styled scrollbars but is not imported anywhere. Add a global thin scrollbar style targeting the chat thread + sidebar using `--border-default` thumb, transparent track. Subtle and consistent.
- **`index.css:211-213` — `::selection` (Tiptap viewer) is `rgba(59, 130, 246, 0.3)`** — but this rule is inside `tiptap.css:211`, not `index.css`. Either way, selection color is blue-500 at 30%, not violet. Add a global `::selection { background: hsl(var(--accent) / 0.25); color: hsl(var(--text-primary)); }` in `index.css`.
- **`ConversationSidebar.tsx:1252` — active filter chip uses `border-b-2 border-accent`.** Correct per spec §5.1.4. But inactive chips use `border-b-2 border-transparent` — sensible to prevent layout shift. However, the `pb-1` at `:1250` clips the chevron icon at 12px from baseline — text descenders in "Bookmarked" etc. sit on the underline when active. Add `pb-1.5` so underline is 2px *below* the descender line.
- **`ChatPanel.tsx:484-494` — composer Settings2 button has no active-state background.** Spec §4.4 says active (popover open) is `--text-secondary` text — currently implemented. But the wrapper has no bg-change on active. Add `bg-surface-raised` when `isControlsOpen` to show the popover is anchored here.
- **`ConversationSidebar.tsx:1197` — Space Scope / Notebook button cluster.** The Notebook button conditionally appears (`:1198-1206`) with no transition. Mount/unmount should fade in over `--duration-fast`. Currently it pops.
- **Placeholder text across the chat surface uses `text-muted`.** `ChatPanel.tsx:476` (composer), `ConversationSidebar.tsx:1269` (sidebar search), `ConversationSpotlight.tsx:205` (spotlight search). Per TOKENS-SPEC §1.3, placeholder should be `--text-muted` (3.4:1). Consistent. Good.
- **`Message.tsx:184` — message row uses `animate-in fade-in-0 duration-fast`.** These are `tailwindcss-animate` plugin classes. They work, but `duration-fast` is a Tailwind class mapped to `--duration-fast` (120ms). Confirm this resolves correctly — in the config `tailwind.config.js:87` it's declared. Good. But every message in the thread fades in on mount, including pre-existing ones when the conversation loads. Per spec §8, "message list on initial load (messages just appear)." Fix: only animate newly-arrived messages, not the initial batch. Requires tracking first-render per message; structural, not polish.
- **`ConversationSpotlight.tsx:241-246, 280-285` — selected-row accent bar `inset-y-0 left-0 w-0.5`.** `w-0.5` in Tailwind is 2px. Correct per spec. But `inset-y-0` means the bar runs the full row height (currently `py-3` = 24px + content ~32px = 56px). A 2px × 56px bar looks like a divider, not a marker. Inset vertically (e.g. `top-2 bottom-2`) so bar height is ~40px — reads as highlight.
- **`ConversationSidebar.tsx:1515-1518, 1685-1688, 2045-2049, 2085-2088` — same `inset-y-0 left-0 w-0.5` issue** in 4 locations. Same fix: inset vertically.

### Craft

- **No visual distinction on hover for already-active rows.** A selected conversation at `:1678-1682` has `bg-surface-raised`; on hover it stays `bg-surface-raised` (the conditional collapses). Active + hover should go one step further — perhaps `bg-[hsl(var(--accent-muted))]` — so the user gets feedback that they're hovering the thing they already have selected.
- **No loading skeleton for the thread.** When switching conversations, the message list blanks until the new conversation's messages load. Spec §7 doesn't address this. Opportunity: flat `--surface` bars at the reading column width mimicking message shapes, at 60% opacity (AESTHETIC-GUIDE §7.3 says "use flat pulse at 60% opacity if needed" — no shimmer).
- **Composer disabled state during streaming.** `ChatPanel.tsx:473` sets `disabled={isSending}` which triggers `disabled:opacity-50` on the textarea. At 50% opacity the placeholder text becomes invisible. Better: keep textarea at full opacity but change placeholder to "Generating response…" and add a `cursor: wait` treatment.

---

## 7. Information density & structure

### Craft

- **Sidebar top section stacks 6 elements before conversations show.** Per the review: top rail (48px) + New Conversation button (40px) + Space Scope card (52px) + Notebook Mode card (conditional, 52px) + filter chips (28px) + Search input (40px) + Select-multiple row (28px) + conditional bulk-move panel. On a 900px viewport, conversations start at ~290px from the top. Linear's sidebar starts conversations at ~110px. **Structural opportunity:** move Select-multiple into an overflow menu on the New Conversation button (`"··· Select multiple"` item); merge Space Scope + filter chips into a single inline row; drop the Notebook card (it's redundant with the Space Scope row showing "· Journal").
- **Composer defaults to no visible info.** Current: only placeholder + kbd hint. Spec §4.4 explicitly removes the "active-flags row" and keeps only kbd hint. But if the user has enabled Web + KB + Deep Research, there's no visible signal the composer is in a different state. Craft opportunity: a single-line dim status above the textarea: `"Web · KB · Deep research enabled"` at `text-xxs` `--text-muted`. Spec §4.4 chose to remove this; user feedback may invalidate that choice. Flag for review.
- **Citation popover shows filename + category + excerpt + "View source" (`CitationFootnote.tsx:51-70`).** Right amount of detail. But filename is at `text-sm font-semibold` and category is at `text-xs text-muted`. The filename often wraps in the 240px-max-width popover. Consider: truncate filename to 40 chars with ellipsis, show `title=` attribute for full name on hover.

---

## 8. Dead code / stylesheet issues

### Polish

- **`src/app/websrc/styles/chat.css` is not imported anywhere** (grep: no matches). 223 lines of legacy styles: `.gradient-button`, `.avatar-gradient-user`, `.empty-state-icon` with box-shadow blue-glow, `@keyframes bounce` (explicitly banned per AESTHETIC-GUIDE §3 no bounce), `.typing-dot` with 10px circles, `.message-shadow` with hover-lift. Should be deleted entirely.
- **`tiptap.css` imports happen via `TiptapViewer.tsx:8` and `TiptapEditor.tsx:9`.** Both pull the same file. A single source of rewrite covers both.

---

## Summary counts

| Category | Polish (5-min each) | Craft (30-60 min each) | Structural (hours) |
|---|---|---|---|
| Spacing & rhythm | 9 | 2 | 0 |
| Icons | 13 | 1 | 0 |
| Prose (tiptap.css) | 2 | 0 | **1** (full rewrite) |
| Empty states | 0 | 5 | 0 |
| Typography | 10 | 2 | 0 |
| Micro-details | 14 | 3 | 0 |
| Info density | 0 | 3 | 0 |
| Dead code | 2 | 0 | 0 |

**Totals:** 50 polish · 16 craft · 1 structural

The structural item (tiptap.css rewrite) is the load-bearing piece — it affects every assistant message. Until it's done, polish on the chat chrome will always be framed by a prose surface that's wearing 2024 blue-and-white-alpha clothes.

---

## One thing that surprised me

**The Popover + Radix primitives are done correctly.** `CitationFootnote.tsx`, the composer controls `Popover.Content` in `ChatPanel.tsx:496-516`, the `ConversationSpotlight` dialog, and the `FilePreviewModal` dialog all use Radix with proper `data-[state=open]:animate-in` patterns, correct token-referenced colors, and the scale-in animation that spec §8 calls for. This is the least-broken layer of the redesign — I expected to find `backdrop-blur` leakage here and didn't. The surprise-in-the-other-direction: I expected `@tailwindcss/typography` to be installed and the `prose` classes to be doing real work. They're completely inert. Every assistant message has been rendering with dead classes and ancient blue-on-white-alpha CSS. The redesign's "editorial prose" pillar (§3.3 of the redesign spec) is not shipped — it's just the chrome around it that's been updated.
