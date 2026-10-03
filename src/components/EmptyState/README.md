# EmptyState Component

Consistent, helpful empty states across the application.

## Purpose

Provide users with clear guidance when views have no content: one sentence,
one way forward.

## Features

- Optional glyph in a quiet tile
- Title set in the reading face, optional description
- At most one action, with an optional keyboard shortcut shown beside it
- Theme-aware through tokens (no `dark:` variants)

## Usage

### Basic Usage

```tsx
import { EmptyState } from '@/components/EmptyState';
import { Search } from 'lucide-react';

<EmptyState
  icon={<Search />}
  title="No results found."
  description="Try adjusting your search query."
/>
```

### With Action

The action is data, not an element: `EmptyState` renders the one button
itself so every empty state's action looks the same.

```tsx
<EmptyState
  title="Nothing indexed yet."
  action={{ label: 'Add a folder', onClick: handleAddFolder, shortcut: '⌘I' }}
/>
```

There are no preset variants. A surface that needs a named empty state wraps
`EmptyState` locally (see `FileBrowser/EmptyStates.tsx`).

## Props

| Prop | Type | Default | Description |
|------|------|---------|-------------|
| `icon` | `ReactNode` | - | Glyph shown in the tile (lucide-react icons are sized to 20px) |
| `title` | `string` | required | Main sentence |
| `description` | `string` | - | Supporting line, capped at `max-w-sm` |
| `action` | `{ label: string; onClick: () => void; shortcut?: string }` | - | The single call to action |
| `className` | `string` | - | Merged onto the wrapper with `cn()` |

`EmptyStateProps` is exported for wrappers.

## Layout

One size: centered column, `px-6 py-16`. The icon tile is 44px (`h-11 w-11`,
`rounded-xl`) on a 5% `--text-primary` wash; the title is 19px `font-serif`;
the description is `text-ui text-text-muted`; the action is an `h-8` bordered
`bg-surface` button with `shadow-control` and the `pressable` press scale, and
its shortcut renders in a `.kbd`.

## Design Decisions

### Why This Approach?

**Centered layout:** Draws attention to empty state without feeling like an error
**One sentence, one verb:** The title says what is missing; the action says what to do
**Optional action:** Guides users to next step when applicable
**Muted colors:** Doesn't compete with actual content

### Nine Dimensions

1. **Style:** Clean, minimal, non-threatening
2. **Motion:** Static apart from the button's press scale
3. **Voice:** Helpful, encouraging (not blaming)
4. **Space:** Generous padding, centered alignment
5. **Color:** Muted text, a faint tile behind the glyph
6. **Typography:** Clear hierarchy (serif title > description > action)
7. **Proportion:** Icon sized for visibility without domination
8. **Texture:** Soft tile with a hairline inset ring
9. **Ergonomics:** The action is the only pressable thing on the page

## Accessibility

- The action is a native `<button type="button">` with a visible focus ring
- Pass a decorative icon; the title and description carry the meaning
- Text colors come from tokens that meet contrast in both themes

The component sets no live-region role; a view that needs its empty state
announced should provide one around it.

## Integration Examples

### Library (`FileBrowser/EmptyStates.tsx`)

```tsx
<EmptyState
  title={query ? `No files match “${query}”.` : 'No files match.'}
  action={query ? { label: 'Clear search', onClick: () => setSearchQuery('') } : undefined}
/>
```

### Home error (`Dashboard/DashboardError.tsx`)

```tsx
<EmptyState
  title="Couldn't load Home."
  description={error}
  action={{ label: 'Try again', onClick: () => window.location.reload() }}
/>
```

Other consumers: `FileBrowser/FileBrowser.tsx`, `Compare/ComparePage.tsx`,
`Settings/AIModelsTab.tsx`. Search has its own `SearchEmptyState`.

## Do's and Don'ts

### Do
- Use for genuinely empty states, and for a failed load that has one retry
  (Home and Settings → AI Models do this)
- Provide clear next actions
- Keep descriptions concise (1-2 sentences)
- Test with screen readers

### Don't
- Use for loading states (see LoadingState)
- Use for render errors (see ErrorBoundary)
- Write vague messages ("No data")
- Overload with multiple actions
- Use emojis as icons

## Related Components

- **LoadingState** - For loading content
- **ErrorBoundary** - For error states
- **FirstRun** - For onboarding
- **Toast** - For transient messages
