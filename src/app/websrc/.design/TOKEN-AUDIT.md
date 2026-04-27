# Token System Diagnostic Audit

**Scope:** `/src/app/websrc/` — React 19 + Tailwind + shadcn/Radix + framer-motion
**Verdict:** Three overlapping token systems, no single source of truth, rainbow-accent sprawl, shadcn primitives unused in feature code.

---

## 1. Three Competing Token Systems

**System A — Tailwind shadcn HSL vars** (`index.css:24-77`)
Monochrome neutrals only. `--primary` is literally black (`0 0% 9%`). `--accent` is a near-neutral gray (`0 0% 96.1%`). This is stock shadcn, untouched.

**System B — `themes.css` hex vars** (`styles/themes.css:4-166`)
Entirely separate palette keyed on `[data-theme="dark"]`. Defines `--bg-primary`, `--text-primary`, `--accent-primary` (sky-500), plus `--accent-purple/pink/orange/emerald`, `--glass-bg`, `--gradient-brand`, `--gradient-mesh`, `--shadow-glow`. None of these flow into Tailwind config.

**System C — Tailwind `brand.*` scale** (`tailwind.config.js:19-31`)
Sky-blue 50–950 ramp. **Used in 0 components** (`bg-brand-*` grep: no matches). Dead tokens.

Both A and B toggle dark mode via different mechanisms (A: `.dark` class, B: `[data-theme="dark"]` attribute). `useApplyTheme.ts:27-33` applies both simultaneously — the switching works, but nothing unifies what each system means.

---

## 2. Accent Color Story: Rainbow Soup

No single accent. Competing definitions:
- `tailwind.config.js:32-38` — `accent.purple/pink/orange/emerald` as literal hex
- `themes.css:29-32` and `:131-134` — same four as CSS vars, plus a fifth (`--accent-primary` = sky blue)
- `ChatPanel.tsx:423,461,510,520` — inline `from-blue-500/20 to-purple-500/20`, `from-green-500/20 to-teal-500/20`, `from-rose-500/20 to-orange-500/20`, `from-blue-500/20 to-indigo-500/20` within a single file
- `themes.css:156` dark-mode gradient goes sky → indigo (a sixth hue)

Gradient utilities (`from-*`/`to-*`/`via-*`) appear **1,349 times across 167 files**.

---

## 3. Tokens Used Directly, Not Semantically

- `bg-primary`/`bg-secondary`/`bg-muted`/`bg-accent` semantic utilities: **250 uses across 96 files** (ok — mostly shadcn primitives)
- `text-gray-*`, `bg-blue-*`, etc. raw palette utilities: **422 uses (166 bg + 256 text) across 51 files**
- Inline `var(--...)` in className brackets (e.g. `bg-[var(--surface-elevated)]`): **2,540 uses across 167 files**
- `style={{...}}` inline styles: **68 occurrences**
- Hardcoded hex `#...` literals in TSX/CSS: **166 occurrences across 23 files** (most egregious in `styles/themes.css:6-165` and `command-palette.css:108-130`)
- Opacity-suffix white/black (`bg-white/10`, `text-white/60`): **388 uses across 26 files** — pattern concentrated in Chat, DailyNotes, and ReferenceInbox

The shadcn `Button` (`components/ui/button.tsx`) correctly uses semantic tokens. Feature code does not reuse it — instead it hand-rolls buttons with gradient + blur + border-opacity strings.

---

## 4. Decorative Flourishes In Use

