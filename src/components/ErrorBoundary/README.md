# Error Boundary System

Render-error handling for the Lattice desktop application.

## Overview

Error boundaries catch errors thrown while React renders. They do not catch
failed IPC calls or rejected promises; those surface through React Query's
`error` state, toasts, or an `EmptyState` with a retry. Boundaries work at two
levels:

- **Root-level**: Catches anything the sections don't, and replaces the app with a full-page error
- **Section-level**: Isolates a route's errors so the rail and other surfaces keep working

## Where they are mounted

- `src/main.tsx` wraps `<App />` in `RootErrorBoundary`.
- `src/routes.tsx` passes a section boundary to each route's `Page` wrapper
  (`boundary={...}`), which wraps the lazily loaded surface and its `Suspense`:

| Route | Boundary | Section name shown |
|-------|----------|--------------------|
| `/search` | `SearchSectionErrorBoundary` | Search |
| `/files` | `FilesSectionErrorBoundary` | Library |
| `/chat` | `QASectionErrorBoundary` | Chat |
| `/explorer` | `ExplorerSectionErrorBoundary` | Explorer |
| `/studio` | `StudioSectionErrorBoundary` | Studio |
| `/settings` | `SettingsSectionErrorBoundary` | Settings |

Home, Import, Journal, References and Compare have no section boundary; an
error there reaches `RootErrorBoundary`. `DailySectionErrorBoundary` (section
name "Journal") is exported but not mounted.

## Components

### RootErrorBoundary

Top-level boundary. Renders `FullPageError`.

```tsx
// main.tsx
import { RootErrorBoundary } from './components/ErrorBoundary';

<RootErrorBoundary>
  <App />
</RootErrorBoundary>
```

**Props:** `children`, optional `onError`, optional `onReset`.

On reset it clears `window.queryClient` if one is exposed and removes the
`app-state` session-storage key before calling `onReset`. In development it
also writes the error to the console.

### SectionErrorBoundary

Section-level boundary for isolating feature errors. Renders `SectionError`
unless given a `fallback`.

```tsx
import { SectionErrorBoundary } from '@/components/ErrorBoundary';

<SectionErrorBoundary sectionName="Search" resetKeys={[searchQuery]}>
  <SearchInterface />
</SectionErrorBoundary>
```

**Props:**
- `sectionName` (required): Name shown in the fallback ("Couldn't load Search.")
- `icon`: Accepted and passed to `SectionError`, which does not currently render it
- `resetKeys`: Values that reset the boundary when they change
- `onError`: Custom error handler
- `onReset`: Custom reset handler
- `fallback`: Custom fallback component (receives `error`, `errorInfo`, `resetError`)

### Predefined Section Boundaries

Each is `SectionErrorBoundary` with a fixed `sectionName` and no reset keys:

```tsx
import {
  SearchSectionErrorBoundary,   // "Search"
  FilesSectionErrorBoundary,    // "Library"
  QASectionErrorBoundary,       // "Chat"
  ExplorerSectionErrorBoundary, // "Explorer"
  StudioSectionErrorBoundary,   // "Studio" (lives in StudioSectionErrorBoundary.tsx)
  SettingsSectionErrorBoundary, // "Settings"
  DailySectionErrorBoundary,    // "Journal"
} from '@/components/ErrorBoundary';
```

A new route gets its own one-line wrapper and a `boundary=` on its `Page`.

### ErrorBoundary (Base)

The class component the others build on. `withErrorBoundary(Component, props)`
wraps a component in one.

```tsx
import { ErrorBoundary } from '@/components/ErrorBoundary';

<ErrorBoundary
  name="MyComponent"
  fallback={CustomFallback}
  onError={(error, errorInfo) => console.log(error)}
  onReset={() => console.log('Reset')}
  resetKeys={[userId, viewId]}
  resetOnPropsChange={false}
  isolate
>
  <MyComponent />
</ErrorBoundary>
```

**Props:**
- `name`: Component name for logging and the isolated fallback's heading
- `fallback`: Custom fallback component (`ErrorFallbackProps`: `error`, `errorInfo?`, `resetError`)
- `onError`: Error callback
- `onReset`: Reset callback
- `resetKeys`: Array of dependencies that trigger reset
- `resetOnPropsChange`: Reset on any prop change
- `isolate`: With no `fallback`, render a small inline "Error in {name}" card with a Retry button; without it the boundary renders nothing

Every caught error goes to `logComponentError`. After more than five errors the
boundary refuses to reset, from keys or from **Try again**, to avoid a loop.

### Legacy feature boundaries

`FeatureErrorBoundary.tsx` still exports `SearchErrorBoundary`,
`FilesErrorBoundary`, `QAErrorBoundary` and `SettingsErrorBoundary` through the
index. Nothing mounts them; use the section boundaries above.

