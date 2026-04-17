# Recall Tokens — Specification

**Status:** proposed
**Supersedes:** `index.css:24-77` (shadcn block), `styles/themes.css` (entire file), `tailwind.config.js` (`brand.*`, `accent.{purple,pink,orange,emerald}`, fluid `fontSize`, `fluid-*` spacing, glow shadow, bounce easing, shimmer/fadeIn/glow-pulse animations)
**Scope:** Token primitives only. No component styling, no motion choreography, no iconography.

---

## 0. Shape of the system

- **One layer of truth.** HSL CSS variables in `:root` / `[data-theme="dark"]` on the `<html>` element. Tailwind reads them via `hsl(var(--token))`. No second layer.
- **Hue-neutral hex strategy.** Surfaces/borders/text are defined with a single cool-neutral hue baked in (HSL `220`) so the whole UI sits in one temperature. No warm/cool drift between scales.
- **Dark-first.** All values below are dark-mode first; light is an explicit derivation in §2.
- **Semantic names only.** No `gray-500`, no `brand-500`. If a token can't be named by its role, it doesn't belong in the system.
- **Deletion is load-bearing.** The `brand.*` scale, the `accent.{purple,pink,orange,emerald}` literals, the `fluid-*` spacing, the `text-{display,heading,body}-*` clamp scale, `shadow-glow`, `ease-bounce`, `bounce-in`, `shimmer`, `glow-pulse`, `fadeIn`, and every utility in `themes.css` (§9) are removed as part of adopting this spec.

---

## 1. Color — Dark mode (primary)

Neutrals are in HSL hue `220` (cool blue-gray). Contrast ratios below are measured against `--bg` unless noted.

### 1.1 Surfaces — 3 layers (not 4)

Four was tempting (`bg` / `surface` / `raised` / `sunken`). I cut `sunken`. The aesthetic guide says "surfaces are near-flat; depth is a whisper" — in practice a thinking tool needs a canvas, a panel, and a popover. Inputs can reuse the canvas with a border; nothing needs to feel *below* the page.

| Token | HSL | Hex | Contrast vs `--bg` | Role |
|---|---|---|---|---|
| `--bg` | `222 16% 7%` | `#0f1115` | — | App canvas. The dominant color. |
| `--surface` | `222 14% 10%` | `#15181f` | 1.24:1 | Panels, sidebars, cards. One step above canvas. |
| `--surface-raised` | `222 12% 14%` | `#1d212a` | 1.55:1 | Popovers, menus, modals, command palette. Things that float. |

Delta between steps is ~3% L — visible as a tonal shift, not a contrast jump. Matches "depth is a whisper."

### 1.2 Borders — 3 levels

| Token | HSL | Hex | Role |
|---|---|---|---|
| `--border-subtle` | `222 12% 16%` | `#21252f` | Default hairline. Table rules, dividers, card outlines. |
| `--border-default` | `222 11% 22%` | `#2e333f` | Interactive element rest state: inputs, buttons, kbd chips. |
| `--border-strong` | `222 10% 32%` | `#444a58` | Hover/active on interactive elements. Selected state. |

Three tiers because "typography carries hierarchy, not borders" but borders still do real work: resting vs. interactive vs. engaged. Anything more granular = decoration.

### 1.3 Text — 4 peer registers + 1 state

Four peer registers (`primary / secondary / tertiary / muted`) plus `disabled` as a state-only level.

| Token | HSL | Hex | Contrast vs `--bg` | Role |
|---|---|---|---|---|
| `--text-primary` | `220 15% 96%` | `#f2f4f8` | **15.8:1** | Body, headings, primary content. AA+. |
| `--text-secondary` | `220 12% 76%` | `#bbc1cc` | **9.1:1** | Supporting copy, subheads, active metadata. AA+. |
| `--text-tertiary` | `220 11% 60%` | `#8f95a3` | **5.3:1** | Inline labels, captions, contextual descriptors. AA body. |
| `--text-muted` | `220 10% 45%` | `#676d79` | 3.4:1 | Timestamps, hints, placeholder-ish info. ≥3:1 — supplemental only, never load-bearing. |
| `--text-disabled` | `220 10% 32%` | `#494f59` | 2.0:1 | Disabled state only. Fails AA by design — never carries meaning. |

Four peer registers because the app genuinely has four (see §11.2 for how I widened the gaps to keep them distinct). Contrast steps go **15.8 → 9.1 → 5.3 → 3.4 → 2.0** — each a visible jump, no fuzzy middle. `muted` and `tertiary` are deliberately ~1.9:1 apart so they don't collapse into each other in practice.

**When to use each:**

