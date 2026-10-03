# Lattice Tokens — Specification

**Status:** adopted. Token names and values below match `src/index.css` and `tailwind.config.js` as of 2026-10-02. Where the shipped values moved away from the first draft (warm neutrals, a copper accent, more surface, shadow and radius steps), the tables give the shipped value; §11–§12 keep the draft's reasoning as history.
**Supersedes:** the old shadcn block in `src/index.css`, `src/styles/themes.css` (deleted), and in `tailwind.config.js` the `brand.*` and `accent.{purple,pink,orange,emerald}` colors, fluid `fontSize`, `fluid-*` spacing, glow shadow, bounce easing, and shimmer/fadeIn/glow-pulse animations
**Scope:** Token primitives only. No component styling, no motion choreography, no iconography.

---

## 0. Shape of the system

- **One layer of truth.** HSL channel variables in `src/index.css`: light values in `:root`, dark values in `.dark, [data-theme="dark"]` (both set on `<html>` by `useApplyTheme`). `tailwind.config.js` exposes them as colors via `hsl(var(--token))`. Border tokens carry their own alpha, so Tailwind's `/nn` opacity modifier is ignored on them. No second layer.
- **One-temperature neutrals.** Surfaces/borders/text share one warm-neutral hue family (HSL `30`–`42`) so the whole UI sits in one temperature. No warm/cool drift between scales.
- **Dark-first.** Dark mode is designed first (§1); light is an explicit derivation (§2), even though the CSS lists light in `:root`.
- **Semantic names only.** No `gray-500`, no `brand-500`. If a token can't be named by its role, it doesn't belong in the system.
- **Deletion is load-bearing.** The `brand.*` scale, the `accent.{purple,pink,orange,emerald}` literals, the `fluid-*` spacing, the `text-{display,heading,body}-*` clamp scale, `shadow-glow`, `ease-bounce`, `bounce-in`, `shimmer`, `glow-pulse`, `fadeIn`, and every utility in `themes.css` (§9) are removed as part of adopting this spec.

---

## 1. Color — Dark mode (primary)

Neutrals are in HSL hue `30` (warm gray). Contrast ratios below are measured against `--bg` unless noted.

### 1.1 Surfaces — a ladder

The first draft cut this to three layers (`bg` / `surface` / `raised`). The shipped ladder has six rungs; elevation is lightness, each rung a little lighter and a little warm.

| Token | HSL | Hex | Contrast vs `--bg` | Role |
|---|---|---|---|---|
| `--chrome` | `30 6% 7.5%` | `#141312` | 1.08:1 | Window chrome behind the rail and page (`bg-chrome`). Recedes. |
| `--surface-sunken` | `30 6% 8.5%` | `#171614` | 1.05:1 | Wells set *below* the page. |
| `--bg` | `30 5% 10.5%` | `#1c1b19` | — | App canvas. The dominant color. |
| `--surface` | `30 5% 13%` | `#23211f` | 1.08:1 | Panels, sidebars, cards. One step above canvas. |
| `--surface-overlay` | `30 5% 15.5%` | `#2a2826` | 1.17:1 | Tooltips and other floating content. |
| `--surface-raised` | `30 5% 16.5%` | `#2c2a28` | 1.21:1 | Hover and selected fills, popovers, menus. |

Delta between steps is ~2–3% L — visible as a tonal shift, not a contrast jump. Matches "depth is a whisper."

### 1.2 Borders — 3 levels

Borders are paper-colored at low alpha, so a hairline reads the same on every rung of the ladder.

| Token | HSL / alpha | Role |
|---|---|---|
| `--border-subtle` | `36 30% 92% / 0.07` | Default hairline. Table rules, dividers, card outlines. Also the `*` default border color. |
| `--border-default` | `36 30% 92% / 0.12` | Interactive element rest state: inputs, buttons, kbd chips. |
| `--border-strong` | `36 24% 90% / 0.24` | Hover/active on interactive elements. Selected state. |