## Fallback Components

### FullPageError

Full-screen display used by `RootErrorBoundary`: "Lattice ran into a
problem.", a friendly message from `getUserFriendlyMessage`, and **Try again**,
**Reload** and **Copy details** buttons (plus **Download log** in
development). **Show details** reveals the error name and message (sanitized in
production); the stack and component stack appear only in development.

```tsx
import { FullPageError } from '@/components/ErrorBoundary';

<FullPageError error={error} errorInfo={errorInfo} resetError={() => {}} />
```

### SectionError

Centered in-section message: "Couldn't load {sectionName}.", the friendly
message, **Try again**, and **Dismiss** when `onDismiss` is passed. Technical
details are offered only in development.

```tsx
import { SectionError } from '@/components/ErrorBoundary';

<SectionError
  error={error}
  errorInfo={errorInfo}
  resetError={() => {}}
  sectionName="Search"
/>
```

### InlineError

Minimal display for compact spaces. Not currently used outside this folder.

```tsx
import { InlineError, CompactError } from '@/components/ErrorBoundary';

// Standard inline error
<InlineError error={error} resetError={() => {}} message="Custom message" />

// Compact variant (same as <InlineError compact />)
<CompactError error={error} resetError={() => {}} />
```

## Error Types

`src/types/errors.ts` defines error classes that the fallbacks translate into
user-facing copy via `getUserFriendlyMessage`; `sanitizeErrorMessage` strips
sensitive detail from a message for production display.

```tsx
import {
  NetworkError,       // (message, statusCode?, endpoint?)
  AuthenticationError,
  DatabaseError,      // (message, operation?, table?)
  RenderError,
  FileSystemError,
  SearchError,
  ConfigurationError,
} from '@/types/errors';
```

No code currently throws these; an error of any other type gets the generic
message.

## Error Logging

`src/utils/errorLogger.ts` is a compatibility facade over the local diagnostics
store (`src/utils/diagnostics.ts`). Nothing leaves the machine.

```tsx
import { logError, logComponentError } from '@/utils/errorLogger';

// Manual error logging
logError(error, {
  component: 'SearchInterface',
  action: 'search',
  additionalData: { filters },
});

// Component error logging (called automatically by error boundaries)
logComponentError(error, errorInfo, 'MyComponent');
```

**Features:**
- Entries are recorded in a bounded local store (300 entries; an immediate repeat bumps a count)
- Messages and details are redacted (keys, tokens, prompts, queries, user paths)
- `getRecentErrors`, `getErrorQueue`, `clearErrorQueue`, `exportErrorLog` and `downloadErrorLog` read or export the store

## Best Practices

### 1. Strategic Placement

```tsx
// Good: one boundary per route, through Page
<Page id="search" boundary={SearchSectionErrorBoundary}><SearchInterface /></Page>

// Bad: over-wrapping small components
<ErrorBoundary>
  <Button>Click me</Button>
</ErrorBoundary>
```

### 2. Reset Keys

When a boundary sits inside a view that stays mounted, give it reset keys so a
change of context recovers it:

```tsx
<SectionErrorBoundary sectionName="Document" resetKeys={[documentId]}>
  <DocumentPane documentId={documentId} />
</SectionErrorBoundary>
```

Route boundaries need none: navigating away unmounts them.

### 3. Log with context

```tsx
// Good: rich context
logError(error, { component: 'SearchInterface', action: 'search' });

// Bad: no context
console.error(error);
```

## Testing

```tsx
import { render, screen } from '@testing-library/react';
import { vi } from 'vitest';
import { ErrorBoundary } from '@/components/ErrorBoundary';

const ThrowError = () => {
  throw new Error('Test error');
};

test('catches errors and calls onError', () => {
  const onError = vi.fn();

  render(
    <ErrorBoundary name="Probe" onError={onError} isolate>
      <ThrowError />
    </ErrorBoundary>
  );

  expect(onError).toHaveBeenCalled();
  expect(screen.getByText(/Error in Probe/)).toBeDefined();
});
```

See `ErrorBoundary.test.tsx` and `StudioSectionErrorBoundary.test.tsx`.

## Error Recovery

Boundaries reset when:
- `resetKeys` change
- Props change (only if `resetOnPropsChange` is set)
- The user presses **Try again** / **Retry**

**Reload** in `FullPageError` does a hard refresh.

## Production vs Development

### Development Mode
- Stack and component stacks in the details panels
- Boundaries also log to the console
- **Download log** button on the full-page error

### Production Mode
- Sanitized messages in `FullPageError` details
- No stack traces shown to users
- Logging stays in the local, redacted diagnostics store

## Accessibility

- All actions are native `<button type="button">` elements
- Details toggles set `aria-expanded`
- Colors come from theme tokens
