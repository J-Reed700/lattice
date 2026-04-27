# Quick Reference Guide

## Common Patterns & Code Snippets

This guide provides quick copy-paste examples for common UX patterns in the Lattice/Lattice application.

---

## Loading States

### Skeleton Loaders

```tsx
import { Skeleton, SkeletonText, SkeletonCard, SkeletonList } from '../ui/Skeleton';

// Single line text
<Skeleton variant="text" width="80%" />

// Multi-line text
<SkeletonText lines={3} lastLineWidth="60%" />

// Card placeholder
<SkeletonCard showAvatar={true} showActions={true} />

// List of cards
<SkeletonList count={5} />

// Custom skeleton
<Skeleton variant="rect" width={200} height={100} />
```

### Component Loading Pattern

```tsx
function MyComponent() {
  const [data, setData] = useState(null);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    loadData();
  }, []);

  if (isLoading) {
    return <SkeletonList count={3} />;
  }

  if (!data) {
    return <ErrorState />;
  }

  return <DataView data={data} />;
}
```

---

## Empty States

### Basic Empty State

```tsx
function EmptyState({ title, description, action, onAction }) {
  return (
    <div className="flex flex-col items-center justify-center py-12 px-4">
      <div className="w-16 h-16 bg-gray-100 dark:bg-gray-800 rounded-full flex items-center justify-center mb-4">
        <SearchIcon className="w-8 h-8 text-gray-400 dark:text-gray-500" />
      </div>
      <h3 className="text-lg font-semibold text-gray-900 dark:text-gray-100 mb-2">
        {title}
      </h3>
      <p className="text-sm text-gray-600 dark:text-gray-400 text-center max-w-md mb-6">
        {description}
      </p>
      {action && (
        <Button onClick={onAction}>{action}</Button>
      )}
    </div>
  );
}

// Usage
<EmptyState
  title="No results found"
  description="Try a different search term or adjust your filters"
  action="Clear Filters"
  onAction={handleClearFilters}
/>
```

---

## Error Handling

### Error State Component

```tsx
function ErrorState({ title, message, onRetry, onDismiss }) {
  return (
    <div className="bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-lg p-4">
      <div className="flex items-start gap-3">
        <AlertCircle className="w-5 h-5 text-red-600 dark:text-red-400 flex-shrink-0 mt-0.5" />
        <div className="flex-1 min-w-0">
          <p className="text-sm font-medium text-red-900 dark:text-red-100 mb-1">
            {title}
          </p>
          <p className="text-sm text-red-600 dark:text-red-400 break-words">
            {message}
          </p>
        </div>
        <div className="flex gap-2">
          {onRetry && (
            <Button variant="ghost" size="sm" onClick={onRetry}>
              Retry
            </Button>
          )}
          {onDismiss && (
            <button
              onClick={onDismiss}
              className="text-red-600 dark:text-red-400 hover:text-red-700"
            >
              <X className="w-4 h-4" />
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
```

### Try-Catch with Error State

```tsx
const [error, setError] = useState(null);

async function handleAction() {
  setError(null);
  try {
    await performAction();
  } catch (err) {
    setError(err.message || 'Something went wrong');
  }
}

return (
  <>
    {error && (
      <ErrorState
        title="Action Failed"
        message={error}
        onRetry={handleAction}
        onDismiss={() => setError(null)}
      />
    )}
    <Button onClick={handleAction}>Perform Action</Button>
  </>
);
```

---

## Text Utilities

### Truncate Long Text

```tsx
import { truncate, formatPath, formatFileSize } from '../../lib/utils';

// Truncate at end
const shortText = truncate(longText, 50, 'end');
// "This is a very long text that will be trunca..."

// Truncate in middle (best for filenames)
const fileName = truncate(veryLongFileName, 60, 'middle');
// "Very_long_file_name_with_lots_of...details_v2.pdf"

// Format file path
const path = formatPath('/very/long/path/to/file.txt', 3);
// ".../long/path/to/file.txt"

// Format file size
const size = formatFileSize(1234567890);
// "1.15 GB"
```

### Expandable Text Component

```tsx
function ExpandableText({ text, maxLength = 100 }) {
  const [isExpanded, setIsExpanded] = useState(false);
  const needsTruncation = text.length > maxLength;

  const displayText = needsTruncation && !isExpanded
    ? truncate(text, maxLength, 'end')
    : text;

  return (
    <div>
      <p className="break-words">{displayText}</p>
      {needsTruncation && (
        <button
          onClick={() => setIsExpanded(!isExpanded)}
          className="text-sm text-blue-600 hover:underline"
        >
          {isExpanded ? 'Show less' : 'Show more'}
        </button>
      )}
    </div>
  );
}
```

---

## Form Validation

### Input with Validation