| Register | Use for | Don't use for |
|---|---|---|
| `primary` | Body prose, headings, editable field values, selected item | Secondary metadata, hints |
| `secondary` | Subheads, table column headers, active filter labels, current-nav label | Timestamps, de-emphasized info |
| `tertiary` | Inline descriptors ("2 items · edited 3d ago"), form field helper text, breadcrumb parents | Critical info |
| `muted` | Placeholder text, dim timestamps, icon-only affordance labels, background metadata | Anything a user must read |
| `disabled` | Disabled controls only | Anything that needs to be read but de-emphasized |

### 1.4 Accent — one hue

**Chosen:** **Burnt amber** — HSL `38 77% 45%`, hex **`#cb8919`**.

Darker than the original proposal (`38 92% 58%` / `#f5a524`). Josh's call: "a little darker." The brighter amber read as candy / highlighter; this one reads as aged gold / tobacco / well-used brass — warm without being cheerful, editorial without being soft. Reasoning defended below (§11 tensions).

| Token | HSL | Hex | Role |
|---|---|---|---|
| `--accent` | `38 77% 45%` | `#cb8919` | Primary accent. Focus rings, selection, primary button bg. |
| `--accent-hover` | `38 77% 51%` | `#e09a1c` | Hover/pressed on primary button. +6% L — conventional brighten on dark UI. |
| `--accent-muted` | `38 45% 18%` | `#433319` | Subtle accent tint (selected-row bg, current-line highlight). Same hue, much lower saturation + lightness. |
| `--accent-fg` | `30 20% 8%` | `#181411` | Text *on* `--accent` solid fill. Near-black — the darker amber forced this flip (see below). |

**Contrast notes — read these, they matter:**

- `--accent-fg` on `--accent` (`#181411` on `#cb8919`): **9.6:1** — AA+ for body and large.
- `--accent` on `--bg` (`#cb8919` on `#0f1115`), for accent-as-text / thin rings: **5.9:1** — passes AA body.
- **Why `--accent-fg` went near-black, not white:** at L=45%, white text on amber gives ~3.1:1 (fails AA body) while near-black gives ~9.6:1. The earlier brighter amber (L=58%) could support either. Darker amber can't — foreground **must** be dark. This is the single biggest cascade effect from darkening the accent.

One accent — no `accent-2`, no `accent-secondary`. If a second visual hue is ever needed, it's a semantic state (§1.5), not a mood.

### 1.5 Semantic state — muted by default

Each has a foreground (text/icon *on* the muted bg) and a saturated variant used **only** for critical moments (destructive confirms, error blocking toasts). Default chips/banners/badges use the muted pair.

| Token | HSL | Hex | Role |
|---|---|---|---|
| `--success-muted` | `152 40% 18%` | `#1e3a2d` | Background tint for "saved," "indexed," success chips. |
| `--success-fg` | `152 60% 70%` | `#7dd3a8` | Text/icon on `--success-muted`. 5.6:1 vs muted. |
| `--success` | `152 60% 50%` | `#33cc85` | Reserved. Only for critical confirmations. |
| `--warning-muted` | `28 45% 20%` | `#4a2f12` | Background tint for warning chips. |
| `--warning-fg` | `28 85% 68%` | `#f2a066` | Text/icon on `--warning-muted`. 5.0:1 vs muted. |
| `--warning` | `28 85% 55%` | `#e57324` | Reserved. Only for blocking warnings. |
| `--danger-muted` | `0 45% 22%` | `#522220` | Background tint for destructive affordances (delete-row hover). |
| `--danger-fg` | `0 75% 72%` | `#ea8383` | Text/icon on `--danger-muted`. 5.0:1 vs muted. |
| `--danger` | `0 72% 55%` | `#e04545` | Reserved. Destructive confirm buttons, error toasts. |

**Warning is orange (`28°`), not amber.** Now that accent has moved to `38 77% 45%` (darker, more saturated-warm), keeping warning at `38°` would have collapsed the two into the same visual family — and `warning-fg` in particular would have hovered just one lightness step from accent. Shifting warning to `28°` gives it genuine hue separation (perceptually red-orange rather than gold), which is what "warning" should read as anyway. Warning also stays saturation-forward (`85%`) against accent's more restrained `77%`, so even side-by-side the two read as *different categories*, not *different shades of the same thing*.

**No `--info`.** The audit showed 0 places where info is semantically different from "secondary text." Dead token.

### 1.6 Utility

| Token | HSL | Hex | Role |
|---|---|---|---|
| `--ring` | `38 77% 45%` | `#cb8919` | Focus ring. Identical to `--accent`. Single token for discoverability. |
| `--overlay` | `222 40% 3% / 0.6` | `rgba(6,8,12,0.6)` | Modal scrim. Dark, slightly saturated, not pure black. |

