# Toast System - Quick Reference

Quick copy-paste examples for common toast notification scenarios.

## Setup

```tsx
// 1. Import in your component
import { useToast } from './hooks/useToast';

// 2. Get toast function
const { toast } = useToast();

// 3. Use anywhere in component
toast.success('Operation successful!');
```

## Common Patterns

### Success Message

```tsx
toast.success('File uploaded successfully');
```

### Error with Details

```tsx
toast.error('Failed to save', {
  message: error.message,
  duration: 5000,
});
```

### Warning with Action

```tsx
toast.warning('Indexing paused', {
  message: 'Resume to keep vault updated',
  action: {
    label: 'Resume',
    onClick: () => handleResume(),
  },
});
```

### Info with No Auto-Dismiss

```tsx
toast.info('Update available', {
  message: 'Version 1.2.0',
  duration: 0, // Never auto-dismiss
});
```

### Delete with Undo

```tsx
const handleDelete = async (id) => {
  const backup = await getData(id);
  await deleteData(id);

  toast.success('Item deleted', {
    action: {
      label: 'Undo',
      onClick: () => restoreData(id, backup),
    },
    duration: 5000,
  });
};
```

### Async Operation with Loading

```tsx
import { showPromiseToast } from './utils/toast';

await showPromiseToast(uploadFile(file), {
  loading: 'Uploading...',
  success: 'Upload complete',
  error: 'Upload failed',
});
```

### Copy to Clipboard

```tsx
const handleCopy = async (text) => {
  await navigator.clipboard.writeText(text);
  toast.success('Copied to clipboard', { duration: 2000 });
};
```

### Form Validation

```tsx
if (!formData.email) {
  toast.error('Email is required');
  return;
}
```

### Network Status

```tsx
useEffect(() => {
  window.addEventListener('offline', () => {
    toast.warning('No internet connection', { duration: 0 });
  });

  window.addEventListener('online', () => {
    toast.success('Connection restored');
  });
}, []);
```

### Batch Operations

```tsx
const toastId = toast.info(`Processing ${items.length} items...`, {
  duration: 0,
});

// ... process items ...

toast.dismiss(toastId);
toast.success(`Processed ${items.length} items`);
```

## Configuration

### Change Position

```tsx
import { toastStore } from './stores/toastStore';

toastStore.updateConfig({
  position: 'bottom-right', // or top-left, top-center, etc.
});
```

### Change Duration

```tsx
toastStore.updateConfig({
  defaultDuration: 5000, // 5 seconds
});
```

### Max Toasts

```tsx
toastStore.updateConfig({
  maxToasts: 3, // Show max 3 at once
});
```

## Utility Functions (Non-React)

```tsx
import { showSuccessToast, showErrorToast } from './utils/toast';

// In any function
showSuccessToast('Settings saved');

// With error object
showErrorToast('Save failed', error);
```

## Manual Control

```tsx
// Save toast ID
const toastId = toast.info('Processing...');

// Dismiss specific toast
toast.dismiss(toastId);

// Dismiss all toasts
toast.dismissAll();
```

## Keyboard Shortcuts

- **Esc**: Dismiss all toasts
- **Tab**: Navigate to action buttons
- **Enter/Space**: Click action button

## Best Practices

### DO

```tsx
// ✅ Single toast for operation
toast.success('5 files saved');

// ✅ Meaningful messages
toast.error('Failed to save settings', {
  message: 'Check your permissions',
});

// ✅ Appropriate duration
toast.success('Saved', { duration: 2000 }); // Quick action
toast.error('Error', { duration: 5000 });   // Needs attention
```

### DON'T

```tsx
// ❌ Toast spam
files.forEach(f => toast.success(`${f.name} saved`));

// ❌ Vague messages
toast.error('Error');

// ❌ Too long duration
toast.info('Hello', { duration: 30000 }); // 30 seconds!
```

## Common Use Cases

| Scenario | Type | Duration | Action |
|----------|------|----------|--------|
| Save success | Success | 3s | No |
| Delete | Success | 5s | Undo |
| Upload | Promise | Auto | Retry on error |
| Validation error | Error | 4s | No |
| Network error | Error | 5s | Retry |
| Indexing paused | Warning | Never | Resume |
| Update available | Info | Never | Update |
| Copy to clipboard | Success | 2s | No |
| Batch complete | Success | 4s | No |
| Feature tip | Info | 6s | Learn more |

## Troubleshooting

### Toast not showing?

```tsx
// 1. Check ToastContainer is in App
<ToastContainer />

// 2. Verify toast is called
console.log('Toast called');
toast.success('Test');
```

### Toast won't dismiss?

```tsx
// Force dismiss all
import { toast } from './stores/toastStore';
toast.dismissAll();
```

### Need help?

Check full documentation: `docs/TOAST_SYSTEM.md`