```tsx
import Input from '../ui/Input';
import { validateRequired, validateEmail } from '../../lib/utils';

function MyForm() {
  const [email, setEmail] = useState('');
  const [error, setError] = useState('');

  const validateEmailField = (value) => {
    const required = validateRequired(value, 'Email');
    if (!required.valid) return required.message;

    const email = validateEmail(value);
    if (!email.valid) return email.message;
  };

  return (
    <Input
      label="Email Address"
      value={email}
      onChange={(e) => setEmail(e.target.value)}
      error={error}
      validateOnBlur={true}
      validate={validateEmailField}
      showClearButton={true}
      onClear={() => setEmail('')}
      placeholder="you@example.com"
    />
  );
}
```

### File Upload with Validation

```tsx
import { validateFileSize, validateFileType, formatFileSize } from '../../lib/utils';

function validateFile(file) {
  // Check size (max 1GB)
  const sizeCheck = validateFileSize(file.size, 1024);
  if (!sizeCheck.valid) {
    return {
      valid: false,
      error: `${file.name}: ${sizeCheck.message} (${formatFileSize(file.size)})`
    };
  }

  // Check type
  const typeCheck = validateFileType(file.name, ['.pdf', '.docx', '.txt']);
  if (!typeCheck.valid) {
    return {
      valid: false,
      error: `${file.name}: ${typeCheck.message}`
    };
  }

  return { valid: true };
}

function handleFileSelect(file) {
  const validation = validateFile(file);
  if (!validation.valid) {
    setError(validation.error);
    return;
  }

  // Proceed with upload
  uploadFile(file);
}
```

---

## Async Utilities

### Debounced Search

```tsx
import { debounce } from '../../lib/utils';

function SearchInput() {
  const [query, setQuery] = useState('');

  // Debounce search by 300ms
  const debouncedSearch = useMemo(
    () => debounce((value) => {
      performSearch(value);
    }, 300),
    []
  );

  const handleChange = (e) => {
    const value = e.target.value;
    setQuery(value);
    debouncedSearch(value);
  };

  return (
    <Input
      value={query}
      onChange={handleChange}
      placeholder="Search..."
    />
  );
}
```

### Retry with Backoff

```tsx
import { retry, withTimeout } from '../../lib/utils';

async function fetchWithRetry() {
  try {
    // Retry up to 3 times with exponential backoff
    const result = await retry(
      async () => {
        const response = await fetch('/api/data');
        if (!response.ok) throw new Error('Request failed');
        return response.json();
      },
      3, // max retries
      1000 // base delay (ms)
    );
    return result;
  } catch (error) {
    console.error('All retries failed:', error);
    throw error;
  }
}

// With timeout
async function fetchWithTimeout() {
  try {
    const result = await withTimeout(
      fetch('/api/data'),
      5000, // 5 second timeout
      'Request timed out'
    );
    return result;
  } catch (error) {
    console.error(error);
  }
}
```

---

## Button States

### Button with Loading

```tsx
function MyComponent() {
  const [isLoading, setIsLoading] = useState(false);

  async function handleSubmit() {
    setIsLoading(true);
    try {
      await saveData();
    } finally {
      setIsLoading(false);
    }
  }

  return (
    <Button
      onClick={handleSubmit}
      disabled={isLoading}
      isLoading={isLoading}
    >
      {isLoading ? 'Saving...' : 'Save Changes'}
    </Button>
  );
}
```

### Button with Confirmation

```tsx
function DeleteButton({ onDelete, itemName }) {
  const [showConfirm, setShowConfirm] = useState(false);

  return (
    <>
      <Button
        variant="danger"
        onClick={() => setShowConfirm(true)}
      >
        Delete
      </Button>

      <Dialog
        open={showConfirm}
        onOpenChange={setShowConfirm}
        title="Confirm Deletion"
        description={`Are you sure you want to delete "${itemName}"? This cannot be undone.`}
        primaryAction={{
          label: 'Delete',
          onClick: () => {
            onDelete();
            setShowConfirm(false);
          },
          variant: 'danger'
        }}
        secondaryAction={{
          label: 'Cancel',
          onClick: () => setShowConfirm(false)
        }}
      />
    </>
  );
}
```

---

## Toast Notifications

### Using Toast Hook

```tsx
import { useToast, ToastContainer } from '../ui/Toast';

function MyComponent() {
  const { success, error, warning, info } = useToast();

  async function handleAction() {
    try {
      await performAction();
      success('Action completed successfully!');
    } catch (err) {
      error('Action failed: ' + err.message);
    }
  }

  return (
    <>
      <Button onClick={handleAction}>Perform Action</Button>
      <ToastContainer />
    </>
  );
}
```

---

## Accessibility Patterns

### Accessible Button

```tsx
// Icon button with accessible label
<Button aria-label="Close dialog">
  <X className="w-4 h-4" />
</Button>

// Button with loading state
<Button
  disabled={isLoading}
  aria-busy={isLoading}
  aria-label={isLoading ? 'Loading...' : 'Submit form'}
>
  {isLoading ? 'Loading...' : 'Submit'}
</Button>
```

### Accessible Form

```tsx
<form onSubmit={handleSubmit}>
  <Input
    label="Email"
    type="email"
    aria-required="true"
    aria-invalid={hasError}
    aria-describedby={hasError ? 'email-error' : 'email-help'}
  />
  {hasError && (
    <p id="email-error" role="alert">
      {errorMessage}
    </p>
  )}
  {!hasError && (
    <p id="email-help">
      We'll never share your email
    </p>
  )}

  <Button type="submit">Submit</Button>
</form>
```