Three tiers because "typography carries hierarchy, not borders" but borders still do real work: resting vs. interactive vs. engaged. Anything more granular = decoration.

### 1.3 Text — 4 peer registers + 1 state

Four peer registers (`primary / secondary / tertiary / muted`) plus `disabled` as a state-only level.

| Token | HSL | Hex | Contrast vs `--bg` | Role |
|---|---|---|---|---|
| `--text-primary` | `36 18% 91%` | `#ece9e4` | **14.2:1** | Body, headings, primary content. AA+. |
| `--text-secondary` | `35 9% 74%` | `#c3beb7` | **9.3:1** | Supporting copy, subheads, active metadata. AA+. |
| `--text-tertiary` | `32 6% 58%` | `#9a948d` | **5.8:1** | Inline labels, captions, contextual descriptors. AA body. |
| `--text-muted` | `32 5% 57%` | `#97928c` | 5.6:1 | Timestamps, hints, placeholder-ish info. Supplemental only, never load-bearing. |
| `--text-disabled` | `30 5% 34%` | `#5b5752` | 2.4:1 | Disabled state only. Fails AA by design — never carries meaning. |

Four peer registers because the app genuinely has four (see §11.2). The rule: each step is a visible jump, no fuzzy middle, and `muted` must not collapse into `tertiary`. As shipped the steps are **14.2 → 9.3 → 5.8 → 5.6 → 2.4**.

**When to use each:**

| Register | Use for | Don't use for |
|---|---|---|
| `primary` | Body prose, headings, editable field values, selected item | Secondary metadata, hints |
| `secondary` | Subheads, table column headers, active filter labels, current-nav label | Timestamps, de-emphasized info |
| `tertiary` | Inline descriptors ("2 items · edited 3d ago"), form field helper text, breadcrumb parents | Critical info |
| `muted` | Placeholder text, dim timestamps, icon-only affordance labels, background metadata | Anything a user must read |
| `disabled` | Disabled controls only | Anything that needs to be read but de-emphasized |

### 1.4 Accent — one hue

**Shipped:** **Copper** — HSL `22 58% 72%`, hex **`#E1AD8E`** in dark mode (`12 52% 40%`, `#9B4631` in light). The first draft chose violet; §11.1 and §12 keep that reasoning as history.

| Token | HSL | Hex | Role |
|---|---|---|---|
| `--accent` | `22 58% 72%` | `#E1AD8E` | Primary accent. Focus rings, selection, links. |
| `--accent-hover` | `22 62% 79%` | `#EBC1A8` | Hover/pressed. +7% L — conventional brighten on dark UI. |
| `--accent-muted` | `22 28% 18%` | `#3B2A21` | Subtle accent tint (selected-row bg, current-line highlight). Same hue, much lower saturation + lightness. |
| `--accent-fg` | `30 8% 9%` | `#191715` | Text *on* `--accent` solid fill. Near-black (see below). |

The primary button is **not** accent-filled: it uses the `--action` ink tokens (§1.6).

**Contrast notes — read these, they matter:**

- `--accent-fg` on `--accent` (`#191715` on `#E1AD8E`): **9.0:1** — AA body.
- `--accent` on `--bg` (`#E1AD8E` on `#1c1b19`), for accent-as-text / thin rings: **8.7:1** — passes AA body.
- **Why `--accent-fg` is near-black, not white:** a light accent on a dark canvas needs dark text. Foreground **must** be dark in dark mode. Polarity flips in light mode where the accent darkens to L=40% and off-white wins.

**Discipline cost (noted from shootout):** the accent wants to expand. Restrict to focus rings, selected-state borders, link text, and ≤32px solid surfaces. No large fills, no gradients, never a panel bg. The token system can't prevent misuse — review and lint have to.

One accent — no `accent-2`, no `accent-secondary`. If a second visual hue is ever needed, it's a semantic state (§1.5), not a mood.

### 1.5 Semantic state — muted by default

