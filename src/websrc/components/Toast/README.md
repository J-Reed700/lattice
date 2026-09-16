# Toast Notification System

Professional toast notification system for Lattice/Lattice desktop application.

## Quick Start

### 1. Import ToastContainer in App.tsx

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
import { useToast } from './hooks/useToast';

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
- Auto-dismiss with progress bar
- Manual dismiss
- Action buttons
- Pause on hover
- Queue management
- 6 position options
- Full accessibility (WCAG AA)
- Dark mode support
- Smooth animations

## File Structure

```
src/
├── components/Toast/
│   ├── ToastContainer.tsx    # Main container component
│   ├── ToastItem.tsx          # Individual toast card
│   ├── ToastIcons.tsx         # Type-specific icons
│   ├── ToastDemo.tsx          # Interactive demo
│   ├── index.ts               # Barrel export
│   └── __tests__/
│       └── ToastSystem.test.tsx
├── stores/
│   └── toastStore.ts          # Global state management
├── hooks/
│   └── useToast.ts            # React hook for toasts
├── utils/
│   └── toast.ts               # Utility functions
└── examples/
    └── ToastIntegrationExamples.tsx
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

### Utility Functions

```tsx
import { showSuccessToast, showErrorToast } from './utils/toast';

showSuccessToast('Saved!');
showErrorToast('Failed', error);
showPromiseToast(promise, { loading, success, error });
```

### Configuration

```tsx
import { toastStore } from './stores/toastStore';

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
npm test -- Toast
```

Use demo component for visual testing:

```tsx
import { ToastDemo } from './components/Toast/ToastDemo';

// Add to your app during development
<ToastDemo />
```

## Accessibility

- ARIA live regions for screen readers
- Keyboard navigation (Tab, Esc)
- Focus management
- Color contrast WCAG AA compliant
- Reduced motion support

## Browser Support

- Chrome/Edge 90+
- Firefox 88+
- Safari 14+

## License

MIT
