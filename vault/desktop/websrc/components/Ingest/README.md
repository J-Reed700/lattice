# DropZone Component

Modern file upload interface with drag-and-drop support for the Recall data ingestion system.

## Overview

The `DropZone` component provides a beautiful, accessible, and feature-rich file upload interface with:

- 🎨 **Modern Design**: Framer Motion animations, gradient overlays, and pulsing borders
- 🎯 **Smart File Filtering**: MIME type-based filtering with dynamic icon display
- 📦 **Batch Processing**: Support for multiple file uploads with proper validation
- ♿ **Accessibility**: WCAG AA compliant with keyboard navigation and screen reader support
- 🎭 **Visual Feedback**: Real-time drag state with scale animations and color transitions
- 🔒 **File Validation**: Size limits and type restrictions with user-friendly error messages

## Installation

The component uses `react-dropzone` which is already installed in the project:

```bash
npm install react-dropzone
```

## Basic Usage

```tsx
import { DropZone } from '@/components/Ingest';

function MyComponent() {
  const handleDrop = (files: File[]) => {
    console.log('Dropped files:', files);
    // Process files here
  };

  return <DropZone onDrop={handleDrop} />;
}
```

## Props

| Prop | Type | Default | Description |
|------|------|---------|-------------|
| `onDrop` | `(files: File[]) => void` | **Required** | Callback function called when files are dropped |
| `accept` | `Accept` | All file types | MIME types to accept (react-dropzone Accept type) |
| `maxSize` | `number` | `100 * 1024 * 1024` (100MB) | Maximum file size in bytes |
| `multiple` | `boolean` | `true` | Whether to accept multiple files |
| `disabled` | `boolean` | `false` | Disable the drop zone |
| `className` | `string` | `undefined` | Additional CSS classes |

## Accept Type Definition

```typescript
import { Accept } from 'react-dropzone';

const accept: Accept = {
  'application/pdf': ['.pdf'],
  'image/png': ['.png'],
  'image/jpeg': ['.jpg', '.jpeg'],
  // etc...
};
```

## Features

### 1. Dynamic File Type Icons

The component automatically displays relevant icons based on accepted file types:

- 📄 **Documents**: PDF, Word, text files
- 🖼️ **Images**: JPEG, PNG, GIF, WebP, SVG
- 🎵 **Audio**: MP3, WAV, OGG, AAC, FLAC
- 🎬 **Video**: MP4, WebM, MOV, AVI
- 📦 **Archives**: ZIP, RAR, 7Z, TAR, GZIP

### 2. Visual States

**Default State**:
- Dashed border with hover effect
- Subtle background
- Clear call-to-action text

**Dragging State**:
- Scales up to 1.02x
- Border changes to accent color (3px)
- Gradient overlay appears
- Icon animates upward
- Pulsing border animation

**Rejected State**:
- Red error border
- Error message display

**Disabled State**:
- 50% opacity
- No pointer events

### 3. Animation Details

- **Scale Animation**: Spring-based with 300 stiffness, 30 damping
- **Icon Movement**: -10px Y translation on drag
- **Gradient Overlay**: Fades in/out with 200ms duration
- **Border Pulse**: Infinite loop with 1.5s duration

## Usage Examples

### Example 1: All File Types (Default)

```tsx
import { DropZone } from '@/components/Ingest';

function AllFilesUpload() {
  return (
    <DropZone
      onDrop={(files) => {
        console.log('Files:', files);
      }}
    />
  );
}
```

### Example 2: Document Upload Only

```tsx
import { DropZone } from '@/components/Ingest';
import { Accept } from 'react-dropzone';

function DocumentUpload() {
  const documentTypes: Accept = {
    'application/pdf': ['.pdf'],
    'application/msword': ['.doc'],
    'application/vnd.openxmlformats-officedocument.wordprocessingml.document': ['.docx'],
    'text/plain': ['.txt'],
    'text/markdown': ['.md'],
  };

  return (
    <DropZone
      onDrop={(files) => console.log('Documents:', files)}
      accept={documentTypes}
      maxSize={50 * 1024 * 1024} // 50MB
    />
  );
}
```

### Example 3: Single Image Upload

