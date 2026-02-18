import { Suspense, lazy, memo } from 'react';

import type { FileData } from './DocumentViewer';

/**
 * ViewerContent
 *
 * Purpose: Route document content to appropriate format viewer
 *
 * Features:
 * - Multi-format support (PDF, text, code, images, markdown)
 * - Lazy loading of viewers for code splitting
 * - Fallback for unsupported formats
 * - Loading states
 *
 * States: loading, viewing, unsupported
 * Accessibility: WCAG AA, semantic HTML
 * Performance: Optimized with React.memo to prevent unnecessary re-renders
 */

// Lazy load viewers for code splitting
const PdfViewer = lazy(() => import('./viewers/PdfViewer'));
const TextViewer = lazy(() => import('./viewers/TextViewer'));
const ImageViewer = lazy(() => import('./viewers/ImageViewer'));
const MarkdownViewer = lazy(() => import('./viewers/MarkdownViewer'));
const CodeViewer = lazy(() => import('./viewers/CodeViewer'));

export interface ViewerContentProps {
  fileData: FileData;
  searchQuery?: string;
}

export const ViewerContent = memo(({ fileData }: ViewerContentProps) => {
  const renderViewer = () => {
    const mimeType = fileData.mimeType.toLowerCase();
    const extension = fileData.fileType.toLowerCase();

    // PDF
    if (mimeType === 'application/pdf' || extension === 'pdf') {
      return <PdfViewer filePath={fileData.path} />;
    }

    // Images
    if (mimeType.startsWith('image/')) {
      return <ImageViewer filePath={fileData.path} fileName={fileData.fileName} />;
    }

    // Markdown
    if (extension === 'md' || extension === 'markdown') {
      return <MarkdownViewer content={fileData.content || ''} />;
    }

    // Code files with syntax highlighting
    const codeExtensions = [
      'js', 'ts', 'tsx', 'jsx',
      'py', 'rs', 'go', 'java',
      'cpp', 'c', 'h', 'hpp',
      'css', 'scss', 'sass', 'less',
      'html', 'xml',
      'json', 'yaml', 'yml', 'toml',
      'sh', 'bash', 'zsh',
      'sql', 'graphql',
      'dockerfile', 'makefile',
    ];

    if (extension && codeExtensions.includes(extension)) {
      return (
        <CodeViewer
          content={fileData.content || ''}
          language={extension}
          fileName={fileData.fileName}
        />
      );
    }

    // Plain text
    if (
      mimeType.startsWith('text/') ||
      extension === 'txt' ||
      extension === 'log' ||
      fileData.content
    ) {
      return <TextViewer content={fileData.content || ''} />;
    }

    // Unsupported format
    return (
      <div className="flex items-center justify-center h-full bg-[var(--bg-secondary)]">
        <div className="text-center max-w-md p-8">
          <div className="w-16 h-16 bg-[var(--bg-tertiary)] rounded-full flex items-center justify-center mx-auto mb-4">
            <svg
              className="w-8 h-8 text-[var(--text-tertiary)]"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
              />
            </svg>
          </div>
          <h3 className="text-lg font-semibold text-[var(--text-primary)] mb-2">
            Preview Not Available
          </h3>
          <p className="text-[var(--text-secondary)] mb-4">
            This file type cannot be previewed in the app. You can open it in an external
            application using the button in the header.
          </p>
          <div className="text-sm text-[var(--text-tertiary)] space-y-1">
            <p>File type: {fileData.fileType.toUpperCase()}</p>
            <p>MIME type: {fileData.mimeType}</p>
          </div>
        </div>
      </div>
    );
  };

  return (
    <div className="h-full bg-[var(--bg-secondary)] overflow-auto">
      <Suspense
        fallback={
          <div className="flex items-center justify-center h-full">
            <div className="flex flex-col items-center gap-4">
              <div className="animate-spin rounded-full h-12 w-12 border-4 border-[var(--accent-primary)] border-t-transparent" />
              <p className="text-[var(--text-secondary)] font-medium">Loading viewer...</p>
            </div>
          </div>
        }
      >
        {renderViewer()}
      </Suspense>
    </div>
  );
}, (prevProps, nextProps) => 
  // Only re-render if the file path changes
   prevProps.fileData.path === nextProps.fileData.path
);

ViewerContent.displayName = 'ViewerContent';

export default ViewerContent;
