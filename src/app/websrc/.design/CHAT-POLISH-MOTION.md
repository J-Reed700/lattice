# Chat — Motion Audit & Refinement Plan

**Status:** audit
**Scope:** motion language of `components/Chat/**` post-redesign.
**Related:** `AESTHETIC-GUIDE.md` §2.5, §6 · `TOKENS-SPEC.md` §7 · `CHAT-REDESIGN-SPEC.md` §8
**Files audited:** `ChatView.tsx`, `ChatPanel.tsx`, `Message.tsx`, `MessageActions.tsx`, `SourceCitations.tsx`, `CitationFootnote.tsx`, `ComposerControls.tsx`, `ConversationSidebar.tsx`, `ConversationSpotlight.tsx`, `ConversationLinkedDocumentsPanel.tsx`, `FilePreviewModal.tsx`, `components/ui/popover.tsx`, `components/ui/Button/Button.tsx`, `index.css`, `tailwind.config.js`

---

## 1. Motion theme diagnosis

**Verdict: partially present, mostly by accident.**

The redesign correctly removed the previous sins (bounce easings, glow pulses, staggered cascades, `animate-pulse` avatars, ring-flash highlights). What has replaced them is a pair of defaults — Tailwind's `transition-colors` + Radix's `tailwindcss-animate` fade/zoom primitives — used consistently enough that the app doesn't feel patchwork, but never tuned. The result is a competent baseline that *misses the finishing motion*:

- Every floating surface (citation popover, composer controls popover, spotlight dialog, file preview dialog) uses the same `data-[state=open]:animate-in fade-in-0 zoom-in-95` incantation, which is visually coherent — but the duration is the `tailwindcss-animate` default (150ms, `ease`), not the project tokens (`--duration-base`, `--ease-out`). So the motion is consistent with itself, but not with the spec, and not with a considered feel.
- `transition-colors` is the universal hover treatment with no duration specified (browser default 0s = instant). Hover on a sidebar row is instant; hover on a message action button is instant; hover on a citation `[1]` is instant. That's *sort of* on-brand (AESTHETIC-GUIDE §6: "If animation is noticed, it's too much") but it reads as missing polish rather than restraint because there's no transition-duration hook at all. A 120ms color ease-out on hover is the difference between "I clicked into this" and "this window just opened."
- The sidebar row has explicit `transition-colors duration-fast`; the sidebar filter chips and spaces panel don't. Same component file, two different treatments. That's a patchwork signal to careful eyes.
- Message entrance uses the only explicit `duration-fast` + tailwind-animate compose in the whole feature (`animate-in fade-in-0 duration-fast` — Message.tsx:184). It's the most deliberate motion moment in the codebase and nobody will ever notice it because new messages land below the viewport at the bottom of the scroll.
- The Send button is dead static on press. No feedback. The one place a user genuinely wants confirmation-of-intent is the one place there is no motion at all.
- The active-conversation left-bar in the sidebar (`absolute inset-y-0 left-0 w-0.5 bg-accent`) is a conditional render — it pops in and out instantly. Linear, Notion, and Arc all slide this element between rows. Static pop is the single loudest "patchwork" tell in the app.

**If every motion moment were tuned to the same token set at the same precision** — 120ms ease-out on color hovers, 180ms ease-out on floating surface entrances, a single considered press-state on buttons — the app would cross the finished-feeling threshold. Right now it's sitting at ~70% of the way there.

---

## 2. Moment-by-moment audit