### Live Region for Announcements

```tsx
// Search results announcement
<div
  role="status"
  aria-live="polite"
  aria-atomic="true"
>
  {isLoading && (
    <span className="sr-only">Loading search results...</span>
  )}
  {!isLoading && results.length > 0 && (
    <span className="sr-only">
      Found {results.length} results
    </span>
  )}
</div>
```

---

## Performance Patterns

### Memoization

```tsx
import { memo, useMemo, useCallback } from 'react';

// Memoize component
const ExpensiveComponent = memo(({ data, onClick }) => {
  return <div onClick={onClick}>{data.name}</div>;
});

// Memoize computation
function MyComponent({ items }) {
  const sortedItems = useMemo(
    () => items.sort((a, b) => b.score - a.score),
    [items]
  );

  const handleClick = useCallback((item) => {
    console.log('Clicked:', item);
  }, []);

  return (
    <>
      {sortedItems.map(item => (
        <ExpensiveComponent
          key={item.id}
          data={item}
          onClick={handleClick}
        />
      ))}
    </>
  );
}
```

---

## Responsive Design

### Mobile-First Breakpoints

```tsx
// Stack on mobile, row on desktop
<div className="flex flex-col md:flex-row gap-4">
  <div className="w-full md:w-1/2">Column 1</div>
  <div className="w-full md:w-1/2">Column 2</div>
</div>

// Hide on mobile, show on desktop
<div className="hidden md:block">
  Desktop only content
</div>

// Show on mobile, hide on desktop
<div className="block md:hidden">
  Mobile only content
</div>
```

---

## Edge Case Handling

### Long Filename Display

```tsx
import { truncate } from '../../lib/utils';

function FileDisplay({ fileName, path }) {
  const [showFullPath, setShowFullPath] = useState(false);

  const displayName = fileName.length > 60
    ? truncate(fileName, 60, 'middle')
    : fileName;

  const displayPath = showFullPath
    ? path
    : formatPath(path, 3);

  return (
    <div>
      <h3
        className="break-words"
        title={fileName.length > 60 ? fileName : undefined}
      >
        {displayName}
      </h3>
      <div className="flex items-center gap-2">
        <p className="text-sm font-mono break-all">
          {displayPath}
        </p>
        {path.length > 50 && (
          <button
            onClick={() => setShowFullPath(!showFullPath)}
            className="text-xs text-blue-600 hover:underline"
          >
            {showFullPath ? 'less' : 'more'}
          </button>
        )}
      </div>
    </div>
  );
}
```

---

## Common Tailwind Patterns

### Card Layout
```tsx
<div className="bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg p-6 shadow-sm">
  Card content
</div>
```

### Hover Effect
```tsx
<div className="transition-all duration-150 hover:shadow-md hover:scale-[1.02]">
  Hover me
</div>
```

### Focus Ring
```tsx
<button className="focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:ring-offset-2">
  Keyboard accessible
</button>
```

### Truncate Text
```tsx
<p className="truncate">
  This text will truncate with ellipsis
</p>

<p className="line-clamp-3">
  This text will show 3 lines max
</p>
```

---

## Testing Utilities

### Component Testing

```tsx
import { render, screen, fireEvent, waitFor } from '@testing-library/react';

test('button shows loading state', async () => {
  const handleClick = jest.fn();
  render(<Button onClick={handleClick}>Click me</Button>);

  const button = screen.getByRole('button', { name: 'Click me' });
  fireEvent.click(button);

  await waitFor(() => {
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute('aria-busy', 'true');
  });
});
```

---

## Useful VS Code Snippets

Add these to your `.vscode/snippets.json`:

```json
{
  "Empty State": {
    "prefix": "empty-state",
    "body": [
      "<div className=\"flex flex-col items-center justify-center py-12 px-4\">",
      "  <div className=\"w-16 h-16 bg-gray-100 dark:bg-gray-800 rounded-full flex items-center justify-center mb-4\">",
      "    <${1:Icon} className=\"w-8 h-8 text-gray-400 dark:text-gray-500\" />",
      "  </div>",
      "  <h3 className=\"text-lg font-semibold text-gray-900 dark:text-gray-100 mb-2\">",
      "    ${2:Title}",
      "  </h3>",
      "  <p className=\"text-sm text-gray-600 dark:text-gray-400 text-center max-w-md\">",
      "    ${3:Description}",
      "  </p>",
      "</div>"
    ]
  }
}
```

---

## Quick Commands

```bash
# Start dev server
npm run dev

# Run tests
npm test

# Run accessibility tests
npm run test:a11y

# Build for production
npm run build

# Lint code
npm run lint

# Format code
npm run format
```

---

**For more details, see:**
- [Accessibility Guide](./ACCESSIBILITY.md)
- [UX Improvements](./UX_IMPROVEMENTS.md)
- [Full Summary](./SUMMARY.md)
