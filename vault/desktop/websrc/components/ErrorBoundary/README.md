# Error Boundary System

Comprehensive error handling for the Recall/Vault desktop application.

## Overview

This error boundary system provides graceful error handling with recovery options at multiple levels:

- **Root-level**: Catches all uncaught errors in the entire app
- **Section-level**: Isolates errors to specific features (Search, Files, Settings, etc.)
- **Inline-level**: Minimal error displays for small components

## Components

### RootErrorBoundary

Top-level error boundary that wraps the entire application. Displays a full-screen error page with recovery options.

```tsx
// main.tsx
import { RootErrorBoundary } from './components/ErrorBoundary';

ReactDOM.createRoot(rootElement).render(
  <RootErrorBoundary>
    <App />
  </RootErrorBoundary>
);
```

**Features:**
- Full-screen error page
- Detailed error information (development mode)
- User-friendly message (production mode)
- Recovery options: Reload App, Try Again, Report Issue
- Copy error details button
- Automatic error logging

### SectionErrorBoundary

Section-level error boundary for isolating feature errors.

```tsx
import { SectionErrorBoundary } from './components/ErrorBoundary';

<SectionErrorBoundary
  sectionName="Search"
  resetKeys={[searchQuery]}
>
  <SearchInterface />
</SectionErrorBoundary>
```

**Props:**
- `sectionName`: Name of the section (e.g., "Search", "File Browser")
- `icon`: Optional custom icon
- `resetKeys`: Array of values that trigger automatic reset when changed
- `onError`: Custom error handler
- `onReset`: Custom reset handler
- `fallback`: Custom fallback component

**Features:**
- Inline error display that doesn't crash entire app
- Retry button
- Collapsible error details (dev mode)
- Automatic reset on navigation/state change

### Predefined Section Boundaries

```tsx
import {
  SearchSectionErrorBoundary,
  FilesSectionErrorBoundary,
  QASectionErrorBoundary,
  SettingsSectionErrorBoundary,
  DailySectionErrorBoundary,
} from './components/ErrorBoundary';

// Usage
<SearchSectionErrorBoundary>
  <SearchInterface />
</SearchSectionErrorBoundary>
```

### ErrorBoundary (Base)

Enhanced error boundary with advanced features.

```tsx
import { ErrorBoundary } from './components/ErrorBoundary';

<ErrorBoundary
  name="MyComponent"
  fallback={CustomFallback}
  onError={(error, errorInfo) => console.log(error)}
  onReset={() => console.log('Reset')}
  resetKeys={[userId, viewId]}
  resetOnPropsChange={false}
  isolate={true}
>
  <MyComponent />
</ErrorBoundary>
```

**Props:**
- `name`: Component name for logging
- `fallback`: Custom fallback component
- `onError`: Error callback
- `onReset`: Reset callback
- `resetKeys`: Array of dependencies that trigger reset
- `resetOnPropsChange`: Auto-reset on any prop change
- `isolate`: Show minimal error instead of throwing

## Fallback Components

### FullPageError

Full-screen error display for critical failures.

```tsx
import { FullPageError } from './components/ErrorBoundary';

<FullPageError
  error={error}
  errorInfo={errorInfo}
  resetError={() => {}}
/>
```

### SectionError

Inline error card for section failures.

```tsx
import { SectionError } from './components/ErrorBoundary';

<SectionError
  error={error}
  errorInfo={errorInfo}
  resetError={() => {}}
  sectionName="Search"
/>
```

### InlineError

Minimal error display for compact spaces.

```tsx
import { InlineError, CompactError } from './components/ErrorBoundary';

// Standard inline error
<InlineError
  error={error}
  resetError={() => {}}
  message="Custom message"
/>

// Compact variant
<CompactError error={error} resetError={() => {}} />
```

## Error Types

Custom error types for specific error handling strategies:

```tsx
import {
  NetworkError,
  AuthenticationError,
  DatabaseError,
  RenderError,
  FileSystemError,
  SearchError,
  ConfigurationError,
} from '../types/errors';

// Example usage
throw new NetworkError('Failed to fetch', 500, '/api/search');
throw new DatabaseError('Query failed', 'SELECT', 'documents');
```

## Error Logging

Integrated error logging with development/production modes:

```tsx
import { logError, logComponentError } from '../utils/errorLogger';

// Manual error logging
logError(error, {
  component: 'SearchInterface',
  action: 'search',
  additionalData: { query: 'test' }
});

// Component error logging (called automatically by error boundaries)
logComponentError(error, errorInfo, 'MyComponent');
```

**Features:**
- Console logging (development)
- External service integration (production) - Sentry placeholder
- Error sanitization (removes sensitive data)
- Local error queue for debugging
- Export error log functionality

## Development Tools

### ErrorSimulator

Development-only component for testing error boundaries.

```tsx
import { ErrorSimulator } from './components/ErrorBoundary';

// Add to app (only shows in development)
<ErrorSimulator />
```

**Features:**
- Trigger different error types
- Test sync and async errors
- Verify error boundary behavior
- Visual error testing interface

### Test Components

