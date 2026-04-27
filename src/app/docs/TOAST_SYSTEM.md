# Toast Notification System

Complete guide for the toast notification system in Lattice/Lattice desktop application.

## Table of Contents

- [Overview](#overview)
- [Features](#features)
- [Installation](#installation)
- [Usage](#usage)
  - [Basic Usage](#basic-usage)
  - [Using the Hook](#using-the-hook)
  - [Using Utility Functions](#using-utility-functions)
- [Toast Types](#toast-types)
- [Configuration](#configuration)
- [Advanced Usage](#advanced-usage)
- [Integration Examples](#integration-examples)
- [Accessibility](#accessibility)
- [Performance](#performance)
- [Troubleshooting](#troubleshooting)

## Overview

The toast notification system provides user feedback for actions throughout the application. It features:

- 4 toast types (success, error, warning, info)
- Auto-dismiss with configurable duration
- Manual dismiss with close button
- Optional action buttons
- Pause on hover
- Queue management (max concurrent toasts)
- Configurable positioning
- Smooth animations
- Full accessibility support (WCAG AA compliant)

## Features

### Core Features

- **4 Toast Types**: Success, Error, Warning, Info
- **Auto-dismiss**: Configurable duration (default 4000ms)
- **Manual Dismiss**: Close button on each toast (if dismissible)
- **Action Buttons**: Optional action button with custom handler
- **Pause on Hover**: Auto-dismiss pauses when hovering
- **Queue Management**: Maximum 5 concurrent toasts (configurable)
- **Position Configuration**: 6 positions (top/bottom × left/center/right)
- **Progress Bar**: Visual countdown for auto-dismiss
- **Icon Indicators**: Type-specific icons

### Design Features

- Clean, modern design matching app theme
- Color-coded by type
- Drop shadow for depth
- Smooth animations (300ms)
- Responsive design
- Dark mode support

### Accessibility Features

- WCAG AA compliant
- ARIA live regions
- Keyboard navigation (Tab, Esc)
- Screen reader announcements
- Focus management
- Reduced motion support

## Installation

The toast system is already integrated into the app. To use it in your components:

### 1. Add ToastContainer to App

```tsx
// src/App.tsx
import { ToastContainer } from './components/Toast';

function App() {
  return (
    <div>
      {/* Your app content */}
      <ToastContainer />
    </div>
  );
}
```

### 2. Import in Your Component

```tsx
// Using the hook
import { useToast } from './hooks/useToast';

// Or using utility functions
import { showSuccessToast, showErrorToast } from './utils/toast';
```

## Usage

### Basic Usage

#### Using the Hook (Recommended for Components)

```tsx
import { useToast } from './hooks/useToast';

function MyComponent() {
  const { toast } = useToast();

  const handleSave = async () => {
    try {
      await saveData();
      toast.success('Data saved successfully');
    } catch (error) {
      toast.error('Failed to save data', {
        message: error.message,
        duration: 5000,
      });
    }
  };

  return <button onClick={handleSave}>Save</button>;
}
```

#### Using Utility Functions (Recommended for Non-React Code)

```ts
import { showSuccessToast, showErrorToast } from './utils/toast';

async function saveSettings(settings) {
  try {
    await api.save(settings);
    showSuccessToast('Settings saved');
  } catch (error) {
    showErrorToast('Failed to save settings', error);
  }
}
```

## Toast Types

### Success

```tsx
toast.success('File uploaded successfully');

toast.success('File uploaded', {
  message: 'Your file is now available in the lattice',
  duration: 3000,
});
```

**Use for:**
- Successful operations
- Confirmations
- Achievements

### Error

```tsx
toast.error('Failed to delete file');

toast.error('Upload failed', {
  message: 'Network connection error',
  duration: 5000,
  action: {
    label: 'Retry',
    onClick: () => handleRetry(),
  },
});
```

**Use for:**
- Failed operations
- Validation errors
- Network errors
- Permission errors

### Warning

```tsx
toast.warning('Indexing paused');

toast.warning('Large file detected', {
  message: 'This file may take a while to process',
  duration: 6000,
});
```

**Use for:**
- Non-critical issues
- User attention needed
- State changes that require awareness

### Info

```tsx
toast.info('Indexing complete');

toast.info('New update available', {
  message: 'Version 1.2.0 is ready to install',
  action: {
    label: 'Update Now',
    onClick: () => handleUpdate(),
  },
  duration: 0, // Don't auto-dismiss
});
```

**Use for:**
- General information
- Status updates
- Tips and hints
- Feature announcements

## Configuration

### Global Configuration

```tsx
import { toastStore } from './stores/toastStore';

// Update configuration
toastStore.updateConfig({
  position: 'bottom-right',
  maxToasts: 3,
  defaultDuration: 5000,
  pauseOnHover: false,
});
```

### Toast Options

```tsx
interface ToastOptions {
  message?: string;           // Optional secondary message
  duration?: number;          // Auto-dismiss duration (0 = no auto-dismiss)
  dismissible?: boolean;      // Show close button (default: true)
  icon?: React.ReactNode;     // Custom icon (default: type-specific icon)
  action?: {
    label: string;
    onClick: () => void;
  };
}
```

### Position Options

- `top-right` (default)
- `top-left`
- `top-center`
- `bottom-right`
- `bottom-left`
- `bottom-center`

## Advanced Usage

### Promise-Based Toast

Show loading, success, and error states automatically:

```tsx
import { showPromiseToast } from './utils/toast';

const handleUpload = async (files) => {
  await showPromiseToast(uploadFiles(files), {
    loading: 'Uploading files...',
    success: (result) => `Uploaded ${result.count} files`,
    error: 'Upload failed',
  });
};
```

### Manual Dismiss

```tsx
const toastId = toast.info('Processing...', { duration: 0 });

// Later...
toast.dismiss(toastId);

// Or dismiss all
toast.dismissAll();
```

### Undo Actions

```tsx
const handleDelete = async (fileId) => {
  const fileData = await getFileData(fileId);
  await deleteFile(fileId);

  toast.success('File deleted', {
    action: {
      label: 'Undo',
      onClick: async () => {
        await restoreFile(fileId, fileData);
        toast.success('File restored');
      },
    },
    duration: 5000,
  });
};
```

### Batch Operations

```tsx
const handleBatchOperation = async (items) => {
  const toastId = toast.info(`Processing ${items.length} items...`, {
    duration: 0,
  });

  try {
    for (let i = 0; i < items.length; i++) {
      await processItem(items[i]);
      toast.dismiss(toastId);
      toast.info(`Processing... (${i + 1}/${items.length})`);
    }

    toast.dismiss(toastId);
    toast.success(`Successfully processed ${items.length} items`);
  } catch (error) {
    toast.dismiss(toastId);
    toast.error('Batch operation failed');
  }
};
```

### Custom Icons

```tsx
import { CustomIcon } from './icons';

toast.info('Custom notification', {
  icon: <CustomIcon />,
});
```

## Integration Examples

See `src/examples/ToastIntegrationExamples.tsx` for complete examples including:

1. Settings Save
2. File Upload
3. File Delete with Undo
4. Indexing Status
5. Form Validation Errors
6. Update Available
7. Batch Operations
8. Copy to Clipboard
9. Network Connection Status
10. Keyboard Shortcut Feedback

## Accessibility

### Keyboard Navigation

- **Tab**: Focus on toast action buttons
- **Esc**: Dismiss all toasts
- **Enter/Space**: Activate focused button

### Screen Reader Support

- Toast container has `role="region"` and `aria-label="Notifications"`
- Each toast has `role="status"` and `aria-live="polite"`
- Progress bars have `role="progressbar"` with appropriate ARIA attributes

### Reduced Motion

The system respects `prefers-reduced-motion` media query:

```css
@media (prefers-reduced-motion: reduce) {
  .animate-slide-in {
    animation: none;
  }
}
```

### Color Contrast

All toast types meet WCAG AA contrast requirements:
- Text contrast: 4.5:1 minimum
- Icon contrast: 3:1 minimum

## Performance

### Optimization Strategies

1. **Efficient Re-renders**: Uses subscription-based state management
2. **Cleanup**: Auto-cleanup of dismissed toasts
3. **Throttling**: Queue management prevents toast spam
4. **GPU Acceleration**: Uses transform for animations
5. **Lazy Rendering**: Only renders active toasts

### Best Practices

```tsx
// ✅ Good: Single toast for operation
toast.success('All files saved');

// ❌ Bad: Multiple toasts in loop
files.forEach(file => {
  toast.success(`${file.name} saved`); // Spam!
});

// ✅ Good: Batch with summary
await Promise.all(files.map(saveFile));
toast.success(`${files.length} files saved`);
```

## Troubleshooting

### Toast Not Appearing

**Problem**: Toast called but nothing shows up

**Solutions**:
1. Verify `<ToastContainer />` is in your component tree
2. Check z-index conflicts (toast uses z-index: 9999)
3. Check if max toast limit reached (default: 5)

```tsx
// Check if container is rendered
import { useToastStore } from './stores/toastStore';

function Debug() {
  const { toasts } = useToastStore();
  console.log('Active toasts:', toasts);
  return null;
}
```

### Toast Duration Not Working

**Problem**: Toast doesn't auto-dismiss or dismisses too quickly

**Solutions**:
1. Check if `duration: 0` was set (prevents auto-dismiss)
2. Check if `pauseOnHover` is enabled and mouse is hovering
3. Verify duration value is in milliseconds (not seconds)

```tsx
// ✅ Correct: 5 seconds
toast.success('Saved', { duration: 5000 });

// ❌ Wrong: 5 milliseconds
toast.success('Saved', { duration: 5 });
```

### Animations Not Smooth

**Problem**: Toast animations are janky

**Solutions**:
1. Check for heavy render operations in parent components
2. Verify GPU acceleration is working (use Chrome DevTools Performance)
3. Check if `prefers-reduced-motion` is enabled

### Multiple Toasts Overlapping

**Problem**: Toasts stack incorrectly or overlap

**Solutions**:
1. Check CSS specificity conflicts
2. Verify position configuration is correct
3. Check if custom styles are interfering

```tsx
// Reset to defaults
toastStore.updateConfig({
  position: 'top-right',
  maxToasts: 5,
});
```

### Toast Not Dismissing

**Problem**: Toast stuck on screen

**Solutions**:
1. Check if `duration: 0` was set
2. Verify `dismissible: true` (default)
3. Check if JavaScript errors are preventing cleanup

```tsx
// Force dismiss all
import { toast } from './stores/toastStore';
toast.dismissAll();
```

## API Reference

### `useToast()` Hook

```tsx
const { toast } = useToast();

toast.success(title, options?)
toast.error(title, options?)
toast.warning(title, options?)
toast.info(title, options?)
toast.dismiss(id)
toast.dismissAll()
```

### Utility Functions

```tsx
showSuccessToast(message, options?)
showErrorToast(message, error?, options?)
showWarningToast(message, options?)
showInfoToast(message, options?)
showPromiseToast(promise, messages)
dismissToast(id)
dismissAllToasts()
```

### Store API

```tsx
import { toastStore } from './stores/toastStore';

toastStore.addToast(toast)
toastStore.dismissToast(id)
toastStore.dismissAll()
toastStore.updateConfig(config)
toastStore.subscribe(listener)
```

## Migration Guide

### From ErrorToast to Toast System

If you're using the existing `ErrorToastContainer`, you can migrate gradually:

```tsx
// Old
<ErrorToastContainer errors={errors} onDismiss={dismissError} />

// New (both can coexist)
<ErrorToastContainer errors={errors} onDismiss={dismissError} />
<ToastContainer />

// Eventually replace ErrorToastContainer with toast.error()
useEffect(() => {
  if (error) {
    toast.error('Operation failed', { message: error.message });
  }
}, [error]);
```

## Support

For issues or questions:
1. Check this documentation
2. Review examples in `src/examples/ToastIntegrationExamples.tsx`
3. Check existing implementations in the codebase
4. Open an issue in the project repository

## License

MIT License - See project LICENSE file for details
