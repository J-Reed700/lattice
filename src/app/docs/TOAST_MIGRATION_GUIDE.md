# Toast System Migration Guide

This guide helps you migrate from the simple UI Toast component to the new comprehensive Toast system.

## Why Migrate?

The new toast system provides:

- **Global State Management**: Toasts work across the entire app without prop drilling
- **More Features**: Action buttons, pause on hover, promise toasts, undo actions
- **Better Performance**: Efficient re-renders with subscription-based state
- **Queue Management**: Automatic handling of multiple toasts
- **Enhanced Accessibility**: Full WCAG AA compliance
- **Progress Bars**: Visual countdown for auto-dismiss
- **Better Configuration**: Position, duration, max toasts, etc.

## Comparison

### Old System (UI Component)

```tsx
// Each component manages its own toasts
import { useToast, ToastContainer } from './components/ui/Toast';

function MyComponent() {
  const { toasts, success, error, dismissToast } = useToast();

  return (
    <>
      <button onClick={() => success('Saved')}>Save</button>
      <ToastContainer toasts={toasts} onDismiss={dismissToast} />
    </>
  );
}
```

**Limitations:**
- Local state only (toasts per component)
- No action buttons
- No global configuration
- Basic features only
- Each component needs ToastContainer

### New System (Global Store)

```tsx
// Global toast system - works everywhere
import { useToast } from './hooks/useToast';

function MyComponent() {
  const { toast } = useToast();

  return (
    <button onClick={() => toast.success('Saved')}>Save</button>
  );
}

// App.tsx - ToastContainer once at root
import { ToastContainer } from './components/Toast';

function App() {
  return (
    <>
      {/* App content */}
      <ToastContainer />
    </>
  );
}
```

**Benefits:**
- Global state (toasts work everywhere)
- Action buttons, undo, promise toasts
- Configurable position, duration, max toasts
- Progress bars, pause on hover
- Single ToastContainer at root

## Migration Steps

### Step 1: Add New ToastContainer to App

```tsx
// src/App.tsx
import { ToastContainer } from './components/Toast';

function App() {
  return (
    <ThemeProvider>
      {/* Your app content */}

      {/* Add at the end, before closing tags */}
      <ToastContainer />
    </ThemeProvider>
  );
}
```

### Step 2: Update Imports

```tsx
// Old
import { useToast, ToastContainer } from './components/ui/Toast';

// New
import { useToast } from './hooks/useToast';
// No need to import ToastContainer in every component
```

### Step 3: Update Component Code

**Old Code:**
```tsx
function MyComponent() {
  const { toasts, success, error, dismissToast } = useToast();

  const handleSave = async () => {
    try {
      await saveData();
      success('Saved successfully');
    } catch (err) {
      error('Save failed');
    }
  };

  return (
    <>
      <button onClick={handleSave}>Save</button>
      <ToastContainer toasts={toasts} onDismiss={dismissToast} />
    </>
  );
}
```

**New Code:**
```tsx
function MyComponent() {
  const { toast } = useToast();

  const handleSave = async () => {
    try {
      await saveData();
      toast.success('Saved successfully');
    } catch (err) {
      toast.error('Save failed', {
        message: err.message, // Optional details
      });
    }
  };

  return <button onClick={handleSave}>Save</button>;
}
```

### Step 4: Remove Local ToastContainers

Remove all `<ToastContainer />` instances from individual components. You only need one at the root (App.tsx).

```tsx
// Remove these from components:
<ToastContainer toasts={toasts} onDismiss={dismissToast} />
```

### Step 5: Utilize New Features

#### Action Buttons

```tsx
// Old: No support for actions
success('File deleted');

// New: Add undo action
toast.success('File deleted', {
  action: {
    label: 'Undo',
    onClick: () => restoreFile(),
  },
});
```

#### Promise Toasts

```tsx
// Old: Manual handling
const handleUpload = async () => {
  try {
    info('Uploading...');
    await uploadFile();
    dismissToast(toastId);
    success('Upload complete');
  } catch (err) {
    dismissToast(toastId);
    error('Upload failed');
  }
};

// New: Automatic handling
import { showPromiseToast } from './utils/toast';

const handleUpload = async () => {
  await showPromiseToast(uploadFile(), {
    loading: 'Uploading...',
    success: 'Upload complete',
    error: 'Upload failed',
  });
};
```