| # | Moment | Exists? | Current | Spec? | Recommendation |
|---|---|---|---|---|---|
| 1 | Message entrance (new message) | Yes | `animate-in fade-in-0 duration-fast` on each `<article>` (Message.tsx:184). ~120ms fade. | Matches §8 (fade-only, no translateY). | **Keep.** One refinement — when a NEW message arrives at stream start, the fade is correct. When the conversation thread *switches*, this fires on every message in the new thread — a cascade the spec explicitly bans. See P0 item. |
| 2 | Streaming cursor (`▍`) | Yes | `chat-cursor-blink 1s steps(2, end) infinite` (index.css:181, Message.tsx:254). | Matches §3.7 exactly. | **Keep.** Minor: contrast-verify `--text-muted` against `--bg` at 3.4:1 — cursor may disappear in skim reading. Consider `--text-tertiary` (5.3:1). |
| 3 | Streaming text reveal | No animation | Characters just append. No typewriter. | Matches §3.7 ("just-appear, no character cascade"). | **Keep.** Correct. |
| 4 | Tool call expand/collapse | N/A | Not implemented — spec §3.5 Option A defers. | Out of scope. | **Defer.** When built, use height transition `--duration-base` `--ease-out` per §3.5. |
| 5 | Citation footnote → popover open | Yes | `data-[state=open]:animate-in fade-in-0 zoom-in-95` (CitationFootnote.tsx:49). `tailwindcss-animate` defaults: 150ms, `ease`. | Close to §8 (fade + scale 0.98→1, `--duration-base` `--ease-out`). | **Tune.** Add `duration-base` utility to the className so it binds to the token. Spec says scale 0.98→1 but Radix default is 0.95→1; adjusting requires `animation-duration` override or wrapping in framer-motion. Defensible to leave zoom as 95; insist on duration + easing tokens. |
| 6 | Citation popover close | Yes | `data-[state=closed]:animate-out fade-out-0 zoom-out-95`. Same 150ms `ease`. | Spec implies `--ease-in` for exits (§7.2), but not explicit for popovers. | **Tune.** Apply `--duration-fast` on exit (shorter than open) + `--ease-in` for deliberate dismissal feel. Linear's popovers close faster than they open — 120ms fade-only is enough. |
| 7 | Source citations expand/collapse (end-of-message) | No animation | `{isExpanded && <ul>...</ul>}` conditional render. Pops. (SourceCitations.tsx:255). | §8 says expand/collapse: height transition `--duration-base` `--ease-out`. | **Add.** Height transitions are genuinely hard without layout jumps — either use `grid-template-rows: 0fr → 1fr` trick or Radix `Collapsible`. Today it pops, which is the second-loudest patchwork signal after the active-row accent bar. |
| 8 | Conversation select (thread replacement) | Partial | `scrollTo` with `behavior: isConversationChange ? 'auto' : 'smooth'` (ChatPanel.tsx:172). Messages re-render; each one plays its entrance fade. | §8: messages should NOT animate out on change; new thread fades in together, not per-message. | **Fix (P0).** Suppress the per-message fade on initial mount of a new conversation. Either gate `animate-in` behind a first-render sentinel, or let the thread container fade in as a whole (opacity 0→1, 180ms) while messages inside skip their individual entrance animations. Today: 20-message thread = 20 staggered fades = exactly the cascade §8 bans. |
| 9 | Sidebar item hover (conversation row) | Yes | `transition-colors duration-fast` — background-color only. | Matches §8. | **Keep.** |
| 10 | Sidebar item active (selected) | **Static** | `{isActive && <span className="absolute inset-y-0 left-0 w-0.5 bg-accent" />}` — conditional render, no motion (Sidebar.tsx:1684). | §8 implied instant color change, but this is the highest-leverage motion in the sidebar. | **Add (signature moment).** Use Framer Motion `layoutId="sidebar-active-bar"` shared across all rows so the bar slides vertically between rows. Duration `--duration-base` (180ms) `--ease-out`. This is the single motion moment that makes Linear/Arc/Notion feel alive. Reduced-motion disables. See §3 signature moments. |
| 11 | Sidebar filter chip click (active state) | Yes | `transition-colors` on border-color (Sidebar.tsx:1250). No duration specified → 0ms. | §8 implies instant for filter state. | **Tune.** Add `duration-fast` so the 2px underline-bar fades in 120ms instead of popping. Small detail, adds up. |
| 12 | Composer send (button press + message appears + textarea clears) | **Partial** | Button has `transition-colors` for hover/bg; no press feedback. Textarea clears instantly via `setInput('')`. New message fades in via #1. | Spec §4.3 defines states but not press motion. AESTHETIC-GUIDE §2.5: motion confirms causality. | **Add (P0).** Send button is a causal action with zero feedback. Add a 60ms `active:scale-[0.97]` (or `whileTap={{ scale: 0.97 }}` if promoted to `components/ui/Button`). The submitted message fade-in already handles the "it happened" signal — press state confirms the *click registered* before the network round-trip. Currently: user clicks, nothing happens visibly for 100-400ms until optimistic message renders. That gap kills confidence. |
| 13 | Composer controls popover open/close | Yes | `data-[state=open]:animate-in fade-in-0 zoom-in-95` (ChatPanel.tsx:501). Default 150ms. | Matches §8 pattern. | **Tune.** Same as #5 — bind to `duration-base` utility. Spec calls for `--duration-base` (180ms) but Popover content override classes don't set duration; running at tailwindcss-animate default 150ms. Close enough to read the same, but token-discipline says wire it. |
| 13b | Composer controls popover exit easing | Yes | Default `ease`. | Spec not explicit but §7.2 says `--ease-in` for exits. | **Tune.** Differentiate open (`ease-out`) from close (`ease-in`). |
| 14 | Composer popover checkbox toggle | Partial | `transition-colors` on hover-bg (ComposerControls.tsx:160). Check mark renders via conditional `{checked && <span.../>}` — static pop. | Spec doesn't specify. | **Tune.** Checkmark pop is fine if fast; if added motion is wanted, a 120ms scale 0.8→1 fade-in on the inner dot. Small, optional. |
| 15 | Spaces panel reveal (sidebar "Spaces" button) | **No animation** | `isSpacesOpen && createPortal(...)` (Sidebar.tsx:1924). Pops instantly. Not a shadcn Dialog (spec §5.6 says to use Dialog). | §8 modal open pattern: fade + scale. | **Add (P1).** Two options: (a) add `animate-in fade-in-0 zoom-in-95 duration-base` to the `<aside>` and `animate-in fade-in-0` to the backdrop button (quick win without migrating to shadcn Dialog), or (b) properly migrate to shadcn Dialog per spec §5.6. Option (a) is 15 min; (b) matches spec. |
| 16 | Spotlight open/close (⌘K) | Yes | Dialog.Overlay + Dialog.Content with `animate-in fade-in-0 zoom-in-95` (Spotlight.tsx:190, 193). | Matches §6 and §8. | **Tune.** Same token-binding as #13. Also: spotlight warrants `--duration-slow` (280ms) per TOKENS-SPEC §7.1 "modal/dialog open" — this is a full-width deliberate surface, slightly slower reveal reads more intentional. Right now it snaps open at 150ms which is fine for a popover but feels undercooked for the app's most prominent overlay. |
| 17 | Spotlight result selection (keyboard ↑↓) | **Static pop** | `isSelected && <span className="... left-0 w-0.5 bg-accent" />` (Spotlight.tsx:242). Accent bar pops between rows on keyboard nav. | §8 doesn't address but this is same pattern as #10. | **Add.** Same `layoutId` shared-element trick as #10 — spotlight accent bar slides vertically as user presses arrow keys. This is the motion moment that turns a functional cmd-K into a *feeling* cmd-K (see Things 3, Raycast). Single highest-leverage polish moment in the spotlight. |
| 18 | New conversation create | Partial | Spinner on button (`Loader2 animate-spin`). New conversation row fades in via Message-row entrance (wait, no — the sidebar row entrance is NOT animated; messages are). | §8 implied instant. | **Tune.** Sidebar row appearance is a pop. Could add `animate-in fade-in-0 duration-fast` to the first-rendered row. Low priority — the act of creating a conversation moves focus elsewhere so the pop isn't seen. |
| 19 | Delete conversation | Partial | `window.confirm()` native modal (jarring — browser chrome leaks in). `Loader2 animate-spin` during delete. Row vanishes from the list instantly on success. | Spec doesn't cover motion for destructive confirm. | **Tune.** Row exit pop is acceptable. The real issue is `window.confirm` — native confirm breaks the design language. Replace with shadcn AlertDialog (out of motion scope, but worth flagging). Motion-wise: row fade-out over `--duration-fast` `--ease-in` would be courteous. |
| 20 | Rename conversation | Yes | Renaming state renders an inline `<input>` in place of `<h3>`. No transition; swap is instant. Save → visual returns to `<h3>`. | Spec doesn't cover. | **Keep.** Inline rename is snappy at its best — fade/morph transitions would slow it. Current pop is correct. |
| 21 | Empty state → first message | No transition | Empty state conditional-renders; first message renders through the message list. | §8: no page-mount cascade. | **Keep.** Correct per spec. Could add a whisper-fade on the empty-state-hiding transition if wanted (opacity 1→0 over 120ms before messages render) but not needed. |
| 22 | Error state in message | **Static pop** | Failed message shows `<AlertCircle />` + error text inline (Message.tsx:198). Pops into header row. | §7 (states table) doesn't prescribe motion. | **Tune.** Inline error should fade in (`animate-in fade-in-0 duration-fast`) — user just submitted, they need a calm "here's what happened" not a jarring text insertion. Low-lift, high-polish. |
| 23 | Verification badge state change | Partial | Badge renders based on `verificationSummary`. When status changes post-stream, the badge swaps class names. No transition between states (off → verified → partially verified). | Spec §3.2 doesn't specify. | **Tune.** Color transition via `transition-colors duration-base`. This is a signal state change — the user will watch the badge resolve from "claims evaluating" to "verified" and a 180ms ease-out color shift sells it. Right now: pop. |
| 24 | Scroll to bottom on new message | Yes | `container.scrollTo({ top: ..., behavior: isConversationChange ? 'auto' : 'smooth' })` (ChatPanel.tsx:170). Smooth for streaming, jump for thread change. | Correct pattern — matches §8 intent for thread change vs streaming. | **Keep.** This logic is well thought through. |
| 25 | Deep-link highlight flash | Yes | `chat-message-highlighted` class (index.css:192) applies `chat-message-highlight` keyframe: `inset 2px 0 0 hsl(var(--accent))` → transparent, 1500ms `--ease-out`. | Matches §8 exactly. | **Keep.** One of the well-executed motions in the feature. |
| 26 | File preview modal open/close | Yes | Dialog.Overlay + Content with `animate-in fade-in-0 zoom-in-95` (FilePreviewModal.tsx:481). Default 150ms. | §7 doesn't explicitly cover but implies same overlay pattern. | **Tune.** Same as spotlight (#16) — 92vh × 96vw dialog deserves `--duration-slow` (280ms). Currently feels too fast for a modal of that size; reads as a popover in disguise. |
| 27 | Focus rings on tab navigation | Yes | `:focus-visible` CSS rule: 2px outline, 2px offset, no box-shadow halo (index.css:163). No fade-in — instant. | Spec §8 (TOKENS-SPEC): "Focus ring appearance, `--duration-fast`." Spec actually IS ambiguous — AESTHETIC-GUIDE §6 says "focus rings are crisp" (implying no fade), TOKENS-SPEC §7.3 lists "focus ring appearance" as animated. | **Keep.** Current behavior (instant) is correct for keyboard-first confidence. TOKENS-SPEC §7.3 should probably be updated to mark focus ring as NOT animated. Flag as spec inconsistency, not code issue. |
| 28 | Button.tsx `whileTap={{ scale: 0.98 }}` | Yes | Framer-motion `motion.button` with `whileTap={{ scale: 0.98 }}` (Button.tsx:65). | Spec doesn't address. | **Tune subtly.** 0.98 is almost imperceptible — you can FEEL it more than SEE it on small buttons. For larger buttons (size="lg"), 0.97-0.975 is right. For small icon buttons (32×32 Send), 0.96-0.97 reads. **The bigger issue**: Button.tsx is used for some things, but Chat's primary Send button (ChatPanel.tsx:531) is a hand-rolled `<button>` with NO press feedback. The polish exists in a primitive nobody in Chat uses. Adopt `components/ui/Button` for Send + Stop, or inline the `active:scale-[0.97]` Tailwind utility (no framer-motion dependency). See P0. |

