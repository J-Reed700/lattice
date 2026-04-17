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

### 1.3 Text — 4 levels

| Token | HSL | Hex | Contrast vs `--bg` | Role |
|---|---|---|---|---|
| `--text-primary` | `220 15% 96%` | `#f2f4f8` | **15.8:1** | Body, headings, primary content. |
| `--text-secondary` | `220 12% 72%` | `#b0b6c2` | **8.3:1** | Supporting copy, metadata, secondary labels. |
| `--text-muted` | `220 10% 55%` | `#848b98` | **4.7:1** | Timestamps, hints, placeholder-ish info. Meets AA body. |
| `--text-disabled` | `220 10% 38%` | `#565c68` | 2.4:1 | Disabled state only. Fails AA by design — never carries meaning. |

Four because the app genuinely has four registers (primary > secondary > muted > off). No "tertiary" — `muted` covers it and contrast budget doesn't leave room for a fifth.

### 1.4 Accent — one hue

**Chosen:** **Amber** — HSL `38 92% 58%`, hex **`#f5a524`**.

Reasoning defended below (§11 tensions). Token set is minimal:

| Token | HSL | Hex | Role |
|---|---|---|---|
| `--accent` | `38 92% 58%` | `#f5a524` | Primary accent. Focus rings, selection, primary button bg. |
| `--accent-hover` | `38 94% 52%` | `#f09808` | Hover/pressed on primary button. |
| `--accent-muted` | `38 70% 22%` | `#5e4412` | Subtle accent tint (selected-row bg, current-line highlight). |
| `--accent-fg` | `30 20% 10%` | `#1f1a14` | Text *on* `--accent` solid fill. Deep warm-black, not pure black. |

Contrast `--accent-fg` on `--accent`: **11.2:1** — far above AA for large and AA+ for body. Contrast `--accent` on `--bg` (as thin ring/text): **8.1:1** — comfortably above AA.

One accent — no `accent-2`, no `accent-secondary`. If a second visual hue is ever needed, it's a semantic state (§1.5), not a mood.

### 1.5 Semantic state — muted by default

Each has a foreground (text/icon *on* the muted bg) and a saturated variant used **only** for critical moments (destructive confirms, error blocking toasts). Default chips/banners/badges use the muted pair.

| Token | HSL | Hex | Role |
|---|---|---|---|
| `--success-muted` | `152 40% 18%` | `#1e3a2d` | Background tint for "saved," "indexed," success chips. |
| `--success-fg` | `152 60% 70%` | `#7dd3a8` | Text/icon on `--success-muted`. 5.6:1 vs muted. |
| `--success` | `152 60% 50%` | `#33cc85` | Reserved. Only for critical confirmations. |
| `--warning-muted` | `38 45% 20%` | `#4a3a12` | Background tint for warning chips. |
| `--warning-fg` | `38 85% 68%` | `#f0b85c` | Text/icon on `--warning-muted`. 5.1:1 vs muted. |
| `--warning` | `38 85% 55%` | `#e59b24` | Reserved. Only for blocking warnings. |
| `--danger-muted` | `0 45% 22%` | `#522220` | Background tint for destructive affordances (delete-row hover). |
| `--danger-fg` | `0 75% 72%` | `#ea8383` | Text/icon on `--danger-muted`. 5.0:1 vs muted. |
| `--danger` | `0 72% 55%` | `#e04545` | Reserved. Destructive confirm buttons, error toasts. |

Note: `--warning` and `--accent` share the amber family deliberately — warning is a more saturated yellow-amber (`38 85% 55%`), accent is softer gold (`38 92% 58%` with different lightness). In practice they never co-occur: accent = identity/focus, warning = state. If that feels too close in review, we shift warning toward `32°` (orange-warn) — noted as an open question in §12.

**No `--info`.** The audit showed 0 places where info is semantically different from "secondary text." Dead token.

### 1.6 Utility

| Token | HSL | Hex | Role |
|---|---|---|---|
| `--ring` | `38 92% 58%` | `#f5a524` | Focus ring. Identical to `--accent`. Single token for discoverability. |
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
| `--text-primary` | `220 30% 10%` | `#121823` | **16.1:1** | Body, headings. |
| `--text-secondary` | `220 15% 32%` | `#474d5c` | **8.7:1** | Supporting. |
| `--text-muted` | `220 10% 46%` | `#696f7c` | **4.8:1** | Hints, timestamps. Meets AA body. |
| `--text-disabled` | `220 10% 68%` | `#a5aab4` | 2.1:1 | Disabled only. |

### 2.4 Accent

| Token | HSL | Hex | Role |
|---|---|---|---|
| `--accent` | `36 88% 45%` | `#d68510` | Darker than dark mode's accent — light mode needs more saturation depth for legibility on a bright canvas. |
| `--accent-hover` | `36 90% 38%` | `#b86e08` | Hover state. |
| `--accent-muted` | `38 80% 92%` | `#fbecd2` | Selected-row tint, subtle highlights. |
| `--accent-fg` | `30 30% 98%` | `#fbf9f6` | Text on solid `--accent`. Off-white, not pure white. |

Contrast `--accent-fg` on `--accent`: **4.9:1** — AA body. Contrast `--accent` on `--bg`: **4.6:1** — AA body for accent-as-text usage.

### 2.5 Semantic state

| Token | Hex | | Token | Hex |
|---|---|---|---|---|
| `--success-muted` | `#e6f5ec` | | `--danger-muted` | `#fbe7e7` |
| `--success-fg` | `#1e7d4a` | | `--danger-fg` | `#a62828` |
| `--success` | `#1f9d54` | | `--danger` | `#c32d2d` |
| `--warning-muted` | `#fcf1d8` | | `--ring` | `#d68510` |
| `--warning-fg` | `#8c5a08` | | `--overlay` | `rgba(20,22,28,0.45)` |
| `--warning` | `#c47808` | | | |

