# Recall — Aesthetic Guide

Strategic north star for all visual decisions. No tokens, no code. This doc defines the DNA.

---

## 1. Product soul

Recall is a local-first tool for thought: a private index of the files, notes, and conversations that make up a person's working memory. It is an instrument, not a destination. The aesthetic must therefore recede — the interface is a lens onto the user's own material, and the user's material is what should look good. Recall is closer to a well-made text editor or a code IDE than to a SaaS dashboard: calm, legible, high-density when asked, quiet when idle, and trustworthy because it never performs.

---

## 2. The DNA principles

Shared DNA distilled from Linear, Notion, Vercel, Arc, and Things 3.

1. **Content is the interface.** The user's notes, documents, and conversations are the product. Chrome exists to frame them, never to compete with them.
2. **Typography carries hierarchy, not borders.** Weight, size, and spacing do the structural work. Reach for a rule or a card only when type and rhythm genuinely can't.
3. **One neutral palette, one accent.** A single tuned grayscale ramp plus one restrained accent, used sparingly for state and identity. Color is a signal, not a mood.
4. **Surfaces are near-flat; depth is a whisper.** A hairline border and a one-pixel tonal shift are usually enough. Shadow is reserved for things that truly float (menus, modals, drags).
5. **Motion confirms causality.** Animation exists to show what caused what — a panel opening, a state changing. It is short, linear-ish, and unnoticed. Nothing bounces. Nothing celebrates.
6. **Density is a feature.** Power users live in this app. Generous line-height and legible scale, but tight information grids — closer to a code editor than a marketing site.
7. **Keyboard-first, pointer-supported.** Every primary action has a shortcut and a visible affordance for finding it. The command palette is the spine.

---

## 3. What we are NOT

The current app violates nearly every principle above. These are the specific sins we are correcting, drawn from what is shipping in `index.css`, `themes.css`, `tailwind.config.js`, and the Dashboard/Chat components today:

- **No rainbow accent system.** We are retiring `--accent-purple`, `--accent-pink`, `--accent-orange`, `--accent-emerald` as general-purpose palette entries. One accent. Semantic colors (success/warning/error) exist but are muted and used only for state.
- **No gradient text.** The `bg-gradient-to-r from-sky-400 via-purple-400 to-sky-400 bg-clip-text` "Welcome back" headline and any `gradient-text-*` utility is gone. Headlines are a single color with deliberate weight.
- **No glow.** `--shadow-glow`, `.glow`, `.glow-hover`, `animate-glow-pulse`, the sky-blue ring halos on focus — all removed. Focus rings are a crisp single-color outline.
- **No `backdrop-blur` as decoration.** `.glass`, `.glass-strong`, `.glass-modal`, `.glass-navigation`, `.glass-subtle` are not an aesthetic — they are a last resort for a specific problem (an element over unpredictable content). Default surfaces are opaque.
- **No bento-card theatrics.** The `translateY(-4px)` hover lift, the gradient top-border that fades in on hover, the `::before` mesh overlays, the `bento-card-enhanced` double-treatment — cut. Cards, if used at all, are a hairline border and nothing else.
- **No bounce easing.** `cubic-bezier(0.68, -0.55, 0.265, 1.55)`, `--ease-bounce`, `--transition-bounce`, `--ease-spring` with overshoot — deleted. Motion is standard ease-out or linear. Overshoot is cartoonish.
- **No shimmer loading, no `glow-pulse`, no staggered `fadeInUp` cascades** on every screen mount. Skeletons are flat blocks. Content appears; it does not perform an entrance.
- **No tinted card backgrounds per-action.** The Dashboard's `from-[var(--accent-primary)]/5`, `from-[var(--success)]/10`, `from-[var(--warning)]/10` per-button treatment is retired. Buttons are neutral. Color is earned by state, not decoration.
- **No gradient mesh backgrounds.** `--gradient-mesh` radial-gradient washes behind pages are gone. The canvas is one flat, well-tuned neutral.
- **No two parallel token systems.** Today we run `hsl(var(--background))` (shadcn) *and* `--bg-primary: #ffffff` (themes.css) *and* raw Tailwind `sky/purple/orange` classes simultaneously. Phase 2 collapses this to one source of truth. Until then, authors should treat the shadcn tokens as authoritative and stop reaching for the others.
- **No `console.log` left in render paths.** Unrelated to aesthetic, but it's in `Dashboard.tsx` and it signals the overall lack of discipline we're correcting.

---

## 4. Aesthetic mood

Five adjectives, chosen to resolve the tension between the references:

**Quiet. Considered. Editorial. Precise. Unfinished-feeling.**

"Unfinished-feeling" is deliberate — Linear and Arc both look like working drafts rather than final products. That aesthetic (visible grid, thin rules, content slightly under-styled) reads as confidence. Over-polish reads as marketing.

---

## 5. Dark-first

Dark mode is the primary design target. Light mode is a derivation.

Recall is a thinking tool, used for long sessions, often in low light, frequently alongside a code editor or terminal. The dominant reference apps all design dark-first (Linear, Vercel, Arc) or achieve their signature look through it. Designing dark-first forces discipline around contrast, depth, and accent restraint — mistakes are louder in dark mode, and anything that survives translates cleanly to light. Light mode will be derived, tuned, and shipped — but it is not where decisions are made.

---

## 6. Decision heuristics

When in doubt:

- **Remove color before adding it.**
- **Reduce contrast before increasing it.** Most UI chrome wants to be quieter than it is.
- **Make it smaller.** Our instinct to enlarge is usually wrong; reach for weight or spacing first.
- **Use a hairline before a shadow.** Use a shadow before a blur. Use a blur almost never.
- **Prefer a keyboard path before a new button.**
- **If animation is noticed, it's too much.**
- **If a screen needs a gradient, the layout is wrong.**