**Summary counts:**
- Keep as-is: 7 moments (2, 3, 9, 20, 21, 24, 25, 27)
- Tune (duration/easing/token-binding): 11 moments (5, 6, 11, 13, 13b, 14, 16, 18, 22, 23, 26, 28)
- Add (motion missing): 6 moments (7, 10, 12, 15, 17, 19-ish)
- Fix (wrong motion): 1 moment (8 — per-message cascade on thread switch)
- Defer: 1 moment (4 — tool calls)

---

## 3. Signature moments

Three places where a small investment in motion craft pays back the whole feature. In priority order:

### 3.1 The sidebar active-conversation bar (highest ROI)

**Current:** 2px accent bar conditionally renders on the active row. Switch conversations → bar vanishes from old row, appears on new row. Zero motion. The most-used motion path in the app is the most static.

**Proposal:** Use Framer Motion's `layoutId` to make the accent bar a single DOM node shared across all conversation rows. Framer Motion handles the position interpolation — the bar physically slides vertically from old row to new row over `--duration-base` (180ms) `--ease-out`. Reduced-motion: bar jumps instantly (per `useReducedMotion`).

**Why it matters:** This is the one motion moment the user experiences every session, multiple times per session. Linear, Notion, Arc all do this. It's the single most "finished" detail you can add. Costs maybe 20 lines of code including the reduced-motion fallback.