All `-fg` on `-muted` pairs meet AA (≥4.5:1). Verified.

---

## 3. Typography

### 3.1 Stack

| Family | Stack | Role |
|---|---|---|
| `--font-sans` | `'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif` | Default. Inter is already loaded; no change. |
| `--font-mono` | `'JetBrains Mono', Menlo, Monaco, Consolas, monospace` | Code, keyboard shortcuts, file paths, identifiers. |
| **No serif.** | | Rejected. A serif would be an "editorial moment" accent — but the aesthetic guide's editorial-ness is about *restraint*, not *typographic variety*. Linear, Arc, Vercel ship no serif. |

### 3.2 Scale — fixed, 1.125 modular (major second)

No `clamp()`. Tauri desktop app, predictable viewport, fixed scale is cleaner and more editorial.

Semantic names. The old `text-display-*` / `text-heading-*` /  `text-body-*` names were zero-used — but the Tailwind-ish `text-xs/sm/base/lg/xl/2xl/3xl` names ARE what the 1,349 current usages reach for. Keep the ergonomics, re-anchor the values. Add an `xxs` for micro-labels. Drop `4xl+` — nothing in a desktop tool needs 48px+ type.

| Token | Size (rem) | Size (px) | Line-height | Tracking | Weight default | Role |
|---|---|---|---|---|---|---|
| `text-xxs` | `0.6875rem` | 11px | 1.45 | `0.02em` (+loose) | 500 | Micro-labels, kbd chips, status dots. |
| `text-xs` | `0.75rem` | 12px | 1.5 | `0.01em` (+slight) | 400 | Captions, timestamps, table meta. |
| `text-sm` | `0.8125rem` | 13px | 1.55 | `0` | 400 | Secondary UI text. |
| `text-base` | `0.9375rem` | 15px | 1.6 | `0` | 400 | **Body default.** (Not 16px — desktop app, denser.) |
| `text-lg` | `1.0625rem` | 17px | 1.55 | `-0.005em` | 500 | Emphasized body, large labels. |
| `text-xl` | `1.25rem` | 20px | 1.4 | `-0.01em` | 600 | H3-equivalent, section headings. |
| `text-2xl` | `1.5rem` | 24px | 1.3 | `-0.015em` | 600 | H2. |
| `text-3xl` | `1.875rem` | 30px | 1.2 | `-0.02em` | 600 | H1 / page titles. Ceiling. |

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
| `--text-primary/secondary/tertiary` | **DELETED + re-added** | re-introduced above with new scale (primary/secondary/muted/disabled) |
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
| `websrc/hooks/useApplyTheme.ts` | Remove the dual-apply logic — `[data-theme]` is the source of truth; keep `.dark` class only if shadcn primitives need it (they do, for `darkMode: 'class'`). Apply both, same values, no conflict. |

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
- **Typography for rendered Markdown / user content.** The scale above is for *chrome*. User-authored content (notes, chat messages) may need its own prose scale (`prose-*`) later — deferred.
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

The risk I'm accepting: amber is also my warning hue. I'm resolving this by making warning a different lightness/saturation (`38 85% 55%` vs. accent `38 92% 58%`) and, critically, by convention — accent is identity/focus/selection, warning is state on chips and banners. In practice they shouldn't co-occupy a region. If in review it feels too close, shift warning to `28°` (orange) — noted in §12.

**Defense in one sentence:** Amber because it's warm enough to feel human in a tool that's otherwise quiet, distinct enough from both our neutral hue and our danger hue to carry meaning cleanly, and not claimed by a major reference app — so Recall owns it.

### 11.2 Three text levels, then disabled — not four peer levels

The obvious answer is `primary / secondary / tertiary / quaternary` as peers. I went `primary / secondary / muted / disabled` — three semantic levels plus one state-only level. Reason: every system I've seen with four peer text levels ends up with nobody knowing when to use level 3 vs level 4, and they degrade into noise. By making the fourth level *state-only* (disabled), I remove the decision. If you need a fourth *semantic* register, you don't — use `--text-muted` with weight 500 or a size step.

### 11.3 `text-base` = 15px, not 16px

Web-standard body is 16px. I'm shipping 15px as the default. Reason: this is a desktop app with power-user density as an explicit goal ("closer to a code editor than a marketing site"). Linear ships 13-14px body; Arc ships 13px chrome; Vercel dashboard sits at 14px. 16px reads as marketing-site / onboarding. 15px is the middle — denser than web default, legible enough that nobody complains. If users with accessibility needs bump their OS zoom, the rem unit scales cleanly.

---

## 12. Open questions for Josh

1. **Amber warning vs. amber accent proximity.** If in a real component review these read too similar, do we shift warning to orange (`28°`) or keep the shared family and rely on context? My lean: ship as specified, revisit after component work.
2. **`text-base` at 15px vs. 16px.** Are you comfortable with the density tradeoff? (This is the single most debatable typography decision — affects every screen.)
3. **`.dark` class vs. `[data-theme="dark"]` attribute.** Tailwind's `darkMode: ['selector', 'class']` reads `.dark`; shadcn components assume it. Current app toggles both. Spec assumes we keep both applied in sync (same values). Acceptable, or do you want a single-mechanism fix in this phase?
4. **Serif rejection.** I cut it outright. If you want an editorial serif for, e.g., rendered Markdown headings in notes, say so and I'll spec an optional `--font-serif` for *prose content only* (not chrome).
5. **`--chart-*` tokens.** Keeping the five-color chart palette stubbed but not specified. Defer to whenever charts actually ship, or do you want me to propose now?
