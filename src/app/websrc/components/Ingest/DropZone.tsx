/**
 * DropZone
 *
 * Purpose: Modern file upload interface with drag-and-drop support
 *
 * Features:
 * - Drag-and-drop file upload with visual feedback
 * - Multiple file type support with MIME type filtering
 * - Animated transitions using Framer Motion
 * - File size validation
 * - Dynamic file type icons
 * - Click-to-browse fallback
 * - Responsive design with gradient effects
 * - Support for single or multiple file uploads
 *
 * Visual States:
 * - Default: Dashed border, subtle styling
 * - Dragging: Scaled up, gradient overlay, pulsing border
 * - Active: Animated icon, clear call-to-action
 *
 * Accessibility: WCAG AA, keyboard navigation, screen reader support
 */

import { useCallback, useState } from 'react';

import { motion } from 'framer-motion';
import { Upload, FileUp, FileText, Image, Video, Music, FileArchive, File } from 'lucide-react';
import { useDropzone, type Accept } from 'react-dropzone';

import { cn } from '@/lib/utils';

import Button from '../ui/Button/Button';

export interface DropZoneProps {
  onDrop: (_files: File[]) => void;
  accept?: Accept;
  maxSize?: number;
  multiple?: boolean;
  disabled?: boolean;
  className?: string;
}

const MIME_TYPE_CATEGORIES = {
  documents: [
    'application/pdf',
    'application/msword',
    'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
    'text/plain',
    'text/markdown',
    'application/rtf',
  ],
  images: [
    'image/jpeg',
    'image/png',
    'image/gif',
    'image/webp',
    'image/svg+xml',
    'image/bmp',
  ],
  audio: [
    'audio/mpeg',
    'audio/wav',
    'audio/ogg',
    'audio/webm',
    'audio/aac',
    'audio/flac',
  ],
  video: [
    'video/mp4',
    'video/webm',
    'video/ogg',
    'video/quicktime',
    'video/x-msvideo',
  ],
  archives: [
    'application/zip',
    'application/x-rar-compressed',
    'application/x-7z-compressed',
    'application/x-tar',
    'application/gzip',
  ],
};

const DEFAULT_ACCEPT: Accept = {
  ...Object.values(MIME_TYPE_CATEGORIES).reduce((acc, types) => {
    types.forEach(type => {
      acc[type] = [];
    });
    return acc;
  }, {} as Accept),
};

const getFileTypeIcon = (acceptedTypes?: Accept): React.ReactNode => {
  if (!acceptedTypes || Object.keys(acceptedTypes).length === 0) {
    return <Upload className="w-10 h-10 mb-4 text-[hsl(var(--accent))]" />;
  }

  const types = Object.keys(acceptedTypes);

  if (types.some(t => MIME_TYPE_CATEGORIES.documents.includes(t))) {
    return <FileText className="w-10 h-10 mb-4 text-[hsl(var(--accent))]" />;
  }
  if (types.some(t => MIME_TYPE_CATEGORIES.images.includes(t))) {
    return <Image className="w-10 h-10 mb-4 text-[hsl(var(--accent))]" />;
  }
  if (types.some(t => MIME_TYPE_CATEGORIES.audio.includes(t))) {
    return <Music className="w-10 h-10 mb-4 text-[hsl(var(--accent))]" />;
  }
  if (types.some(t => MIME_TYPE_CATEGORIES.video.includes(t))) {
    return <Video className="w-10 h-10 mb-4 text-[hsl(var(--accent))]" />;
  }
  if (types.some(t => MIME_TYPE_CATEGORIES.archives.includes(t))) {
    return <FileArchive className="w-10 h-10 mb-4 text-[hsl(var(--accent))]" />;
  }

  return <File className="w-10 h-10 mb-4 text-[hsl(var(--accent))]" />;
};

const getSupportedFormats = (acceptedTypes?: Accept): string => {
  if (!acceptedTypes || Object.keys(acceptedTypes).length === 0) {
    return 'Supports documents, images, audio, video, archives, and more';
  }

  const types = Object.keys(acceptedTypes);
  const categories: string[] = [];

  if (types.some(t => MIME_TYPE_CATEGORIES.documents.includes(t))) {
    categories.push('documents');
  }
  if (types.some(t => MIME_TYPE_CATEGORIES.images.includes(t))) {
    categories.push('images');
  }
  if (types.some(t => MIME_TYPE_CATEGORIES.audio.includes(t))) {
    categories.push('audio');
  }
  if (types.some(t => MIME_TYPE_CATEGORIES.video.includes(t))) {
    categories.push('video');
  }
  if (types.some(t => MIME_TYPE_CATEGORIES.archives.includes(t))) {
    categories.push('archives');
  }

  if (categories.length === 0) {
    return 'Supports selected file types';
  }

  if (categories.length === 1) {
    return `Supports ${categories[0]}`;
  }

  return `Supports ${categories.slice(0, -1).join(', ')} and ${categories.slice(-1)}`;
};

