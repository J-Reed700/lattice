# Dashboard Flatten — Implementation Specification

**Status:** proposed
**Paired with:** `AESTHETIC-GUIDE.md` §"No bento-card theatrics", `TOKENS-SPEC.md`, Chat/Journal/Reference redesign specs.
**Scope:** `components/Dashboard/**` visual only. No data, hooks, queries, features added or removed.

---

## 1. Current state

`Dashboard.tsx` inlines its own render. The other folder files (`DashboardHeader`, `DashboardContent`, `DashboardStats`, `DashboardEmpty`, `QuickActions`, `RecentActivity`, `RecentDocuments`) are **unreferenced** — only `DashboardSkeleton` and `DashboardError` are imported (12-13).

Live card count: **5 `BentoCard`s** (`RecentActivity` span-2×2 at 106; four stats at 139/149/157/165) plus **3 tinted quick-action buttons** (55/70/85) that repeat the same sin without using `BentoCard`.

Flourishes:
- Gradient headline `bg-clip-text text-transparent` — 43.
- `staggerContainer`+`fadeInUp` mount cascade — 37, 52, 102.
- Per-row `initial/animate/delay` on activity rows — 112-117.
- Tinted chrome `border-[hsl(var(--accent))]/20 hover:shadow-md` — 57, 72, 87.
- `BentoCard` hover darken+border-promote — `BentoCard.tsx:26` (reads as lift).
- `drop-shadow-md` on icon — `DashboardHeader.tsx:23`. `hover:scale-105` — `DashboardStats.tsx:103`.

---

## 2. Flatten direction

**Mixed, by role:**
- **Stats:** no container. Typography and spacing only.
- **Quick Actions:** neutral rows, hairline dividers. No border/tint/shadow.
- **Recent Activity:** bare section — heading, hairline beneath, flat list.

Dashboard is a **scanning surface**. "Typography carries hierarchy, not borders" applies most literally here. A hairline-bordered card for Recent Activity was rejected: Chat, Journal, References render primary content without card wrappers; Dashboard shouldn't be the exception.

---

## 3. Per-section plan

### 3.1 Page header
Single-color `text-3xl` (weight 600, tracking `-0.02em`) in `--text-primary`. Below: `text-sm` `--text-tertiary` date. No icon, drop-shadow, or gradient. `40px` top, `24px` before first section.

### 3.2 Stats — inline metric row, no container
4-column grid, `gap: 48px`. Collapses 2-up <720px, 1-up <480px.
- Label: `text-xs` uppercase `tracking: 0.04em` weight 500 `--text-tertiary`.
- Value: `text-2xl` weight 600 `tabular-nums` `--text-primary`, ~4px top margin.
- No icon. No trend chip. (The current `"+12%"` is fabricated — `value > 0 ? "+12%" : undefined` — delete regardless.)

Separator: the 48px gap. Only add a vertical hairline if review shows columns read merged.

### 3.3 Quick Actions — neutral rows
Heading `Quick Actions` at `text-lg` weight 500 `--text-secondary`, hairline beneath.
Each row: 56px, full-width left-aligned. 18px lucide icon (`--text-tertiary`) + label (`text-sm` weight 500 `--text-primary`) + description (`text-xs` `--text-muted`). No border/tint/shadow.
Hover: `--surface` fill only, 120ms `--ease-out`. No border change, no lift.
Rows separated by `--border-subtle` hairlines. If one action needs emphasis, earn it with weight 600 on the label — never color.

### 3.4 Recent Activity — bare section
Heading `Recent Activity` at `text-lg` weight 500 `--text-secondary`, hairline beneath.
Up to 5 rows: 16px file icon (`--text-tertiary`) + filename (`text-sm` weight 500 `--text-primary`, truncate) + metadata (`text-xs` `--text-muted`, relative time · type, interpunct-separated). Rows separated by `--border-subtle` hairlines, padding `12px 0`. Hover: `--surface` fill.
Empty state: single `text-sm` `--text-tertiary` line. No icon, no illustration. No per-row entrance animation.

---

## 4. `StatCard` and `BentoCard`

Neither survives. `StatCard`'s icon+trend features are the decoration we're cutting; with stats as pure type, it has no job.

`BentoCard` is used nowhere else — grep finds only its own module + `Dashboard.tsx`.

`StatCard` has one external import at `DocumentList.tsx:9`, but that file declares its own local `StatCard = memo(...)` at line 9 that shadows the import — **the import is dead code**. Delete both directories and the shadowed import.

---

## 5. Layout

**Single column, centered, `max-width: 760px`** — matching Chat/Journal/References. Dashboard is scanning not reading, but a wider column for the home screen than the primary surfaces would advertise Dashboard as the visually substantial one. It isn't.

Container: `max-w-[760px] mx-auto`, padding `24px` horizontal / `40px` top / `64px` bottom.

**Section spacing: `48px`** (larger than siblings' 32px because sections are heterogeneous, not repeating rows).

The current grid (`Dashboard.tsx:104-106`) is deleted. Every section is a full-width strip within the column.

---

## 6. What to delete

Grep-confirmed. After migration:

- `components/ui/BentoCard/` (both files).
- `components/ui/StatCard/` (both files).
- `components/Dashboard/{DashboardHeader,DashboardContent,DashboardStats,DashboardEmpty,QuickActions,RecentActivity,RecentDocuments}.tsx` — all unreferenced.
- Trim `components/Dashboard/index.ts` to `Dashboard`, `DashboardSkeleton`, `DashboardError`.
- Remove dead `StatCard` import at `DocumentList.tsx:9`.

`DashboardSkeleton.tsx` stays but its structure is rewritten: header row, 4-column metric strip, 3 action rows, 5 activity rows. Flat `Skeleton` at `--surface`, no shimmer.

---

## 7. Motion

**Removed:** `staggerContainer`, `fadeInUp`, per-row `initial/animate/delay`, `hover:scale-105`, `hover:shadow-md`.

**Kept:** hover fill on rows (`--duration-fast` `--ease-out` background only); focus-ring per TOKENS §8.

No mount entrance. Dashboard appears; it does not perform.

---

## 8. Voice / copy flagged (not fixed)

- `"Welcome back"` (43), `"Here's your knowledge hub"` (47) — keynote/marketing voice.
- `"Add Files / Import local files"` (64-65), `"Add Web Page / Import from URL"` (79-80), `"Search / Find in knowledge base"` (94-95) — parenthetical-explanation voice; "knowledge base" is product-speak.
- `"Welcome to Vault"` / `"Get started by indexing your first folder…"` (`DashboardEmpty.tsx:17-22`) — onboarding voice; also app-name drift ("Vault" vs "Recall").
- `"Good morning/afternoon/evening"` greeting-by-clock (`DashboardHeader.tsx:3-7`) — brand move. File being deleted, but flag the pattern.
- Consider `"Indexed"` over `"Documents"` for the stat label — matches Recall's "index" vocabulary elsewhere.