```tsx
import {
  DelayedErrorComponent,
  ClickToErrorComponent
} from './components/ErrorBoundary';

// Throws error after delay
<DelayedErrorComponent delayMs={1000} />

// Throws error on button click
<ClickToErrorComponent />
```

## Hooks

### useErrorRecovery

Hook for implementing retry logic with exponential backoff.

```tsx
import { useErrorRecovery } from '../hooks/useErrorRecovery';

const { executeWithRetry, isRetrying, retryCount } = useErrorRecovery({
  maxRetries: 3,
  baseDelay: 1000,
  onError: (error, attempt) => console.log(`Attempt ${attempt}`, error),
});

const fetchData = async () => {
  return executeWithRetry(async () => {
    const response = await fetch('/api/data');
    if (!response.ok) throw new Error('Fetch failed');
    return response.json();
  });
};
```

## Best Practices

### 1. Strategic Placement

```tsx
// ✅ Good: Wrap major sections
<SearchSectionErrorBoundary>
  <SearchInterface />
</SearchSectionErrorBoundary>

// ❌ Bad: Over-wrapping small components
<ErrorBoundary>
  <Button>Click me</Button>
</ErrorBoundary>
```

### 2. Reset Keys

Use reset keys to automatically recover when context changes:

```tsx
// ✅ Good: Reset when user or view changes
<SectionErrorBoundary resetKeys={[userId, currentView]}>
  <UserDashboard />
</SectionErrorBoundary>

// ❌ Bad: No reset keys means user is stuck in error state
<SectionErrorBoundary>
  <UserDashboard />
</SectionErrorBoundary>
```

### 3. Custom Error Messages

Provide user-friendly error messages:

```tsx
// ✅ Good: Helpful message
throw new NetworkError(
  'Unable to connect to search service. Please check your connection.',
  503
);

// ❌ Bad: Technical jargon
throw new Error('WebSocket connection to ws://localhost:3000 failed');
```

### 4. Error Logging

Always log errors with context:

```tsx
// ✅ Good: Rich context
logError(error, {
  component: 'SearchInterface',
  action: 'search',
  additionalData: { query, filters }
});

// ❌ Bad: No context
console.error(error);
```

### 5. Fallback UI

Use appropriate fallback for context:

```tsx
// ✅ Good: Full page for critical errors
<RootErrorBoundary>
  <App />
</RootErrorBoundary>

// ✅ Good: Inline for feature errors
<SectionErrorBoundary>
  <FeatureComponent />
</SectionErrorBoundary>

// ✅ Good: Compact for small spaces
<InlineError error={error} compact />
```

## Testing

### Manual Testing

1. Use the ErrorSimulator component (dev mode only)
2. Click different error types to test boundaries
3. Verify recovery mechanisms work
4. Check error logging output

### Programmatic Testing

```tsx
import { render, screen } from '@testing-library/react';
import { ErrorBoundary } from './components/ErrorBoundary';

const ThrowError = () => {
  throw new Error('Test error');
};

test('catches errors and shows fallback', () => {
  const onError = jest.fn();

  render(
    <ErrorBoundary onError={onError}>
      <ThrowError />
    </ErrorBoundary>
  );

  expect(onError).toHaveBeenCalled();
  expect(screen.getByText(/something went wrong/i)).toBeInTheDocument();
});
```

## Error Recovery Strategies

### 1. Auto-recovery

Error boundaries automatically reset when:
- resetKeys change (e.g., navigation, user change)
- Parent props change (if `resetOnPropsChange={true}`)

### 2. Manual Recovery

Users can trigger recovery via:
- Retry button (resets error boundary)
- Reload app button (hard refresh)
- Go home button (navigate to safe state)

### 3. Exponential Backoff

Use `useErrorRecovery` hook for automatic retries:
- First retry: 1 second delay
- Second retry: 2 seconds delay
- Third retry: 4 seconds delay
- Fails after max retries

## Production vs Development

### Development Mode
- Full stack traces shown
- Component stacks visible
- ErrorSimulator available
- Detailed logging to console
- Download error log button

### Production Mode
- Sanitized error messages
- No stack traces to users
- Error tracking service integration
- User-friendly messages only
- Privacy-protected logging

## Integration Checklist

- [x] RootErrorBoundary wraps app in main.tsx
- [x] Section boundaries around major features
- [x] Error types defined and imported
- [x] Error logger integrated
- [x] Development tools available
- [x] Reset keys configured
- [x] Custom fallback UIs implemented
- [x] Error recovery strategies in place
- [x] Tests written for error scenarios
- [x] Accessibility labels added

## Accessibility

All error components include:
- ARIA labels for screen readers
- Keyboard navigation support
- High contrast error states
- Clear focus indicators
- Reduced motion support

## Performance

Error boundaries are lightweight and have minimal impact:
- No performance overhead when no errors
- Lazy-loaded fallback components
- Efficient error logging
- Debounced retry mechanisms

## Future Enhancements

- [ ] Sentry integration (placeholder in place)
- [ ] Error analytics dashboard
- [ ] User feedback collection
- [ ] A/B testing different error messages
- [ ] Offline error queuing
- [ ] Error pattern detection