export const DropZone: React.FC<DropZoneProps> = ({
  onDrop,
  accept = DEFAULT_ACCEPT,
  maxSize = 100 * 1024 * 1024,
  multiple = true,
  disabled = false,
  className,
}) => {
  const [dragDepth, setDragDepth] = useState(0);

  const handleDrop = useCallback(
    (acceptedFiles: File[]) => {
      setDragDepth(0);
      onDrop(acceptedFiles);
    },
    [onDrop]
  );

  const {
    getRootProps,
    getInputProps,
    isDragActive,
    isDragReject,
    open,
  } = useDropzone({
    onDrop: handleDrop,
    accept,
    maxSize,
    multiple,
    disabled,
    noClick: false,
    noKeyboard: false,
  });

  const isDragging = isDragActive || dragDepth > 0;

  const rootProps = getRootProps();
   
  const {
    onDragEnter,
    onDragLeave,
    onDrag,
    onDragEnd,
    onDragStart,
    onAnimationStart,
    ...restRootProps
  } = rootProps;

  // Avoid unused variable warnings for destructured event handlers
  void onDragEnter;
  void onDragLeave;
  void onDrag;
  void onDragEnd;
  void onDragStart;
  void onAnimationStart;

  return (
    <motion.div
      {...restRootProps}
      className={cn(
        'relative border-2 border-dashed rounded-lg transition-colors duration-base',
        'min-h-[300px] flex flex-col items-center justify-center p-8',
        'cursor-pointer focus:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--ring))] focus-visible:ring-offset-2',
        isDragging
          ? 'border-[hsl(var(--accent))] bg-[hsl(var(--accent))]/5 shadow-lg shadow-[hsl(var(--accent))]/20'
          : 'border-[hsl(var(--border-subtle))] hover:border-[hsl(var(--accent))]/50',
        isDragReject && 'border-[hsl(var(--danger-fg))] bg-[hsl(var(--danger-fg))]/5',
        disabled && 'opacity-50 cursor-not-allowed pointer-events-none',
        className
      )}
      animate={{
        scale: isDragging ? 1.02 : 1,
        borderWidth: isDragging ? 3 : 2,
      }}
      transition={{
        type: 'spring',
        stiffness: 300,
        damping: 30,
      }}
      onDragEnter={() => setDragDepth(d => d + 1)}
      onDragLeave={() => setDragDepth(d => Math.max(0, d - 1))}
    >
      <input {...getInputProps()} aria-label="File upload input" />

      <motion.div
        animate={{
          y: isDragging ? -10 : 0,
          scale: isDragging ? 1.1 : 1,
        }}
        transition={{
          type: 'spring',
          stiffness: 300,
          damping: 20,
        }}
      >
        {getFileTypeIcon(accept)}
      </motion.div>

      <motion.h3
        className="text-xl font-semibold mb-2 text-[hsl(var(--text-primary))]"
        animate={{ scale: isDragging ? 1.05 : 1 }}
      >
        {isDragReject
          ? 'File type not supported'
          : isDragging
            ? 'Drop to import'
            : 'Drag & drop files here'
        }
      </motion.h3>

      <p className="text-sm text-[hsl(var(--text-secondary))] mb-6 max-w-md text-center">
        {getSupportedFormats(accept)} • Automatic format detection •
        {multiple ? ' Batch processing' : ' Single file upload'}
        {maxSize && ` • Max ${formatFileSize(maxSize)}`}
      </p>

      <Button
        onClick={(e) => {
          e.stopPropagation();
          open();
        }}
        variant="secondary"
        size="md"
        leftIcon={<FileUp className="w-4 h-4" />}
        disabled={disabled}
      >
        Or browse files
      </Button>

      {isDragging && !isDragReject && (
        <motion.div
          className="absolute inset-0 bg-gradient-to-br from-[hsl(var(--accent))]/10 to-transparent rounded-xl pointer-events-none"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.2 }}
        />
      )}

      {isDragging && (
        <motion.div
          className="absolute inset-0 rounded-xl border-2 border-[hsl(var(--accent))] pointer-events-none"
          animate={{
            opacity: [0.5, 1, 0.5],
            scale: [1, 1.01, 1],
          }}
          transition={{
            duration: 1.5,
            repeat: Infinity,
            ease: 'easeInOut',
          }}
        />
      )}
    </motion.div>
  );
};

const formatFileSize = (bytes: number): string => {
  if (bytes === 0) return '0 Bytes';

  const k = 1024;
  const sizes = ['Bytes', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));

  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(2))  } ${  sizes[i]}`;
};

export default DropZone;
