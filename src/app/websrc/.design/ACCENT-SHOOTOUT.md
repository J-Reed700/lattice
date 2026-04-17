# Accent Shootout — Violet vs. Steel Blue

**Status:** decision pending
**Constraint:** app must still feel black-dominant; accent earns its place by restraint
**Bg refs:** dark `--bg` `#0f1115` (L=0.005); light `--bg` `#f7f8fa` (L=0.952)

---

## Finalist A — Modern violet

**The pitch.** Violet is the visual signature of the current AI-tooling category — Anthropic uses it on claude.ai chrome, Perplexity leans into it for primary CTAs, Linear uses a near-identical hue at `250°`. For a local-first knowledge tool that runs LLM workflows, it places Recall in a recognizable adjacent tribe without copying any one app outright. **Risk:** at any saturation high enough to register as "violet" rather than "lavender," it reads AI-startup. If used on large fills (panels, hero buttons) it betrays the editorial intent. Restrict to focus rings, selected-state borders, link text, and ≤32px solid surfaces.

**Note on tuning.** The brief's `252° 100% 64%` (`#6D4AFF`) hits **3.27:1** vs `--bg` — fails AA body. Lifted to L=72% to clear AA cleanly while staying recognizably violet-not-lavender.

### Dark mode

| Token | HSL | Hex | Defense |
|---|---|---|---|
| `--accent` | `252 100% 72%` | `#8B72FF` | 5.35:1 vs bg — AA body. L=64% from brief failed AA (3.27:1). |
| `--accent-hover` | `252 100% 78%` | `#A38FFF` | +6% L. |
| `--accent-muted` | `252 35% 18%` | `#251F3D` | Low-sat tint for selected-row bg. |
| `--accent-fg` | `252 25% 8%` | `#13101F` | Near-black. White on `#8B72FF` is **3.57:1** (fails AA body); dark fg gives 5.35:1. |

### Light mode

| Token | HSL | Hex | Defense |
|---|---|---|---|
| `--accent` | `252 75% 55%` | `#5538E0` | Darkened for bright canvas. 6.59:1 vs bg. |
| `--accent-hover` | `252 78% 48%` | `#421FD1` | Darkens on hover (light-mode convention). |
| `--accent-muted` | `252 90% 95%` | `#EBE6FE` | Pale violet tint. |
| `--accent-fg` | `252 30% 98%` | `#FAF8FF` | Off-white. 6.91:1 on `#5538E0`. |

---

## Finalist B — Steel blue

**The pitch.** Blue pulled toward gray — the color of a working reference tool, not a brand. Bloomberg Terminal, Things 3's secondary chrome, every well-designed reading app from the last decade. It pairs hardest with the Source Serif 4 prose decision because both signal "this is a place to read and think, not a place to be marketed to." **Risk:** at scale it reads corporate-IT; at small scale it disappears into the cool-neutral `220°` surfaces (only 5° of hue separation). Needs generous whitespace and never gets used as a panel fill.

**Note on tuning.** The brief's `215° 38% 47%` (`#4A6FA5`) hits **3.69:1** vs `--bg` — fails AA body. Lifted to L=58% with slightly reduced saturation; preserves the steel-blue intent and clears AA.

### Dark mode

| Token | HSL | Hex | Defense |
|---|---|---|---|
| `--accent` | `215 36% 58%` | `#6B8FBF` | 5.69:1 vs bg — AA body. Brief's L=47% failed (3.69:1). |
| `--accent-hover` | `215 38% 64%` | `#7FA0CB` | +6% L. |
| `--accent-muted` | `215 25% 18%` | `#23303D` | Low-sat steel tint. |
| `--accent-fg` | `220 25% 8%` | `#10131A` | Near-black. White on `#6B8FBF` is **3.35:1** (fails AA body); dark fg gives 5.69:1. |

### Light mode

| Token | HSL | Hex | Defense |
|---|---|---|---|
| `--accent` | `215 38% 38%` | `#3B5A85` | Darkened for bright canvas. 6.55:1 vs bg. |
| `--accent-hover` | `215 42% 32%` | `#2F4A74` | Darkens on hover. |
| `--accent-muted` | `215 40% 93%` | `#E0E8F2` | Pale steel tint. |
| `--accent-fg` | `220 30% 98%` | `#F8F9FB` | Off-white. 6.86:1 on `#3B5A85`. |

