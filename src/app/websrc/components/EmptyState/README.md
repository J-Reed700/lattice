# EmptyState Component

Consistent, helpful empty states across the application.

## Purpose

Provide users with clear guidance when views have no content, helping them understand why and what to do next.

## Features

- Flexible icon/illustration display
- Clear title and description
- Optional call-to-action
- Three size variants (sm, md, lg)
- Dark mode support
- Accessibility compliant
- Preset variants for common use cases

## Usage

### Basic Usage

```tsx
import { EmptyState } from '@/components/EmptyState';
import { Search } from 'lucide-react';

<EmptyState
  icon={<Search />}
  title="No results found"
  description="Try adjusting your search query"
/>
```

### With Action

```tsx
<EmptyState
  icon={<FileIcon />}
  title="No files yet"
  description="Add files to get started"
  action={
    <Button onClick={handleAddFiles}>
      Add files
    </Button>
  }
/>
```

### Preset Variants

```tsx
import {
  NoSearchResults,
  NoFiles,
  NoTags,
  NoRecentDocuments,
  FirstTimeDaily,
  NoBacklinks,
} from '@/components/EmptyState';

// Search results
<NoSearchResults query={searchQuery} onClear={handleClear} />

// File browser
<NoFiles onAddFiles={handleAddFiles} />

// Tags
<NoTags />

// Recent documents
<NoRecentDocuments />

// Daily note
<FirstTimeDaily onCreate={handleCreate} />

// Backlinks
<NoBacklinks />
```

## Props

| Prop | Type | Default | Description |
|------|------|---------|-------------|
| `icon` | `ReactNode` | - | Icon to display (lucide-react or custom SVG) |
| `title` | `string` | required | Main heading text |
| `description` | `string` | - | Supporting description |
| `action` | `ReactNode` | - | Call-to-action button/link |
| `size` | `'sm' \| 'md' \| 'lg'` | `'md'` | Size variant |
| `className` | `string` | `''` | Additional CSS classes |
| `illustration` | `string` | - | Image URL (alternative to icon) |

## Size Variants

### Small (`sm`)
- Compact padding (py-8)
- 48px icon
- 16px title
- 14px description
- Use in: Sidebars, small panels

### Medium (`md`) - Default
- Standard padding (py-12)
- 64px icon
- 18px title
- 16px description
- Use in: Main content areas

### Large (`lg`)
- Generous padding (py-16)
- 80px icon
- 20px title
- 18px description
- Use in: Full-page empty states

## Design Decisions

### Why This Approach?

**Centered layout:** Draws attention to empty state without feeling like an error
**Icon + text pattern:** Universal recognition, works across cultures
**Optional action:** Guides users to next step when applicable
**Muted colors:** Doesn't compete with actual content

### Nine Dimensions

1. **Style:** Clean, minimal, non-threatening
2. **Motion:** Static (no animation needed for empty state)
3. **Voice:** Helpful, encouraging (not blaming)
4. **Space:** Generous padding, centered alignment
5. **Color:** Muted grays, subtle backgrounds
6. **Typography:** Clear hierarchy (title > description > action)
7. **Proportion:** Icon sized for visibility without domination
8. **Texture:** Soft circular icon background
9. **Ergonomics:** Action buttons properly sized (44x44px minimum)

## Accessibility

- `role="status"` for screen reader announcement
- `aria-label="Empty state"` for context
- `aria-hidden="true"` on decorative icons
- Semantic HTML (`h3` for title, `p` for description)
- Keyboard-accessible action buttons
- Sufficient color contrast (4.5:1 minimum)

## Integration Examples

### SearchInterface

```tsx
{results.length === 0 && !isLoading && (
  <NoSearchResults
    query={searchQuery}
    onClear={() => setSearchQuery('')}
  />
)}
```

### FileTree

```tsx
{files.length === 0 && !isLoading && (
  <NoFiles onAddFiles={handleOpenFilePicker} />
)}
```

### TagManager

```tsx
{tags.length === 0 && (
  <NoTags />
)}
```

### RecentDocuments

```tsx
{recentDocs.length === 0 && (
  <NoRecentDocuments />
)}
```

## Do's and Don'ts

### Do
- Use for genuinely empty states (not errors)
- Provide clear next actions
- Keep descriptions concise (1-2 sentences)
- Use appropriate size for context
- Test with screen readers

### Don't
- Use for loading states (see LoadingState)
- Use for errors (see ErrorBoundary)
- Write vague messages ("No data")
- Overload with multiple actions
- Use emojis as icons

## Related Components

- **LoadingState** - For loading content
- **ErrorBoundary** - For error states
- **WelcomeScreen** - For onboarding
- **Toast** - For transient messages
