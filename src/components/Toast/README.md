# Toast Notification System

Toast notification system for the Lattice desktop application.

This is the general-purpose toast. Errors raised through `ErrorContext` render
in the separate `ErrorToastContainer` (`src/components/ErrorToast/`).

## Quick Start

### 1. ToastContainer is mounted once in App.tsx

It is already there; don't mount a second one.

```tsx
import { ToastContainer } from './components/Toast';

function App() {
  return (
    <>
      {/* Your app content */}
      <ToastContainer />
    </>
  );
}
```

### 2. Use in Components

```tsx
import { useToast } from '@/hooks/useToast';

function MyComponent() {
  const { toast } = useToast();

  return (
    <button onClick={() => toast.success('Saved!')}>
      Save
    </button>
  );
}
```

## Features

- 4 toast types (success, error, warning, info)
- Auto-dismiss with progress bar (`duration: 0` keeps a toast until dismissed)
- Manual dismiss, and Esc dismisses all
- One action button per toast
- Pause on hover
- At most `maxToasts` on screen; the oldest are dropped
- 6 position options
- Slide-in animation, off under reduced motion

## File Structure

```
src/
├── components/Toast/
│   ├── ToastContainer.tsx    # Main container component
│   ├── ToastItem.tsx          # Individual toast card
│   ├── ToastIcons.tsx         # Type-specific icons
│   ├── index.ts               # Barrel export
│   └── __tests__/
│       └── ToastSystem.test.tsx
├── stores/
│   ├── toastStore.ts          # Zustand store + `toast` / `toastStore` helpers
│   └── toastStore.test.ts
├── hooks/
│   └── useToast.ts            # React hook for toasts
└── utils/
    └── toast.ts               # Utility functions
```

## API

### useToast Hook

```tsx
const { toast } = useToast();

toast.success(title, options?)
toast.error(title, options?)
toast.warning(title, options?)
toast.info(title, options?)
toast.dismiss(id)
toast.dismissAll()
```

Each creator returns the toast's id. `options` takes `message`, `duration`,
`action: { label, onClick }`, `dismissible` and `icon`. Outside React, import
`toast` from `@/stores/toastStore` directly.

### Utility Functions

```tsx
import { showSuccessToast, showErrorToast, showPromiseToast } from '@/utils/toast';

showSuccessToast('Saved!');
showErrorToast('Failed', error);   // error's message becomes the toast message
showPromiseToast(promise, { loading, success, error });
```

Also `showWarningToast`, `showInfoToast`, `dismissToast(id)` and
`dismissAllToasts()`.

### Configuration

```tsx
import { toastStore } from '@/stores/toastStore';

toastStore.updateConfig({
  position: 'top-right',
  maxToasts: 5,
  defaultDuration: 4000,
  pauseOnHover: true,
});
```

## Examples

### Basic Success

```tsx
toast.success('File uploaded successfully');
```

### Error with Details

```tsx
toast.error('Upload failed', {
  message: error.message,
  duration: 5000,
});
```

### Delete with Undo

```tsx
toast.success('File deleted', {
  action: {
    label: 'Undo',
    onClick: () => restoreFile(),
  },
});
```

### Async Operation

```tsx
await showPromiseToast(uploadFile(), {
  loading: 'Uploading...',
  success: 'Upload complete',
  error: 'Upload failed',
});
```

## Testing

Run tests:

```bash
npx vitest run src/components/Toast src/stores/toastStore.test.ts
```

## Accessibility

- The container is a labelled `region` with `aria-live="polite"`; each toast is a `role="status"`
- The progress bar is a `role="progressbar"`
- Esc dismisses all toasts
- Reduced motion turns off the slide-in

## License

MIT
