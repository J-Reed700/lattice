# UX Overhaul Brief — September 2026

**Status:** active implementation brief. Read this before touching any surface.
**Pairs with:** `AESTHETIC-GUIDE.md` (visual DNA) and `PRODUCT-THESIS.md` (what the app is).
**Goal:** the app currently speaks two visual dialects. The redesigned surfaces (Home, Chat thread, Journal, References) are quiet and editorial. Everything else (Library, Search, Import, Settings, the Chat sidebar, first-run, loading) is generated-looking: cards inside cards, uppercase section labels, an explanatory sentence under every heading, icons in tinted boxes, jargon. This brief collapses the app to the first dialect.

---

## 1. The tells we are removing

If you see one of these, it goes.

| Tell | Replace with |
|---|---|
| Explanatory sentence under a title ("Configure retrieval behavior, reranking, and pipeline tuning") | Nothing, or a data line ("1,247 documents · 412 MB"). |
| Uppercase tracked section labels (`COLLECTIONS & PRESETS`, `OPTIONAL SPACE SCOPE`) | Sentence-case heading in `text-sm font-medium text-text-primary` or the `SectionHeading` pattern from Home. |
| Icon inside a tinted rounded box next to a heading | Just the heading. |
| Card inside a card, bordered box inside a bordered box | One level. Hairline `border-t`/`border-b` between rows. No box at all when type and spacing already separate things. |
| Pills/badges for file types, status, counts (`PDF Document`, `LOCAL`, `8 files`) | Plain muted text in the meta line. A pill only when it conveys state (e.g. `Active`). |
| Full-width accent button as the first thing in a sidebar (`+ New conversation`) | An icon button in the sidebar header row with a tooltip that shows the shortcut. |
| Percentages for things that aren't percentages (BM25 1200.0%) | The real number, formatted, or nothing. |
| Placeholder stats that are always zero (`Storage 0 B`, `Searches today 0`) | Only real numbers. Delete the stat if there is no source for it. |
| Copy: "your knowledge base", "knowledge library", "Easy on the eyes", "Bright and clean", "Start typing to…", "Drag & drop files here or click the button below" | Plain nouns and verbs. See §4. |
| `Sparkles` icon, celebratory copy ("Welcome to Lattice!") | No sparkle. Neutral copy. |
| Decorative empty-state icon in an accent circle with a `text-2xl` title | One line of muted text, optionally one action. |
| `console.log` in render paths | Removed. |
| Redundant labels (group header "Today", row label "TODAY", row title "Friday, September 4") | Say it once. |

---

## 2. Page anatomy (mandatory)

Every top-level route uses one of two anatomies. Do not invent a third.

### A. Reading column (Home, Search, Import, Settings content pane)

```
<main class="h-full overflow-y-auto bg-bg">
  <div class="mx-auto w-full max-w-[760px] px-6 pt-10 pb-16">
    <PageHeader title="Library" meta="1,247 documents · 2 folders" actions={…} />
    …sections…
  </div>
</main>
```

`PageHeader` (`components/ui/PageHeader.tsx`) is the only page title. Title is `font-serif text-3xl font-semibold tracking-[-0.02em]`. `meta` is one line of `text-sm text-text-tertiary` data. `actions` is an optional right-aligned cluster of small ghost/secondary buttons. Nothing else goes in the header.

Sections use `SectionHeading` from Home (`text-lg font-medium text-text-secondary pb-3`) followed by a `border-t border-border-subtle` container whose rows are `border-b border-border-subtle`.

Library is the one reading-column page that is allowed to go wider (`max-w-[1100px]`) because it is a dense table.

### B. Sidebar + main (Chat, Journal, References, Library when a rail is open)

Sidebar is `w-[280px] shrink-0 border-r border-border-subtle bg-surface flex flex-col`.

Sidebar header row is identical everywhere:

```
<div class="flex h-12 items-center justify-between border-b border-border-subtle px-4">
  <h2 class="font-serif text-sm font-semibold text-text-primary">Chat</h2>
  <div class="flex items-center gap-1">
    <IconButton label="New conversation" shortcut="⌘N" icon={Plus} />
    <IconButton label="Hide sidebar" shortcut="⌘\" icon={PanelLeft} />
  </div>
</div>
```

Below the header, in this order and only if the surface needs them: scope selector (one line, `text-sm`, a `Select`-style trigger — never a labelled box), search input (`h-8`, `rounded-sm`, `border-border-default`, `bg-bg`), a single row of text filter tabs (`text-xs`, underline-active, max five, one row). Then the list.

List rows: title on one line with `truncate` (never `line-clamp-1 break-words` on a narrow flex child), one meta line `text-xs text-text-muted` combining space and time with ` · `, optional one-line preview `text-xs text-text-tertiary truncate`. Active row: `bg-surface-raised` plus a 2px accent bar on the left, matching Home/Journal.

---

## 3. Settings anatomy

Settings uses anatomy A for the content pane and a plain text list for the sidebar (no icon box, no icons at all in the tab list). Groups are separated by a `text-xxs uppercase` group label only because it is a navigation list, not content.

Content is built from two primitives in `components/ui/SettingsSection.tsx`:

- `SettingsSection` — `title` (`text-base font-medium`), optional one-line `description` (`text-sm text-text-tertiary`) only when the section name is genuinely ambiguous, then children separated by hairlines. No card, no background.
- `SettingsRow` — a `label` (`text-sm text-text-primary`), an optional `hint` (`text-xs text-text-muted`) under the label, and the control right-aligned in a `min-w-[220px]` column. A row with a wide control (textarea, list) stacks the control under the label.

Sidebar footer (Export / Import / Reset) must never clip the tab list: the tab list scrolls, the footer is `shrink-0`. Remove the "Saved automatically" line with the green dot; autosave is the default and needs no announcement. Show a transient "Saved" in the row that changed if feedback is needed.

Dead settings are deleted, not restyled. A control that changes nothing in the app is a lie.

---

## 4. Voice

- Titles are nouns: Home, Search, Library, Import, Chat, Journal, References, Settings.
- Buttons are verbs: Import, Add folder, Install, Send. Never "Click to…".
- The product refers to the user's material as "your documents" or "your library". Never "knowledge base".
- Empty states are one sentence, present tense, no exclamation marks, optionally one action. Examples: "No conversations yet." / "Nothing indexed yet. Add a folder to start." / "No results for 'harissa'."
- Errors say what happened and what to do, in that order: "Couldn't read the models folder. Check that the app has disk access."
- Loading is a single line: "Loading…". The boot screen says "Lattice" and nothing else.
- Shortcuts are shown in tooltips as `⌘N`, never in button labels.
- Never explain the UI to the user in the UI ("Catalog hidden to keep this page compact. Expand when you want…"). If a control needs a paragraph, the control is wrong.

---

## 5. Motion, color, density

- Keep the existing tokens. Do not add colors. Semantic color is for state only.
- Accent fill is reserved for one primary action per screen. Everything else is ghost or secondary.
- No `shadow-md` on anything that does not float.
- Rows are `py-2.5` to `py-3`. Cards are gone, so density comes from rhythm, not padding.
- Transitions stay at `duration-fast`. No entrance animations on lists.

---

## 6. Non-negotiables while implementing

- Preserve every existing behavior. This is a re-skin plus copy pass plus removal of dead controls. If a feature has a test, the test still passes; update assertions for copy changes only.
- Tokens only. Use the Tailwind color aliases (`bg-bg`, `text-text-primary`, `border-border-subtle`) or `hsl(var(--token))`. No raw palette classes.
- Everything must render in both themes. The screenshot rig is `scratchpad/shots.mjs`; run it and look before declaring done.
- `npm run type-check`, `npm run lint`, and `npx vitest run` must pass.