---

## Contrast comparison

| Check | Violet | Steel blue | Threshold |
|---|---|---|---|
| accent on dark bg | **5.35:1** | **5.69:1** | ≥4.5 (AA body) |
| accent on light bg | **6.59:1** | **6.55:1** | ≥4.5 |
| accent-fg on accent (dark) | **5.35:1** dark fg | **5.69:1** dark fg | ≥4.5 |
| accent-fg on accent (light) | **6.91:1** white fg | **6.86:1** white fg | ≥4.5 |
| accent-muted vs bg (dark) | 1.42:1 | 1.41:1 | <2 (subtle) |

White-fg-on-accent fails for both in dark mode (3.57 / 3.35). Both must use dark `--accent-fg`. In light mode the polarity flips and white wins for both.

---

## Pairing checks

**Violet:**
- *vs. Source Serif 4 prose:* Cool. Reads as "tech tool wrapped around editorial content" — the contrast is the point or the problem, depending on temperament.
- *vs. shadcn neutrals (`220°` cool-grays):* 32° of hue separation. Clean read.
- *vs. orange warning (`28°`):* 224° apart — maximum hue distance. Zero confusion risk.
- *vs. black-dominant chrome:* Holds the constraint *only if* discipline is enforced. Violet wants to expand. The token system can't prevent that — review will have to.

**Steel blue:**
- *vs. Source Serif 4 prose:* Tightest pairing of any candidate. Both speak "library."
- *vs. shadcn neutrals (`220°`):* Only 5° of hue separation. At small sizes, accent borders can look like darker neutral borders. Has to be louder than amber would have been to register at all.
- *vs. orange warning (`28°`):* 187° apart. Clean separation.
- *vs. black-dominant chrome:* Naturally recedes. Hardest of the candidates to over-use because it doesn't reward over-use — recedes when scaled up rather than commanding attention.

---

## Where each fails

**Violet** fails the moment someone fills a card with it, ships a violet gradient, or uses it as a sidebar background. The hue has gravitational pull toward "AI product" iconography — three months in, you'll find a violet glow somewhere unless the lint actively forbids it. The `--accent-muted` tint at `#251F3D` is also notably violet-coded; using it as a row highlight on a list of notes makes the list feel like a chat product. Discipline cost: high.

**Steel blue** fails by underperforming. At only 5° from the neutral hue, it can read as "a slightly bluer gray border" rather than as accent — especially on hairlines and thin focus rings against `--border-default` `#2e333f`. If it doesn't register, users won't develop a mental model of "blue means selected/active/focused," which defeats the point of having an accent. Discipline cost: low. Identity cost: high.

---

## Recommendation

**Pick violet.** Steel blue is the more tasteful choice on paper and pairs better with the serif prose decision, but its 5° hue separation from the `220°` neutrals makes it functionally invisible at the sizes accents actually appear (1px focus rings, 2px borders, 12px chips) — the accent has to *register* before it can be restrained. Violet at L=72% holds AA, has unmistakable presence at 1px, and places Recall in the AI-tool tribe it actually belongs to; the discipline cost (no large fills, no gradients, never a panel bg) is real but enforceable in lint and review. If Josh wants the editorial-reference-tool read above the AI-tool read, pick steel blue and accept that the accent will be quiet to the point of subliminal.

---

## Red flags surfaced during contrast math

Both finalists' brief-spec HSL values **failed AA on dark bg as body text** (violet at L=64%: 3.27:1; steel blue at L=47%: 3.69:1). Both required lifting lightness to clear 4.5:1 — violet to L=72%, steel blue to L=58%. White-on-accent also fails AA body for both in dark mode; `--accent-fg` must be near-black. If the brief's exact saturation/lightness values are load-bearing for aesthetic reasons, the alternative is restricting accent to UI-only contexts (3:1 threshold) and never using it as text — but that breaks the existing `--ring` and link-text patterns.