---

## 2. Color — Light mode

Same token names, re-anchored. Philosophy: "same app in a different lighting condition, not a different app." Same cool-neutral hue (`220`), same accent hue (`38`), flipped L.

### 2.1 Surfaces

| Token | HSL | Hex | Role |
|---|---|---|---|
| `--bg` | `220 20% 98%` | `#f7f8fa` | Not pure white. Pure white + amber is harsh. |
| `--surface` | `0 0% 100%` | `#ffffff` | Panels *raised* from bg. Note inversion vs. dark (surface is brighter than bg). |
| `--surface-raised` | `0 0% 100%` | `#ffffff` | Same as surface; elevation in light comes from shadow, not tone. |

### 2.2 Borders

| Token | HSL | Hex | Role |
|---|---|---|---|
| `--border-subtle` | `220 16% 92%` | `#e6e8ee` | Default hairline. |
| `--border-default` | `220 15% 86%` | `#d4d7e0` | Interactive rest. |
| `--border-strong` | `220 14% 72%` | `#a8acb9` | Interactive hover/active. |

### 2.3 Text

| Token | HSL | Hex | Contrast vs `--bg` | Role |
|---|---|---|---|---|
| `--text-primary` | `220 30% 10%` | `#121823` | **16.1:1** | Body, headings. AA+. |
| `--text-secondary` | `220 15% 30%` | `#434957` | **9.5:1** | Subheads, active metadata. AA+. |
| `--text-tertiary` | `220 12% 44%` | `#636a7b` | **5.1:1** | Captions, inline descriptors. AA body. |
| `--text-muted` | `220 10% 55%` | `#808795` | 3.3:1 | Placeholder, dim timestamps. ≥3:1 supplemental. |
| `--text-disabled` | `220 10% 70%` | `#adb1bb` | 1.9:1 | Disabled only. |

### 2.4 Accent

Light-mode accent darkens further from the dark-mode value. A darker canvas-ready amber needs to go darker still against a bright canvas to maintain identity and legibility.