Each has a foreground (text/icon *on* the muted bg) and a saturated variant used **only** for critical moments (destructive confirms, error blocking toasts). Default chips/banners/badges use the muted pair.

| Token | HSL | Hex | Role |
|---|---|---|---|
| `--success-muted` | `152 36% 15%` | `#183427` | Background tint for "saved," "indexed," success chips. |
| `--success-fg` | `152 55% 68%` | `#81dab0` | Text/icon on `--success-muted`. 8.1:1 vs muted. |
| `--success` | `152 55% 48%` | `#37be7f` | Reserved. Only for critical confirmations. |
| `--warning-muted` | `32 45% 16%` | `#3b2a16` | Background tint for warning chips. |
| `--warning-fg` | `32 85% 68%` | `#f3b268` | Text/icon on `--warning-muted`. 7.4:1 vs muted. |
| `--warning` | `32 85% 55%` | `#ee932b` | Reserved. Only for blocking warnings. |
| `--danger-muted` | `0 42% 18%` | `#411b1b` | Background tint for destructive affordances (delete-row hover). |
| `--danger-fg` | `0 78% 74%` | `#f08989` | Text/icon on `--danger-muted`. 6.2:1 vs muted. |
| `--danger` | `0 68% 65%` | `#e26969` | Reserved. Destructive confirm buttons, error toasts. |

**Warning is orange (`32°` dark, `28°` light).** It must read as "caution" and never as the accent, and it stays saturation-forward (`85%`) so warning chips read as state-category rather than brand-identity. The copper accent (`22°` dark, `12°` light) sits close to it on the wheel.

**No `--info`.** The audit showed 0 places where info is semantically different from "secondary text." Dead token.

### 1.6 Utility

| Token | HSL | Hex | Role |
|---|---|---|---|
| `--ring` | `22 58% 72%` | `#E1AD8E` | Focus ring. Identical to `--accent`. Single token for discoverability. |
| `--overlay` | `30 20% 3% / 0.62` | — | Modal scrim. Dark, slightly warm, not pure black. |
| `--highlight` | `40 90% 60% / 0.26` | — | `<mark>` background (search and reading highlights). |
| `--action` | `36 22% 90%` | `#ebe7e0` | Primary button fill (`Button` default variant): ink, not accent. |
| `--action-hover` | `0 0% 100%` | `#ffffff` | Primary button hover. |
| `--action-fg` | `30 8% 9%` | `#191715` | Text on `--action`. 14.5:1. |

---

## 2. Color — Light mode

Same token names, re-anchored. Philosophy: "same app in a different lighting condition, not a different app." Same warm-neutral hue family (`30`–`42`), the accent deepened from `22°` to `12°`, flipped L.

### 2.1 Surfaces

A ladder, dimmest first: the chrome recedes, the page is the brightest thing in the window.

| Token | HSL | Hex | Role |
|---|---|---|---|
| `--chrome` | `38 18% 92.5%` | `#efede8` | Window chrome. Dimmest rung. |
| `--surface-raised` | `38 16% 92.5%` | `#efede9` | Hover and selected fills. In light, "raised" is a tint, not a brightening. |
| `--surface-sunken` | `38 16% 94%` | `#f2f0ed` | Wells below the page. |
| `--bg` | `40 26% 96.5%` | `#f8f7f4` | App canvas. Not pure white. |
| `--surface` | `42 45% 98.6%` | `#fdfcfa` | Panels *raised* from bg. Note inversion vs. dark (surface is brighter than bg). |
| `--surface-overlay` | `40 40% 99.4%` | `#fefefd` | Tooltips and floating content; elevation in light comes from shadow, not tone. |

### 2.2 Borders

Ink at low alpha, so a hairline reads the same on every rung.

| Token | HSL / alpha | Role |
|---|---|---|
| `--border-subtle` | `30 18% 18% / 0.09` | Default hairline. |
| `--border-default` | `30 18% 18% / 0.15` | Interactive rest. |
| `--border-strong` | `30 14% 20% / 0.3` | Interactive hover/active. |

### 2.3 Text