**Bonus:** Same `layoutId` pattern applies to spotlight keyboard selection (#17). Share the concept across both surfaces for a unified feel.

### 3.2 Send button press state

**Current:** User clicks Send. Nothing visible happens for 100-400ms until optimistic message renders in the thread (below the fold, out of view). In a thinking tool people use hundreds of times a day, that gap is a confidence bleed.

**Proposal:** 60ms scale-down on `:active` (`active:scale-[0.97]`), 120ms color shift to `--accent-hover` on hover, 0ms return (snap back). Optional secondary: subtle icon state — Send icon nudges 2px right on press, returns on release (costs almost nothing, adds a physics-of-a-paper-airplane detail that's memorable without being cute).

**Why it matters:** This is the primary action of the primary screen. It earns more attention than any other single button in the app.

### 3.3 Spotlight scale + duration

**Current:** ⌘K opens a dialog with `zoom-in-95` at 150ms `ease`. Feels like a popover.

**Proposal:** Spec-aligned `--duration-slow` (280ms) `--ease-out` for open, `--duration-base` (180ms) `--ease-in` for close. Combine with the #17 result-selection slide for a spotlight that feels as deliberate as Raycast.

**Why it matters:** Spotlight is the app's keyboard spine (per AESTHETIC-GUIDE §2.7). Its entrance is the user's reward for reaching for ⌘K. Right now: a competent popover. With the change: a considered moment.

---

## 4. Global rules to establish

These should be codified so the next component built inherits them without rediscovery.

### 4.1 Floating surface motion (popovers, dialogs, command palette)

- **Open:** `fade-in-0 zoom-in-95` + `duration-base` (180ms) + `ease-out`
- **Close:** `fade-out-0 zoom-out-95` + `duration-fast` (120ms) + `ease-in`
- **Large modals** (dialogs >640px or >50vh — e.g. Spotlight, FilePreviewModal): bump open to `duration-slow` (280ms).
- **Backdrop:** fade only, `duration-base` / `duration-fast`, matches content timing.

Action item: introduce `animate-in` variant utilities wired to our token classes, or a single `cn()` helper `motionFloat(open | close, size)` that composes the classes. The `tailwindcss-animate` plugin does NOT bind to `transitionDuration` tokens — the `duration-base` class applies to `transition-*`, not `animation-*`. Resolution options:

- **Option A (light):** set a custom `--animate-duration` CSS variable and wrap `animate-in` usage in a helper that inlines `style={{ animationDuration: 'var(--duration-base)' }}`.
- **Option B (stronger):** migrate Radix animations to framer-motion `AnimatePresence` inside the custom `PopoverContent` wrapper. Keeps the API identical, gives full control.

Recommend Option A for this pass. B is a larger migration.

### 4.2 Hover state motion

- **All color hover transitions** use `transition-colors duration-fast` (120ms, ease is the browser default cubic-bezier-ish-out — acceptable).
- **All opacity reveal transitions** (hover-revealed icons) use `transition-opacity duration-fast`.
- **NO hover transforms** (no `translateY`, no `scale` on hover). Active/pressed states only use scale. This is already the pattern; codify it.

Action item: grep for `transition-colors` without a `duration-*` sibling in Chat. Several in `ConversationSidebar.tsx`, `ConversationLinkedDocumentsPanel.tsx`, `SourceCitations.tsx`, `CitationFootnote.tsx` — all currently run at the browser default (0ms, no transition). Add `duration-fast` universally.

### 4.3 Press/active state motion

- **All clickable buttons:** `active:scale-[0.97]` OR `whileTap={{ scale: 0.97 }}` if framer-motion is already imported.
- **No press state on text links** (the underline is the press affordance).
- **No press state on icon-only buttons smaller than 20px** (the target is too small for scale feedback to read).
- **Reduced motion:** disabled via `motion-reduce:scale-100` utility.

Action item: add to `components/ui/Button` as default (already there, tuned to 0.98 — bump to 0.97 for more perceptible feedback), promote adoption in Chat by replacing inline `<button>` elements in `ChatPanel` send/stop buttons and sidebar new-conversation button.

### 4.4 List selection motion

- **Active/selected indicator** (sidebar, spotlight, any list with keyboard nav): the indicator uses `layoutId` to slide between positions. Duration: `--duration-base` (180ms) `--ease-out`.
- **Reduced motion:** indicator jumps (no `layout` animation).

Action item: refactor sidebar active-row-bar (#10) and spotlight keyboard-selection-bar (#17) to share a motion pattern.

### 4.5 Expand/collapse motion

- **Any accordion-style expand:** height interpolation `--duration-base` (180ms) `--ease-out`. Use `grid-template-rows` trick or Radix `Collapsible` primitive. DO NOT `max-height` with a large value — it distorts timing.
- **Reduced motion:** instant expand.

Action item: `SourceCitations.tsx` (#7), `ConversationLinkedDocumentsPanel.tsx` (already uses native `<details>` for scope assignments — acceptable, but the main panel `{expanded && ...}` pop could use the same treatment).

### 4.6 List entrance motion

- **Individual messages appear:** `animate-in fade-in-0 duration-fast` — CURRENT behavior.
- **Bulk message render on conversation switch:** container fades in once (`opacity: 0 → 1`, 180ms), per-message entrance animations are suppressed on first-render.
- **Never stagger.** The aesthetic guide's ban on cascade applies here.

Action item: fix #8.

### 4.7 Reduced motion compliance

The CSS media query in `index.css:170` correctly overrides animation/transition durations to ~0. However, framer-motion `whileTap`, `layoutId`, and `AnimatePresence` need explicit `useReducedMotion()` handling — they don't honor the CSS media query. Any framer-motion introduction (#10, #17) MUST use `useReducedMotion()` to disable motion.

---

## 5. Implementation punch-list

### P0 — clearly wrong, must fix

1. **Per-message cascade on conversation switch (#8).** Fix in `ChatPanel.tsx` by gating `animate-in` on `Message` component behind a "is this the first render of this thread?" sentinel. Options:
   - Assign a `key={conversationId}` to the thread container, `initial` vs. subsequent renders tracked with a ref.
   - Have `Message` accept `isInitialRender?: boolean` prop, skip `animate-in` when true.
   - Simplest: track `previousConversationId` in ChatPanel (already present on `previousConversationIdRef`), pass `shouldAnimate={!isFirstRenderAfterSwitch}` to Messages for one frame, flip to true. Time: 15 minutes.

2. **Send button has no press feedback (#12).** Add `active:scale-[0.97] motion-reduce:active:scale-100` to `ChatPanel.tsx:531` (send) and `ChatPanel.tsx:521` (stop). Or replace with `components/ui/Button`. Time: 5 minutes for Tailwind-only, 30 minutes for Button migration.

3. **Active-row accent bar pops rather than slides (#10).** Signature moment. Time: 30-60 minutes (install framer-motion into sidebar, add `layoutId="sidebar-active-bar"`, wire reduced-motion fallback).

### P1 — spec gaps

4. **SourceCitations expand/collapse is a pop (#7).** Add height transition. Option: wrap the `<ul>` in a `Collapsible` primitive from Radix (shadcn/Collapsible if added), or use `grid-template-rows: 0fr → 1fr` inline. Time: 20 minutes.

5. **Spaces panel portal has zero motion (#15).** Add `animate-in fade-in-0 zoom-in-95 duration-base` to the `<aside>` and `animate-in fade-in-0 duration-base` to the backdrop. Or migrate to shadcn Dialog per spec §5.6 (larger scope). Time: 10 minutes (quick fix) / 2 hours (migration).

6. **Popover/dialog durations not token-bound (#5, #6, #13, #13b, #16, #26).** Across all uses of `animate-in fade-in-0 zoom-in-95`, bind `animation-duration` to `var(--duration-base)` via inline style. Differentiate open/close easing (`ease-out` vs `ease-in`). Bump modal-size dialogs (spotlight, file preview) to `var(--duration-slow)`. Time: 45 minutes total.

7. **Hover transitions without duration (many).** Grep `transition-colors` in Chat files without a `duration-*` neighbor; add `duration-fast`. Locations:
   - `ConversationSidebar.tsx` filter chips (#11, line 1250)
   - `ConversationSidebar.tsx` spaces panel buttons (lines ~1955, ~1963, ~2014, ~2021, ~2169)
   - `ConversationLinkedDocumentsPanel.tsx` (all `transition-colors`)
   - `CitationFootnote.tsx` (the `[1]` marker)
   - `MessageActions.tsx` (all three action buttons)
   - `ComposerControls.tsx` (tab buttons, tool rows)
   Time: 20 minutes.

### P2 — polish

8. **Spotlight keyboard selection indicator slide (#17).** Same `layoutId` pattern as #10. Time: 20 minutes once #10 is in place.

9. **Verification badge state transition (#23).** Add `transition-colors duration-base` to the badge wrapper so `--success-muted` → `--warning-muted` fades rather than pops. Time: 5 minutes.

10. **Inline error in message (#22).** Add `animate-in fade-in-0 duration-fast` to the failure `<span>`. Time: 3 minutes.

11. **Streaming cursor color contrast.** Verify `--text-muted` (3.4:1) is visible during streaming; if marginal, bump to `--text-tertiary` (5.3:1). Time: 5 minutes.

12. **Focus ring spec inconsistency.** Update `TOKENS-SPEC.md §7.3` to mark "focus ring appearance" as NOT animated (matching actual crisp-instant behavior). Time: 2 minutes.

13. **Button.tsx scale tuning.** Bump `whileTap` from 0.98 to 0.97. Time: 30 seconds.

### Spec follow-ups (not code)

14. Add to `CHAT-REDESIGN-SPEC.md §8` the global motion rules in §4 above, so future components don't re-derive.

15. Add a "motion theme" line to `AESTHETIC-GUIDE.md §2.5`: "Hover is ≤120ms color; press is ~60ms scale; floating surfaces are 150-280ms fade+scale; list selection slides, never pops."

---

## 6. Appendix — what's already right

Worth recognizing so the refactor doesn't accidentally remove:

- `ChatPanel.tsx:172` — `behavior: isConversationChange ? 'auto' : 'smooth'` in the scrollTo is exactly right. Jump on thread change, smooth on streaming. Don't touch.
- `index.css:181` `chat-cursor-blink` — 1s `steps(2, end)` is the classic terminal cursor rhythm. Correct.
- `index.css:192` `chat-message-highlighted` — 1500ms ease-out bar fade replaces the old cyan-ring glow. Clean, purposeful, spec-aligned. Exemplary.
- `index.css:170-178` — reduced-motion media query, correct coverage of CSS transitions and animations.
- The aesthetic choice to NOT animate most hover states to the degree of the aesthetic guide's "if animation is noticed, it's too much" — restraint is real here. The tune-ups in §5 do NOT add noise; they make the restraint legible.