- **Shadows:** `shadow-glow`, `shadow-elevation-1/2/4`, `shadow-sm/md/lg/xl/2xl`, and colored `shadow-blue-500/10`, `shadow-rose-500/10`, `shadow-black/40` inline
- **Glows:** `.glow`, `.glow-hover`, `shadow-glow` (`tailwind.config.js:102`, `themes.css:90,165`)
- **Glass/blur:** `backdrop-blur-xs/sm/md/xl/2xl`, `.glass`, `.glass-strong`, `.glass-navigation`, `.glass-modal`, `.glass-subtle` (`themes.css:357-555`)
- **Gradients:** `--gradient-brand`, `--gradient-brand-subtle`, `--gradient-mesh`, plus 167 files using inline `from-*/to-*`
- **Non-linear easings:** `bounce-in` cubic-bezier(0.68,-0.55,...), `ease-bounce`, `ease-spring`, `transition-bounce` (`tailwind.config.js:143`, `themes.css:96,101-102`)
- **Animations:** `shimmer`, `fade-in`, `fadeIn`, `glow-pulse`, `text-shimmer`, `bounce-in`, `animate-pulse-slow`, `animate-shake` (across `tailwind.config.js` + `themes.css` + `ErrorBoundary.animations.css`)
- **Hover transforms:** `translateY(-2px)`, `translateY(-4px)` in `.card-interactive`, `.bento-card`, `.bento-card-enhanced`
- **Top-border accents, bento cards, segmented controls, command-bar aesthetic** — all defined as utility classes in `themes.css:407-505`

"2025 trend" comments appear in the CSS. That's a symptom.

---

## 5. Typography: Scale Defined, Not Used

Defined in `tailwind.config.js:77-92` — eleven `text-display-*`/`text-heading-*`/`text-body-*` clamp-based tokens plus `caption`.

**Usage in components: zero matches** for `text-display`, `text-heading`, `text-body`, `text-caption`.

Meanwhile `text-xs/sm/base/lg/xl/2xl/3xl` raw sizes appear **1,349 times across 167 files**. The semantic scale is dead code.

---

## 6. Dark Mode: Works, But Split-Brain

`useApplyTheme.ts` toggles both `.dark` (Tailwind) and `[data-theme="dark"]` (themes.css) — so switching works visually. But the two systems encode different palettes: shadcn goes black/white monochrome, themes.css goes navy + saturated accents. Components that use `bg-background` (System A) and components that use `bg-[var(--bg-primary)]` (System B) will not look like they belong to the same app in dark mode.

Chat components hardcode `bg-[#0d1117]`/`bg-[#0a0e14]`/`bg-[#0b1118]`/`bg-[#0f1723]` (`ChatPanel.tsx:421,438`, `ConversationSidebar.tsx:1892`, `ConversationSpotlight.tsx:220`) — four bespoke dark-mode hexes that bypass both systems entirely.

---

## Top 5 Concrete Inconsistencies (Biggest Needle-Movers)

1. **`themes.css` duplicates and contradicts `index.css`** — `index.css:32` says `--primary: 0 0% 9%` (black). `themes.css:20` says `--brand-primary: #0ea5e9` (sky blue). A button labeled "primary" in one system looks nothing like a button using "brand" in the other.
2. **Five accent hues defined, none chosen** — `tailwind.config.js:32-38` declares purple/pink/orange/emerald as peers; `themes.css:29-32,131-134` repeats them; `ChatPanel.tsx:423,461,510,520` applies four different gradient pairs in one file. There is no primary accent.
3. **Typography scale is entirely unused** — eleven fluid-clamp tokens in `tailwind.config.js:77-92` (`text-display-2xl`, `text-heading-lg`, etc.); zero references in `components/`. All 1,349 text-size usages are raw `text-xs/sm/base/lg`.
4. **Chat feature hardcodes hex backgrounds** — `ChatPanel.tsx:421,438` `bg-gradient-to-b from-[#0d1117] to-[#0a0e14]`; `ConversationSidebar.tsx:1892` `bg-[#0b1118]/95`; `ConversationSpotlight.tsx:220` `bg-[#0f1723]`. Four different "almost-black" values, none tokenized.
5. **shadcn primitives ignored by feature code** — `components/ui/button.tsx` uses semantic `bg-primary` etc. correctly, but `ChatPanel.tsx:510-520`, `ConversationSidebar.tsx:1128`, `FirstFolderPicker.tsx:249` hand-roll buttons with gradients, `backdrop-blur-sm`, and opacity-suffix borders instead of composing `<Button>`.

---

**Word count:** ~495