#### Additional Details

```tsx
// Old: Single message only
error('Upload failed');

// New: Title + details
toast.error('Upload failed', {
  message: 'Network connection error. Please check your internet.',
  duration: 5000,
});
```

## Side-by-Side Examples

### Simple Success Message

```tsx
// Old
success('Settings saved');

// New
toast.success('Settings saved');
```

### Error with Details

```tsx
// Old
error(`Error: ${err.message}`);

// New
toast.error('Operation failed', {
  message: err.message,
});
```

### Delete with Undo

```tsx
// Old: Not possible
success('File deleted');

// New
toast.success('File deleted', {
  action: {
    label: 'Undo',
    onClick: () => restoreFile(),
  },
  duration: 5000,
});
```

### Persistent Notification

```tsx
// Old: duration must be manually managed
info('Indexing paused', 999999);

// New
toast.warning('Indexing paused', {
  duration: 0, // Never auto-dismiss
  action: {
    label: 'Resume',
    onClick: () => resumeIndexing(),
  },
});
```

## Gradual Migration Strategy

You can migrate gradually by running both systems side-by-side:

### Phase 1: Add New System

1. Add `<ToastContainer />` to App.tsx
2. New features use new toast system
3. Old code continues using old system

### Phase 2: Migrate Components

1. Update one component at a time
2. Test thoroughly
3. Remove local ToastContainers

### Phase 3: Clean Up

1. Remove old ui/Toast component (once all migrations complete)
2. Update imports everywhere
3. Remove old dependencies

## Testing Migration

### Checklist

- [ ] ToastContainer added to App.tsx
- [ ] All imports updated
- [ ] Local ToastContainers removed
- [ ] Toasts appear in correct position
- [ ] Auto-dismiss works
- [ ] Manual dismiss works
- [ ] Multiple toasts stack correctly
- [ ] Esc key dismisses all
- [ ] Dark mode styling correct
- [ ] Accessibility features work

### Test Each Toast Type

```tsx
// Add test buttons during development
<div>
  <button onClick={() => toast.success('Test success')}>Success</button>
  <button onClick={() => toast.error('Test error')}>Error</button>
  <button onClick={() => toast.warning('Test warning')}>Warning</button>
  <button onClick={() => toast.info('Test info')}>Info</button>
</div>
```

## Common Issues

### Issue: Toasts not appearing

**Cause**: ToastContainer not in component tree

**Solution**: Add `<ToastContainer />` to App.tsx

### Issue: Multiple toast containers

**Cause**: Old ToastContainers still in components

**Solution**: Remove all except the one in App.tsx

### Issue: TypeScript errors

**Cause**: Import from wrong location

**Solution**:
```tsx
// Wrong
import { useToast } from './components/ui/Toast';

// Correct
import { useToast } from './hooks/useToast';
```

### Issue: Styles not matching theme

**Cause**: Old toast using different CSS variables

**Solution**: New toast uses theme variables automatically

## Rollback Plan

If you need to rollback:

1. Remove `<ToastContainer />` from App.tsx
2. Revert imports to old system
3. Restore local ToastContainers in components

The old system remains in `src/components/ui/Toast` until migration is complete.

## Support

- See full documentation: `docs/TOAST_SYSTEM.md`
- Check examples: `src/examples/ToastIntegrationExamples.tsx`
- Use demo component: `src/components/Toast/ToastDemo.tsx`

## Timeline Recommendation

- **Week 1**: Add new system, start using for new features
- **Week 2-3**: Migrate existing components one by one
- **Week 4**: Test thoroughly, remove old system

## Questions?

Common questions:

**Q: Can I use both systems during migration?**
A: Yes, they can run side-by-side.

**Q: Do I need to update all components at once?**
A: No, migrate gradually.

**Q: Will this break existing functionality?**
A: No, if you keep both systems during migration.

**Q: Should I use the new system for everything?**
A: Yes, the new system is more feature-rich and better maintained.