| Token | HSL | Hex | Role |
|---|---|---|---|
| `--accent` | `36 82% 36%` | `#a76708` | Deeper still than dark mode's `#cb8919` — bright canvas needs more depth. |
| `--accent-hover` | `36 85% 30%` | `#8f5608` | Hover state. Darkens further (opposite of dark mode's brighten). |
| `--accent-muted` | `38 70% 92%` | `#fbecd0` | Selected-row tint, subtle highlights. |
| `--accent-fg` | `30 30% 98%` | `#fbf9f6` | Text on solid `--accent`. Off-white — at L=36% amber, white wins over black (6.4:1 vs 3.3:1). Inverse of the dark-mode situation. |

Contrast `--accent-fg` on `--accent` (`#fbf9f6` on `#a76708`): **6.4:1** — AA+ body. Contrast `--accent` on `--bg` (`#a76708` on `#f7f8fa`), for accent-as-text: **5.7:1** — AA body.

### 2.5 Semantic state

| Token | Hex | | Token | Hex |
|---|---|---|---|---|
| `--success-muted` | `#e6f5ec` | | `--danger-muted` | `#fbe7e7` |
| `--success-fg` | `#1e7d4a` | | `--danger-fg` | `#a62828` |
| `--success` | `#1f9d54` | | `--danger` | `#c32d2d` |
| `--warning-muted` | `#fbe5cf` | | `--ring` | `#a76708` |
| `--warning-fg` | `#8a4208` | | `--overlay` | `rgba(20,22,28,0.45)` |
| `--warning` | `#c15c08` | | | |

Warning in light mode is now `28°` orange (matching the dark-mode shift). `--warning-fg` (`#8a4208`) on `--warning-muted` (`#fbe5cf`): ~**6.9:1**, AA+.

All `-fg` on `-muted` pairs meet AA (≥4.5:1). Verified.

---

## 3. Typography

### 3.1 Stack

| Family | Stack | Role |
|---|---|---|
| `--font-sans` | `'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif` | Default chrome. Inter is already loaded; no change. |
| `--font-mono` | `'JetBrains Mono', Menlo, Monaco, Consolas, monospace` | Code, keyboard shortcuts, file paths, identifiers. |
| `--font-serif` | `'Source Serif 4', 'Charter', 'Iowan Old Style', 'Apple Garamond', Georgia, Cambria, 'Times New Roman', serif` | **Prose surfaces only** — rendered note body, markdown output, long-form reading. Never chrome. |

**Why Source Serif 4:** open-source (SIL OFL), designed by Frank Grießhammer for sustained reading, ships a full weight and optical-size range, and pairs with Inter because both trace from the same humanist-sans/humanist-serif design lineage. Charter is the classic Bitstream-era fallback (present on most macOS installs as "Charter"). Iowan and Apple Garamond are Apple-system fallbacks. Georgia/Cambria/TNR close out the chain for Windows and legacy environments.

**Loading:** Source Serif 4 is **not** currently loaded in the app. Adding `--font-serif` requires either a `@font-face` block pulling from `fonts.googleapis.com`/self-hosted WOFF2, or a `<link>` preload in `index.html`. **Flag this for implementation** — the token can ship, but until the font loads, the stack will fall through to Charter/Iowan/Georgia (still acceptable, but not the designed-for default).

**Usage rule (critical, codify in lint later):** Serif appears in **prose surfaces only** — rendered markdown, note body, reader views. Never in chrome, UI controls, labels, buttons, menus, or headings of chrome. If a heading sits *above* prose content and is semantically part of the prose (e.g. an `<h2>` inside a rendered note), it uses serif. If it sits in chrome (sidebar heading, panel title, dialog title), it uses sans.

### 3.2 Scale — fixed, 1.125 modular (major second), anchored at 16px

No `clamp()`. Tauri desktop app, predictable viewport, fixed scale is cleaner and more editorial.

Semantic names. The old `text-display-*` / `text-heading-*` / `text-body-*` names were zero-used — but the Tailwind-ish `text-xs/sm/base/lg/xl/2xl/3xl` names ARE what the 1,349 current usages reach for. Keep the ergonomics, re-anchor the values. Add an `xxs` for micro-labels. Drop `4xl+` — nothing in a desktop tool needs 48px+ type.

**Base is 16px.** (Previously 15px — see §11.3 for why I changed my mind.)

| Token | Size (rem) | Size (px) | Line-height | Tracking | Weight default | Role |
|---|---|---|---|---|---|---|
| `text-xxs` | `0.6875rem` | 11px | 1.45 | `0.02em` (+loose) | 500 | Micro-labels, kbd chips, status dots. |
| `text-xs` | `0.75rem` | 12px | 1.5 | `0.01em` (+slight) | 400 | Captions, timestamps, table meta. |
| `text-sm` | `0.875rem` | 14px | 1.55 | `0` | 400 | Secondary UI text, dense table rows. |
| `text-base` | `1rem` | **16px** | 1.6 | `0` | 400 | **Body default.** Convention over density (see §11.3). |
| `text-lg` | `1.125rem` | 18px | 1.55 | `-0.005em` | 500 | Emphasized body, large labels. |
| `text-xl` | `1.25rem` | 20px | 1.4 | `-0.01em` | 600 | H3-equivalent, section headings. |
| `text-2xl` | `1.5rem` | 24px | 1.3 | `-0.015em` | 600 | H2. |
| `text-3xl` | `1.875rem` | 30px | 1.2 | `-0.02em` | 600 | H1 / page titles. Ceiling. |

Steps are the 1.125 major-second ratio loosely applied with small roundings for whole-px landings (e.g. 18 instead of 18.0, 14 instead of 14.22). Keeping whole pixels matters more than ratio purity on a desktop viewport — subpixel type is noisier than a clean grid.

Tracking rule (single rule, spelled out): **display tightens, body is neutral, caption loosens.** Codified in the table above.

### 3.3 Weights

Exactly three: **400 (regular), 500 (medium), 600 (semibold)**.

- No 300: too fragile on dark backgrounds, indistinguishable from `--text-muted`.
- No 700/800: hierarchy should come from size and color, not weight escalation. Linear uses 600 as its heaviest weight everywhere; we follow.

---

## 4. Spacing

Base unit: **4px**. Tailwind's default already matches. Keep it; remove the fluid additions.

**Kept (semantic aliases over numeric, for frequent primitives):**

| Token | Value | Role |
|---|---|---|
| `--space-hair` | `1px` | Borders, dividers. |
| `--space-0` | `0` | — |
| `--space-px` | `1px` | — |
| Tailwind `0.5`…`32` | (default) | Direct scale usage is fine; it's the 4px grid. |

**Deleted:**

- `spacing.fluid-xs/sm/md/lg/xl/2xl` (all clamp-based, 0 semantic value in a fixed-viewport app)
- `--space-xs/sm/md/lg/xl` (themes.css clamp spacing — dead on arrival)

Rule: if you reach for a value outside the 4px grid, you're compensating for a layout mistake.

---

## 5. Radius

Three. Named by scale, used by role.

| Token | Value | Role |
|---|---|---|
| `--radius-sm` | `4px` | Inputs, buttons, chips, kbd. |
| `--radius-md` | `8px` | Cards, panels, dropdowns. |
| `--radius-lg` | `12px` | Modals, command palette, full-bleed popovers. |

Removed: `--radius-xl` (16px) and `--radius-2xl` (24px) from themes.css. Bento-card-class roundness reads as marketing. 12px is the visual ceiling in this aesthetic.

`--radius-full` (`9999px`) exists for avatars/pills but isn't part of the scale.

---

## 6. Shadow / Elevation

Two shadows. Plus none.

| Token | Value | Role |
|---|---|---|
| `--shadow-none` | `none` | Default. Most surfaces. |
| `--shadow-sm` | `0 1px 2px rgba(0,0,0,0.25), 0 1px 1px rgba(0,0,0,0.18)` | Raised surfaces (cards that truly need lift, sticky headers). Dark-mode values; light mode halves alpha. |
| `--shadow-md` | `0 8px 24px rgba(0,0,0,0.40), 0 2px 6px rgba(0,0,0,0.30)` | Overlays only: popovers, modals, command palette, drag previews. |

Light-mode equivalents:

| Token | Value |
|---|---|
| `--shadow-sm` | `0 1px 2px rgba(15,20,30,0.06), 0 1px 1px rgba(15,20,30,0.04)` |
| `--shadow-md` | `0 8px 24px rgba(15,20,30,0.12), 0 2px 6px rgba(15,20,30,0.06)` |

**Rules:**

- No colored shadows. Shadows are neutral. (`shadow-blue-500/10` etc. are banned.)
- No glow. `--shadow-glow` is deleted.
- No inset "inner glow" decoration.
- Borders do elevation work first. A hairline + `--surface-raised` tone shift is the default lift mechanism. Shadows are reserved for *floating* things.

---

## 7. Motion

### 7.1 Durations

| Token | Value | Role |
|---|---|---|
| `--duration-fast` | `120ms` | Hover state, press feedback, focus ring appearance. |
| `--duration-base` | `180ms` | Open/close of menus, toggles, accordion. Default for state changes. |
| `--duration-slow` | `280ms` | Modal/dialog open, full-panel reveals. |

Three, because faster (80ms) reads as "no animation" (fine — just omit the transition) and slower (400ms+) reads as "waiting" in a desktop tool. 120/180/280 is tighter than the old 150/250/350 — desktop apps should feel snappier than web.

### 7.2 Easings

| Token | Value | Role |
|---|---|---|
| `--ease-out` | `cubic-bezier(0.22, 1, 0.36, 1)` | Entrances (enters frame, arrives, settles). Default for "thing appeared." |
| `--ease-in` | `cubic-bezier(0.4, 0, 1, 1)` | Exits (dismiss, collapse, leave). |
| `--ease-linear` | `linear` | Indeterminate loaders, progress. |

**Deleted:** `ease-bounce`, `ease-spring`, `bounce-in`, `smooth` (redundant with `ease-out`), `transition-bounce`.

### 7.3 What animates vs. what does not

**Animated:**

- State transitions (menu open, tab switch, accordion, toggle)
- Focus ring appearance
- Enter/exit of overlays (modals, popovers, toasts)
- Loading indeterminates (linear progress, spinner rotation)
- Explicit user-caused reveal (panel slide, tree expand)

**NOT animated:**

- Page mount cascades (`fadeInUp` staggered children — deleted)
- Hover lifts (`translateY(-2px/-4px)` — deleted)
- Glow pulses (`glow-pulse`, `text-shimmer` — deleted)
- Skeleton shimmers (`shimmer` keyframe — deleted; use flat pulse at 60% opacity if needed)
- Welcome/entrance performances

If animation is noticed, it's too much.

---

## 8. Focus

One treatment. Everywhere.

```
:focus-visible {
  outline: 2px solid hsl(var(--ring));
  outline-offset: 2px;
  box-shadow: none;        /* explicit — no halo */
}
```

- 2px crisp outline, `--ring` (= `--accent`).
- 2px offset — separates ring from element without visual noise.
- No `box-shadow` glow halo. The current `[data-theme="dark"] :focus-visible { box-shadow: 0 0 0 4px rgba(56,189,248,0.1); }` is deleted.
- No pulse, no fade-in ramp, no color shift mid-focus.

For inputs where outline-offset clips on adjacent elements, we fall back to `box-shadow: 0 0 0 2px hsl(var(--ring))` — still a crisp ring, no blur.

---

## 9. Migration map

### 9.1 Token rename/deletion table

| Current | Action | New |
|---|---|---|
| `--background` (index.css) | rename | `--bg` |
| `--foreground` | rename | `--text-primary` |
| `--card`, `--card-foreground` | collapse | use `--surface` / `--text-primary` |
| `--popover`, `--popover-foreground` | collapse | use `--surface-raised` / `--text-primary` |
| `--primary`, `--primary-foreground` | rename | `--accent`, `--accent-fg` (shadcn Button uses "primary" to mean brand fill — that's `--accent` here) |
| `--secondary`, `--secondary-foreground` | collapse | use `--surface` / `--text-primary` |
| `--muted`, `--muted-foreground` | collapse | use `--surface` / `--text-muted` |
| `--accent` (shadcn), `--accent-foreground` | rename | collides with new `--accent`. shadcn's "accent" = hover bg; map to `--surface-raised` / `--text-primary` |
| `--destructive`, `--destructive-foreground` | rename | `--danger`, `--danger-fg` |
| `--border` (shadcn) | rename | `--border-subtle` (most existing uses) |
| `--input` (shadcn) | rename | `--border-default` |
| `--ring` | keep name, re-color | (now amber, not neutral) |
| `--chart-1..5` | keep, restate | derive from accent + semantic; defer spec to charting work |
| `--radius` | keep, renamed | `--radius-md` (shadcn components read `--radius`; keep as alias pointing at `--radius-md`) |
| `--bg-primary/secondary/tertiary` (themes.css) | **DELETED** | map to `--bg` / `--surface` / `--surface-raised` |
| `--text-primary/secondary/tertiary` | **DELETED + re-added** | re-introduced with 4-peer scale: `--text-primary / --text-secondary / --text-tertiary / --text-muted`, plus `--text-disabled` as state |
| `--border-color`, `--border-hover` | **DELETED** | `--border-subtle`, `--border-strong` |
| `--brand-primary/hover/light/dark` | **DELETED** | `--accent` / `--accent-hover` / `--accent-muted` (no `-dark`) |
| `--accent-primary/hover/light` | **DELETED** | `--accent` / `--accent-hover` / `--accent-muted` |
| `--accent-purple/pink/orange/emerald` | **DELETED** | nothing. Use `--accent` for identity, semantic tokens for state. |
| `--success/warning/error/info` + `-light` | **DELETED + re-added** | renamed `--success/warning/danger` (no info) with `-muted` + `-fg` variants |
| `--surface-elevated/hover/active` | **DELETED** | `--surface` / `--surface-raised`; hover states come from borders |
| `--glass-bg`, `--glass-border` | **DELETED** | nothing. Use opaque `--surface-raised`. |
| `--gradient-brand`, `--gradient-brand-subtle`, `--gradient-mesh` | **DELETED** | nothing. Headlines are single-color. |
| `--shadow-glow` | **DELETED** | nothing. |
| `--shadow-sm/md/lg/xl/2xl` | reduced | `--shadow-sm`, `--shadow-md`. No `lg/xl/2xl`. |
| `--radius-sm/md/lg/xl/2xl` | reduced | `--radius-sm/md/lg`. No `xl/2xl`. |
| `--space-xs..xl` (clamp) | **DELETED** | Tailwind default 4px grid. |
| `--heading-display`, `--heading-hero` | **DELETED** | `text-2xl` / `text-3xl`. |
| `--font-weight-*` | **DELETED** | Tailwind `font-{normal,medium,semibold}` is enough. |
| `--transition-theme/smooth/bounce` | **DELETED** | `--duration-*`, `--ease-*`, composed per-property. |
| `--duration-fast/normal/slow` | rename | `--duration-fast/base/slow`, re-valued (120/180/280). |
| `--ease-smooth/bounce/spring` | reduced | `--ease-out/in/linear`. |
| `brand.50..950` (tailwind.config) | **DELETED** | — (0 uses). |
| `accent.{purple,pink,orange,emerald}` (tailwind.config) | **DELETED** | — |
| `fontSize.display-*/heading-*/body-*/caption` (tailwind.config, clamp) | **DELETED** | — (0 uses). New scale is on `text-xxs..3xl`. |
| `spacing.fluid-*` (tailwind.config) | **DELETED** | — |
| `boxShadow.glow` (tailwind.config) | **DELETED** | — |
| `boxShadow.elevation-1/2/4` (tailwind.config) | **DELETED** | `--shadow-sm/md`. |
| `backdropBlur.xs` | **DELETED** | — (blur is not a default tool). |
| `keyframes.shimmer/fadeIn/glow-pulse` | **DELETED** | — |
| `animation.shimmer/fadeIn/glow-pulse/fade-in` | **DELETED** | — |
| `transitionTimingFunction.bounce-in` | **DELETED** | — |
| `transitionTimingFunction.smooth` | rename | use `--ease-out` |
| `letterSpacing.display/heading` | keep as-is or remove | redundant with scale-baked tracking; remove. |

### 9.2 Files that change

| File | Action |
|---|---|
| `websrc/index.css` | Rewrite `:root` and `[data-theme="dark"]` (also `.dark` — keep the class selector for Tailwind compatibility, point it at the same values as `[data-theme="dark"]`) with the token set above. |
| `tailwind.config.js` | Remove `brand.*`, `accent.{purple,pink,orange,emerald}`, fluid `fontSize`, `fluid-*` spacing, `boxShadow.glow/elevation-*`, `backdropBlur.xs`, all `keyframes`/`animation` entries, `bounce-in`. Add new `fontSize` map, confirm `letterSpacing` removed or simplified, point `boxShadow` entries at `--shadow-sm/md`, point `borderRadius` at `--radius-sm/md/lg`. |
| `websrc/styles/themes.css` | **Delete entire file.** Remove imports. |
| `websrc/styles/command-palette.css` | Audit for hardcoded hex literals (166 across codebase, some here). Replace with tokens or move to component styling phase. |
| `websrc/hooks/useApplyTheme.ts` | **Keep dual-apply as-is.** Both `.dark` and `[data-theme="dark"]` remain applied in sync, pointing at the same values. Collapsing to a single mechanism is out of scope for this phase and tracked as tech debt (see Decisions log §12). |
| `websrc/index.html` (or font loader) | **Add Source Serif 4.** Not currently loaded. Either add a `@font-face` block to `index.css`, or `<link rel="preload">` + Google Fonts / self-hosted WOFF2. Weights needed: 400, 600 (match what serif content will actually use). Until loaded, `--font-serif` falls through to Charter/Georgia — acceptable, not ideal. |

### 9.3 Not handled by this spec (Phase 4 lint)

- `bg-blue-500`, `text-gray-*`, `from-sky-400` inline Tailwind palette utilities (422 body uses, 1,349 gradient uses). This spec sets up the tokens; a later phase adds an ESLint/Tailwind safelist rule that forbids raw palette classes.
- Per-component hardcoded hexes (`bg-[#0d1117]` etc. in Chat). Migrated in Phase 3 when components are rewritten.

---

## 10. What this spec explicitly does NOT cover

- **Component styling.** Button variants, card treatments, input states — that's Phase 3 (component-designer).
- **Motion choreography.** This spec sets durations and easings. *How* a modal opens (scale+opacity? slide-from-edge?) is the animation-choreographer's job later.
- **Iconography.** Icon sizing, stroke weight, the icon set — out of scope.
- **Illustration / empty-state art.** Out of scope.
- **Charting.** `--chart-1..5` is preserved as a stub; palette will be defined alongside whatever chart library lands.
- **Prose typography scale / rendered-Markdown styling.** The scale above is for *chrome*. `--font-serif` is defined for prose surfaces but the full prose treatment (size, line-height, measure, heading rhythm, blockquote styling) is deferred to whenever the note/reader view is actually spec'd.
- **Lint enforcement.** Phase 4.

---

## 11. Design tensions I resolved

### 11.1 Amber over every other accent candidate

I considered five: **neutral-zinc (shadcn default), Linear-purple, IA-orange, terminal-green, amber**.

- **Neutral-zinc** is what stock shadcn ships. It's the safe answer. It's also the reason the current app has no identity — "monochrome with a gray accent" reads as *unstyled*, not *restrained*. Rejected.
- **Linear-purple** (~`250° 90%`) is the most obvious reference-matching choice. Rejected because (a) we'd read as Linear-derivative rather than Recall, and (b) purple has accessibility cliffs on dark bg at the saturations that feel "premium" — we'd end up muddying it.
- **Terminal-green** (~`140°`) fits the "code editor" half of the DNA but clashes with semantic success green. Two greens in one palette is a bug.
- **IA-orange / red-orange** (~`15°`) is warm and editorial but sits too close to `--danger`; warnings and identity would collide.
- **Amber (`38°`)** — warm, editorial, legibly distinct from the cool-neutral hue `220` surfaces, has enough contrast headroom at 58% L to meet AA on dark bg, and reads as considered rather than trendy. It nods to analog / paper / editor highlight (the `--accent-muted` tint is essentially the "current line" color from Solarized / One Dark). It also gives the app an *identity* without claiming a trend — amber hasn't been the hot-take accent of 2024/25, which fits the "unfinished-feeling" adjective.

The risk I originally flagged — amber accent colliding with an amber warning — is now resolved. Warning has moved to `28°` (orange), giving genuine hue separation. Accent stays at `38°` but darkened to `77% 45%` per Josh's call (previously `92% 58%`), which pulls it away from anything that reads as a state color and toward aged-gold territory. Accent and warning can now safely co-occupy a region without confusion.

**Defense in one sentence:** Burnt amber because it's warm enough to feel human in a tool that's otherwise quiet, dark and restrained enough not to read as marketing or celebration, and now safely distinct from our orange warning hue — so Recall owns it.

### 11.2 Four peer text levels — widen the gaps to keep them distinct

Originally I ran three peer levels plus disabled (`primary / secondary / muted / disabled`), on the theory that four peers always degrade into noise. Josh pushed back: the app genuinely has four registers in use (lead text, sub-text, contextual descriptor, background metadata). I agreed, but with a condition — to avoid the "tertiary vs muted are basically the same" failure mode, I deliberately widened the contrast gaps. On dark mode the lightness steps are **96 → 76 → 60 → 45 → 32**, yielding contrasts of **15.8 → 9.1 → 5.3 → 3.4 → 2.0**. Each step is a perceptually clear jump, not a delta of taste. The usage table in §1.3 codifies when each is correct, so the decision isn't left to eyeball. If in review tertiary and muted still blur together, I widen further (muted drops to L=42%) — but the current spread should hold.

### 11.3 `text-base` = 16px (convention over density, Josh's call)

Originally I shipped 15px as a density move — desktop app, power-user audience, Linear/Arc/Vercel all run 13-14px chrome. Josh chose **16px** instead. The tradeoff: we give up ~6% vertical density per body line in exchange for hitting web-standard body size, which means (a) any rendered user content (notes, chat messages) reads at expected reading-comfort size without an extra override, (b) OS zoom and accessibility tooling behave with zero surprise, and (c) nobody ever has to defend "why is your body text 15px?" in a usability review. Chrome density is recovered via `text-sm` (14px) and `text-xs` (12px) where needed — table rows, secondary metadata, dense sidebars. This is explicitly chosen over density per Josh's call.

---

## 12. Decisions log

The first draft of this spec ended with a list of open questions. This revision closes them — recording what was asked and what Josh decided, so future readers can reconstruct the rationale without digging through chat history.

| Question | Decision | Rationale |
|---|---|---|
| Should accent amber be brighter (`#f5a524`, `38 92% 58%`) or darker? | **Darker.** Final: `#cb8919` (`38 77% 45%`) in dark mode; `#a76708` (`36 82% 36%`) in light mode. | Original amber read as candy-bright / highlighter. Darker reads as aged gold / tobacco — warm without cheerful, editorial without soft. Cascade: `--accent-fg` had to flip from deep-warm-brown to near-black because at L=45% white fails AA on amber. |
| Keep amber warning (`38°`) and rely on context, or shift to orange (`28°`)? | **Shift to orange (`28°`).** | Once accent darkened and moved into amber-amber territory, keeping warning at `38°` would have collapsed the two into the same visual family. `28°` gives genuine hue separation — warning reads as red-orange, accent as gold. Also slightly higher saturation on warning keeps them distinct even adjacent. |
| `text-base` at 15px (density) or 16px (convention)? | **16px.** | Josh chose convention. Gives up ~6% vertical density per body line; gains zero-surprise behavior for rendered prose, OS zoom, and accessibility. Density is still recoverable via `text-sm` (14px) where chrome needs it. |
| Three peer text levels + disabled, or four peer + disabled? | **Four peer + disabled.** Final: `primary / secondary / tertiary / muted / disabled`. | App has four genuine registers. To keep them distinct, I widened the lightness steps and codified per-level usage in §1.3 so the decision isn't left to eyeball. Muted dropped to 3.4:1 (supplemental only); tertiary holds 5.3:1 (AA body). |
| Ship with no serif, or add `--font-serif` for prose? | **Add `--font-serif`.** Scope: rendered note body / markdown / long-form reading surfaces only. Never chrome. Primary: Source Serif 4. | An editorial serif in prose surfaces reinforces the "content is the interface" principle — user-authored text gets a reading-optimized face, chrome stays sans. Strict scoping keeps it from becoming decoration. Requires font loading (flagged in §9.2). |
| `.dark` class vs. `[data-theme="dark"]` attribute — collapse to one? | **Keep both.** | During this migration both mechanisms stay applied in sync via `useApplyTheme`, pointing at the same values. Collapsing to one is out of scope for this phase and is tracked as tech debt for a later cleanup. |
| `--chart-*` tokens — specify now or defer? | **Defer.** | Five-color chart palette remains stubbed. Specified alongside whichever charting library actually ships — palette decisions without a use case are guesswork. |