| Token | HSL | Hex | Contrast vs `--bg` | Role |
|---|---|---|---|---|
| `--text-primary` | `30 12% 10%` | `#1d1a16` | **16.2:1** | Body, headings. AA+. |
| `--text-secondary` | `30 8% 26%` | `#48423d` | **9.2:1** | Subheads, active metadata. AA+. |
| `--text-tertiary` | `30 6% 40%` | `#6c6660` | **5.3:1** | Captions, inline descriptors. AA body. |
| `--text-muted` | `30 5% 41%` | `#6e6963` | 5.1:1 | Placeholder, dim timestamps. Supplemental. |
| `--text-disabled` | `30 6% 68%` | `#b2ada9` | 2.1:1 | Disabled only. |

### 2.4 Accent

Light-mode accent darkens from the dark-mode value. Copper on a bright canvas needs more depth to maintain identity and legibility.

| Token | HSL | Hex | Role |
|---|---|---|---|
| `--accent` | `12 52% 40%` | `#9B4631` | Darker than dark mode's `#E1AD8E` — bright canvas needs more depth. |
| `--accent-hover` | `12 54% 33%` | `#823927` | Hover state. Darkens further (opposite of dark mode's brighten). |
| `--accent-muted` | `18 52% 92%` | `#F5E6E0` | Pale copper tint. Selected-row, subtle highlights. |
| `--accent-fg` | `40 30% 98%` | `#FBFAF8` | Text on solid `--accent`. Off-white — polarity flips at L=40%: white wins over dark. Inverse of the dark-mode situation. |

Contrast `--accent-fg` on `--accent` (`#FBFAF8` on `#9B4631`): **6.1:1** — AA body. Contrast `--accent` on `--bg` (`#9B4631` on `#f8f7f4`), for accent-as-text: **5.9:1** — AA body.

`--action` is `30 10% 12%` (`#221f1c`), hover `30 8% 22%`, text `40 40% 97%` (15.6:1): near-black ink buttons.

### 2.5 Semantic state

| Token | Hex | | Token | Hex |
|---|---|---|---|---|
| `--success-muted` | `#e5f5ee` | | `--danger-muted` | `#fbe9e9` |
| `--success-fg` | `#1b744a` | | `--danger-fg` | `#a82929` |
| `--success` | `#209d63` | | `--danger` | `#c32c2c` |
| `--warning-muted` | `#fbe8d0` | | `--ring` | `#9B4631` |
| `--warning-fg` | `#8b4609` | | `--overlay` | `hsl(30 20% 8% / 0.32)` |
| `--warning` | `#c46008` | | `--highlight` | `hsl(42 95% 62% / 0.38)` |

Warning in light mode is `28°` orange. `--warning-fg` (`#8b4609`) on `--warning-muted` (`#fbe8d0`): ~**5.9:1**, AA.

All `-fg` on `-muted` pairs meet AA (≥4.5:1). Verified.

---

## 3. Typography

### 3.1 Stack

| Family | Stack | Role |
|---|---|---|
| `--font-sans` | `'Inter Variable', 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI Variable Text', 'Segoe UI', system-ui, sans-serif` | Default chrome. |
| `--font-mono` | `'JetBrains Mono Variable', 'JetBrains Mono', ui-monospace, Menlo, Monaco, Consolas, monospace` | Code, keyboard shortcuts, file paths, identifiers. |
| `--font-serif` | `'Source Serif 4 Variable', 'Source Serif 4', 'Charter', 'Iowan Old Style', 'Apple Garamond', Georgia, Cambria, 'Times New Roman', serif` | **Prose surfaces only** — rendered note body, markdown output, long-form reading. Never chrome. |

**Why Source Serif 4:** open-source (SIL OFL), designed by Frank Grießhammer for sustained reading, ships a full weight and optical-size range, and pairs with Inter because both trace from the same humanist-sans/humanist-serif design lineage. Charter is the classic Bitstream-era fallback (present on most macOS installs as "Charter"). Iowan and Apple Garamond are Apple-system fallbacks. Georgia/Cambria/TNR close out the chain for Windows and legacy environments.

**Loading:** all three faces are self-hosted from the `@fontsource-variable/*` packages, imported in `src/main.tsx` (Inter and Source Serif 4 with optical sizing and italics, JetBrains Mono by weight). The app is offline-first and its CSP refuses remote stylesheets, so no Google Fonts link.

**Usage rule (critical, codify in lint later):** Serif appears in **prose surfaces only** — rendered markdown, note body, reader views. Never in chrome, UI controls, labels, buttons, menus, or headings of chrome. If a heading sits *above* prose content and is semantically part of the prose (e.g. an `<h2>` inside a rendered note), it uses serif. If it sits in chrome (sidebar heading, panel title, dialog title), it uses sans.

### 3.2 Scale — fixed, 1.125 modular (major second), anchored at 16px

No `clamp()`. Tauri desktop app, predictable viewport, fixed scale is cleaner and more editorial.

Semantic names. The old `text-display-*` / `text-heading-*` / `text-body-*` names were zero-used — but the Tailwind-ish `text-xs/sm/base/lg/xl/2xl/3xl` names ARE what the 1,349 current usages reach for. Keep the ergonomics, re-anchor the values. Add an `xxs` for micro-labels. Drop `4xl+` — nothing in a desktop tool needs 48px+ type.

**Base is 16px.** (Previously 15px — see §11.3 for why I changed my mind.)

| Token | Size (rem) | Size (px) | Line-height | Tracking | Weight default | Role |
|---|---|---|---|---|---|---|
| `text-xxs` | `0.6875rem` | 11px | 1.45 | `0.02em` (+loose) | 500 | Micro-labels, kbd chips, status dots. |
| `text-xs` | `0.75rem` | 12px | 1.5 | `0.01em` (+slight) | 400 | Captions, timestamps, table meta. |
| `text-ui` | `0.8125rem` | 13px | 1.4 | `-0.003em` | (inherit) | Chrome text: row names, control labels, buttons. |
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

Named by scale, used by role. Concentric: a child's radius is its parent's minus the padding between them.

| Token | Value | Role |
|---|---|---|
| `--radius-xs` | `3px` | Smallest step (`rounded-xs`); not yet used. |
| `--radius-sm` | `5px` | Inputs, buttons, chips, kbd. |
| `--radius-md` | `8px` | Cards, panels, dropdowns. Also `--radius` (shadcn alias). |
| `--radius-lg` | `12px` | Modals, command palette, full-bleed popovers. |
| `--radius-xl` | `16px` | `rounded-xl`: dialogs, the search field, large tiles. |

Bento-card-class roundness reads as marketing. 12px is the visual ceiling in this aesthetic. (`--radius-2xl` from themes.css is gone; Tailwind's default `rounded-2xl` is still available because the config extends rather than replaces the scale.)

`--radius-full` (`9999px`) exists for avatars/pills but isn't part of the scale.

---

## 6. Shadow / Elevation

The first draft had two shadows plus none. The shipped set has six plus none; light-mode shadows are always two layers (contact + ambient), warm-tinted `rgb(40 28 16 / …)`. Full values live in `src/index.css`; Tailwind exposes each as `shadow-<name>`.

| Token | Role |
|---|---|
| `--shadow-none` | Default. Most surfaces. |
| `--shadow-sm` | Raised surfaces (cards that truly need lift, sticky headers). Dark: `0 1px 2px rgb(0 0 0 / 0.3)`. |
| `--shadow-md` | Floating panels and menus. |
| `--shadow-lg` | Popovers, command palette, tooltips, alerts: the furthest float. |
| `--shadow-sheet` | Side sheets and pinned inputs: a ring plus a soft drop. |
| `--shadow-control` | Bordered controls at rest (secondary buttons, segmented controls). |
| `--shadow-action` | The `--action` primary button. |

**Rules:**

- No colored shadows. Shadows are neutral. (`shadow-blue-500/10` etc. are banned.)
- No glow. `--shadow-glow` is deleted.
- No inset "inner glow" decoration. (The 1px inset top-edge highlights in `--shadow-md/lg/sheet/control` are a bevel, not a glow; keep them that faint.)
- Borders do elevation work first. A hairline + `--surface-raised` tone shift is the default lift mechanism. Shadows are reserved for *floating* things.

---

## 7. Motion

### 7.1 Durations

| Token | Value | Role |
|---|---|---|
| `--duration-instant` | `80ms` | Tailwind `duration-instant`; defined, not yet used. |
| `--duration-fast` | `120ms` | Hover state, press feedback, focus ring appearance. |
| `--duration-base` | `180ms` | Open/close of menus, toggles, accordion. Default for state changes. |
| `--duration-slow` | `280ms` | Modal/dialog open, full-panel reveals. |

Three, because faster (80ms) reads as "no animation" (fine — just omit the transition) and slower (400ms+) reads as "waiting" in a desktop tool. 120/180/280 is tighter than the old 150/250/350 — desktop apps should feel snappier than web.

### 7.2 Easings

| Token | Value | Role |
|---|---|---|
| `--ease-out` | `cubic-bezier(0.23, 1, 0.32, 1)` | Entrances (enters frame, arrives, settles). Default for "thing appeared." |
| `--ease-in` | `cubic-bezier(0.4, 0, 1, 1)` | Exits (dismiss, collapse, leave). |
| `--ease-in-out` | `cubic-bezier(0.77, 0, 0.175, 1)` | Things that move while staying on screen. |
| `--ease-drawer` | `cubic-bezier(0.32, 0.72, 0, 1)` | Panes and drawers sliding in; defined, not yet used. |
| `--ease-linear` | `linear` | Indeterminate loaders, progress. |

Tailwind exposes these as `ease-out`, `ease-in`, `ease-in-out`, `ease-drawer`, `ease-linear`.

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
  outline: 2px solid hsl(var(--ring) / 0.9);
  outline-offset: 2px;
  box-shadow: none;        /* explicit — no halo */
}
```

- 2px crisp outline, `--ring` (= `--accent`) at 90%. Lives in `src/index.css` (`@layer base`).
- 2px offset — separates ring from element without visual noise.
- No `box-shadow` glow halo. The old `[data-theme="dark"] :focus-visible { box-shadow: 0 0 0 4px rgba(56,189,248,0.1); }` is deleted.
- No pulse, no fade-in ramp, no color shift mid-focus.

For inputs where outline-offset clips on adjacent elements, we fall back to `box-shadow: 0 0 0 2px hsl(var(--ring))` — still a crisp ring, no blur.

---

## 9. Migration map

This section records the migration as planned. It has landed: the shadcn and themes.css tokens are gone, `themes.css` is deleted, and fonts are self-hosted. Where §1–§8 list more tokens than this map (surface, shadow, radius and easing steps; `--action`, `--highlight`, `--chrome`), §1–§8 are current.

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
| `--ring` | keep name, re-color | (now the accent, not neutral) |
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
| `src/index.css` | Rewrite `:root` and `[data-theme="dark"]` (also `.dark` — keep the class selector for Tailwind compatibility, point it at the same values as `[data-theme="dark"]`) with the token set above. |
| `tailwind.config.js` | Remove `brand.*`, `accent.{purple,pink,orange,emerald}`, fluid `fontSize`, `fluid-*` spacing, `boxShadow.glow/elevation-*`, `backdropBlur.xs`, all `keyframes`/`animation` entries, `bounce-in`. Add new `fontSize` map, confirm `letterSpacing` removed or simplified, point `boxShadow` entries at `--shadow-sm/md`, point `borderRadius` at `--radius-sm/md/lg`. |
| `src/styles/themes.css` | **Delete entire file.** Remove imports. |
| `src/styles/command-palette.css` | Audit for hardcoded hex literals (166 across codebase, some here). Replace with tokens or move to component styling phase. |
| `src/hooks/useApplyTheme.ts` | **Keep dual-apply as-is.** Both `.dark` and `[data-theme="dark"]` remain applied in sync, pointing at the same values. Collapsing to a single mechanism is out of scope for this phase and tracked as tech debt (see Decisions log §12). |
| `src/main.tsx` (font loader) | **Add Source Serif 4.** Done: `@fontsource-variable/source-serif-4` (with Inter and JetBrains Mono) is imported in `src/main.tsx`. |

### 9.3 Not handled by this spec (Phase 4 lint)

- Raw Tailwind palette utilities (`bg-blue-500`, `text-gray-*`, `from-sky-400`). At the time of writing there were 422 body uses and 1,349 gradient uses; as of 2026-10-02 the remaining ones are `emerald-*` classes in `src/components/LearningStudio/`. The ESLint `no-restricted-syntax` rule that forbids raw palette classes is written but commented out in `eslint.config.js`.
- Per-component hardcoded hexes (`bg-[#0d1117]` etc.). Migrated in Phase 3 when components are rewritten.

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

### 11.1 Violet over amber (and every other accent candidate)

*History: the shipped accent is copper on warm neutrals (§1.4, §2.4), not the violet chosen here. The reasoning is kept as the record of the first draft.*

Candidates considered: **neutral-zinc (shadcn default), violet, amber, steel blue, terminal-green, IA-orange**.

- **Neutral-zinc** is what stock shadcn ships. Safe, but the reason the current app has no identity — "monochrome with a gray accent" reads as *unstyled*, not *restrained*. Rejected.
- **Terminal-green** (~`140°`) fits the "code editor" half of the DNA but clashes with semantic success green. Two greens in one palette is a bug.
- **IA-orange / red-orange** (~`15°`) is warm and editorial but sits too close to `--danger`.
- **Steel blue** (~`215°`) paired beautifully with the Source Serif 4 decision but sits only 5° from the `220°` neutrals — functionally invisible at the sizes accents actually appear (1px focus rings, 2px borders, 12px chips). Identity cost: high. Rejected.
- **Amber (`38°`)** was the previous pick. It was warm, editorial, legibly distinct from cool-neutral surfaces, and nodded to analog / paper / editor highlight. **Rejected because it's not industry-standard for AI tools** — amber signals "editorial" or "writing tool," not "LLM-adjacent knowledge product." For a local-first tool that runs AI workflows, the category signal matters: users reach for violet UIs to mean "this is the kind of place where AI happens." Amber read as craft but not as *tribe*.
- **Violet (`252°`)** — claude.ai, Perplexity, Linear all run a near-identical hue. Adopting it places Lattice in the AI-tooling tribe it belongs to without copying any one app outright. At L=72% it holds 5.35:1 on dark bg (AA body), has unmistakable presence at 1px, and gives maximum hue distance (224°) from the orange warning.

**Defense in one sentence:** Violet because it's the category signal for AI-adjacent knowledge tools, clears AA at the lifted L=72% tuning, and leaves warning orange unambiguous at the opposite side of the wheel — so Lattice reads as part of the right tribe without copying any single app.

**Discipline cost noted.** Violet wants to expand: any gradient, panel fill, or large hero use will push the app toward generic-AI-startup chrome. Restrict to focus rings, selected-state borders, link text, and ≤32px solid surfaces. Enforced in review + lint.

### 11.2 Four peer text levels — widen the gaps to keep them distinct

Originally I ran three peer levels plus disabled (`primary / secondary / muted / disabled`), on the theory that four peers always degrade into noise. Josh pushed back: the app genuinely has four registers in use (lead text, sub-text, contextual descriptor, background metadata). I agreed, but with a condition — to avoid the "tertiary vs muted are basically the same" failure mode, I deliberately widened the contrast gaps. On dark mode the lightness steps are **96 → 76 → 60 → 45 → 32**, yielding contrasts of **15.8 → 9.1 → 5.3 → 3.4 → 2.0**. Each step is a perceptually clear jump, not a delta of taste. The usage table in §1.3 codifies when each is correct, so the decision isn't left to eyeball. If in review tertiary and muted still blur together, I widen further (muted drops to L=42%) — but the current spread should hold.

### 11.3 `text-base` = 16px (convention over density, Josh's call)

Originally I shipped 15px as a density move — desktop app, power-user audience, Linear/Arc/Vercel all run 13-14px chrome. Josh chose **16px** instead. The tradeoff: we give up ~6% vertical density per body line in exchange for hitting web-standard body size, which means (a) any rendered user content (notes, chat messages) reads at expected reading-comfort size without an extra override, (b) OS zoom and accessibility tooling behave with zero surprise, and (c) nobody ever has to defend "why is your body text 15px?" in a usability review. Chrome density is recovered via `text-sm` (14px) and `text-xs` (12px) where needed — table rows, secondary metadata, dense sidebars. This is explicitly chosen over density per Josh's call.

---

## 12. Decisions log

*The accent and neutral-hue rows below record the first draft. The shipped system later moved to warm neutrals (hue `30`–`42`) and a copper accent (`22°` dark, `12°` light); see §1.4.*

The first draft of this spec ended with a list of open questions. This revision closes them — recording what was asked and what Josh decided, so future readers can reconstruct the rationale without digging through chat history.

| Question | Decision | Rationale |
|---|---|---|
| Accent hue: amber, steel blue, or violet? | **Violet.** Final: `#8B72FF` (`252 100% 72%`) dark; `#5538E0` (`252 75% 55%`) light. | Amber was the prior pick (warm, editorial) but doesn't read as industry-standard for AI tools. Violet is the category signal (claude.ai, Perplexity, Linear). Steel blue paired best with the serif but sat only 5° from the `220°` neutrals — functionally invisible. Violet clears AA at L=72% and places Lattice in the right tribe. |
| Should accent amber be brighter (`#f5a524`) or darker? | **Superseded.** Accent moved to violet entirely. | Kept in log for history: during the amber phase, darker (`#cb8919`) was chosen over brighter for editorial read. Moot now. |
| Warning hue at `38°` or shift to `28°`? | **`28°`.** | Decided during the amber phase to avoid accent/warning collision. Still correct under violet — with accent at `252°`, warning at `28°` gives 224° hue distance (maximum) and reads as unambiguous red-orange caution. |
| `text-base` at 15px (density) or 16px (convention)? | **16px.** | Josh chose convention. Gives up ~6% vertical density per body line; gains zero-surprise behavior for rendered prose, OS zoom, and accessibility. Density is still recoverable via `text-sm` (14px) where chrome needs it. |
| Three peer text levels + disabled, or four peer + disabled? | **Four peer + disabled.** Final: `primary / secondary / tertiary / muted / disabled`. | App has four genuine registers. To keep them distinct, I widened the lightness steps and codified per-level usage in §1.3 so the decision isn't left to eyeball. Muted dropped to 3.4:1 (supplemental only); tertiary holds 5.3:1 (AA body). |
| Ship with no serif, or add `--font-serif` for prose? | **Add `--font-serif`.** Scope: rendered note body / markdown / long-form reading surfaces only. Never chrome. Primary: Source Serif 4. | An editorial serif in prose surfaces reinforces the "content is the interface" principle — user-authored text gets a reading-optimized face, chrome stays sans. Strict scoping keeps it from becoming decoration. Requires font loading (flagged in §9.2). |
| `.dark` class vs. `[data-theme="dark"]` attribute — collapse to one? | **Keep both.** | During this migration both mechanisms stay applied in sync via `useApplyTheme`, pointing at the same values. Collapsing to one is out of scope for this phase and is tracked as tech debt for a later cleanup. |
| `--chart-*` tokens — specify now or defer? | **Defer.** | Five-color chart palette remains stubbed. Specified alongside whichever charting library actually ships — palette decisions without a use case are guesswork. |