```tsx
import { DropZone } from '@/components/Ingest';

function SingleImageUpload() {
  const imageTypes = {
    'image/jpeg': ['.jpg', '.jpeg'],
    'image/png': ['.png'],
    'image/webp': ['.webp'],
  };

  return (
    <DropZone
      onDrop={(files) => {
        if (files.length > 0) {
          const file = files[0];
          console.log('Selected image:', file.name);
        }
      }}
      accept={imageTypes}
      multiple={false}
      maxSize={10 * 1024 * 1024} // 10MB
    />
  );
}
```

### Example 4: With Upload Progress

```tsx
import { useState } from 'react';
import { DropZone } from '@/components/Ingest';

function UploadWithProgress() {
  const [isUploading, setIsUploading] = useState(false);

  const handleDrop = async (files: File[]) => {
    setIsUploading(true);

    try {
      // Upload files
      await uploadFiles(files);
      alert('Upload successful!');
    } catch (error) {
      console.error('Upload failed:', error);
    } finally {
      setIsUploading(false);
    }
  };

  return (
    <div>
      <DropZone
        onDrop={handleDrop}
        disabled={isUploading}
      />
      {isUploading && <p>Uploading...</p>}
    </div>
  );
}
```

### Example 5: Custom Styling

```tsx
import { DropZone } from '@/components/Ingest';

function CustomStyledDropZone() {
  return (
    <DropZone
      onDrop={(files) => console.log(files)}
      className="min-h-[400px] bg-gradient-to-br from-purple-50 to-blue-50"
    />
  );
}
```

## Accessibility

The component follows WCAG AA standards:

- ✅ Keyboard navigation support (tab, enter, space)
- ✅ Screen reader support with `aria-label`
- ✅ Focus visible states with ring outline
- ✅ High contrast mode compatible
- ✅ Clear visual feedback for all states

## Styling

The component uses CSS custom properties for theming:

```css
--accent-primary      /* Primary accent color */
--border-color        /* Default border color */
--text-primary        /* Primary text color */
--text-secondary      /* Secondary text color */
--error               /* Error state color */
```

## File Size Formatting

The component includes a utility function to format file sizes:

```typescript
formatFileSize(1024) // "1 KB"
formatFileSize(1048576) // "1 MB"
formatFileSize(1073741824) // "1 GB"
```

## MIME Type Categories

Pre-defined categories for common file types:

- **Documents**: PDF, Word, text files, RTF
- **Images**: JPEG, PNG, GIF, WebP, SVG, BMP
- **Audio**: MP3, WAV, OGG, WebM, AAC, FLAC
- **Video**: MP4, WebM, OGG, QuickTime, AVI
- **Archives**: ZIP, RAR, 7Z, TAR, GZIP

## TypeScript Support

Fully typed with TypeScript:

```typescript
export interface DropZoneProps {
  onDrop: (files: File[]) => void;
  accept?: Accept;
  maxSize?: number;
  multiple?: boolean;
  disabled?: boolean;
  className?: string;
}
```

## Browser Support

- ✅ Chrome/Edge (latest)
- ✅ Firefox (latest)
- ✅ Safari (latest)
- ✅ Mobile browsers (iOS Safari, Chrome Mobile)

## Performance

- Uses `useCallback` for memoized event handlers
- Minimal re-renders with proper state management
- Optimized animations with Framer Motion
- Lazy icon rendering based on file types

## Related Components

- `UrlImport` - Import content from URLs
- `BatchProcessor` - Process multiple files with preview
- `ContentPreview` - Preview extracted content before ingestion

## Contributing

When modifying this component:

1. Follow the existing code style from `CLAUDE.md`
2. Update TypeScript types for any new props
3. Test with multiple file types and sizes
4. Verify accessibility with keyboard navigation
5. Check animations on different devices
6. Update this README with new features

## References

- [INGESTION_SYSTEM_SPECIFICATION.md](../../../INGESTION_SYSTEM_SPECIFICATION.md) - Full system specification
- [react-dropzone Documentation](https://react-dropzone.js.org/)
- [Framer Motion API](https://www.framer.com/motion/)
- [WCAG 2.1 Guidelines](https://www.w3.org/WAI/WCAG21/quickref/)

## License

Part of the Recall project. See project LICENSE for details.
