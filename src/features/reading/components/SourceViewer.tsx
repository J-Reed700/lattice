import type { ReactNode } from 'react';

import { DocxViewer } from '@/features/reading/components/ContentViewer/renderers/DocxViewer';
import { HTMLViewer } from '@/features/reading/components/ContentViewer/renderers/HTMLViewer';
import { AudioViewer, audioViewerPropsFromSource } from '@/features/reading/components/viewers/AudioViewer';
import { ImageViewer } from '@/features/reading/components/viewers/ImageViewer';
import { MarkdownViewer } from '@/features/reading/components/viewers/MarkdownViewer';
import { PDFViewer } from '@/features/reading/components/viewers/PDFViewer';
import { TextViewer } from '@/features/reading/components/viewers/TextViewer';
import type {
  PassageLocator,
  PassageMatchTier,
  SourceWithMetadata,
} from '@/types/conversation';
import { formatFileSize, getLanguageFromFileName } from '@/utils/files';

interface SourceViewerProps {
  source: SourceWithMetadata;
  mimeType: string;
  isFileTooLarge: boolean;
  isFileLarge: boolean;
  maxPreviewSize: number;
  /** The file's text, or the path it is read from, is still on its way. */
  isLoading: boolean;
  content: string;
  error: string | null;
  /** What the citation saved of the passage, shown when the file cannot be read. */
  savedCitationText: string;
  previewFilePath: string;
  initialLocator: PassageLocator | null;
  onLocationResolved: (_label: string) => void;
  onMatch: (_tier: PassageMatchTier) => void;
  isWebArchiveArticle: boolean;
  webArchiveHtmlPath: string | null;
  /** The web citation's view, when the source shows as a web article. */
  webArticle: ReactNode;
  onOpenPrimary: () => void;
}

/** The viewer for the source: picked by size, load state, and kind of file. */
export const SourceViewer = ({
  source,
  mimeType,
  isFileTooLarge,
  isFileLarge,
  maxPreviewSize,
  isLoading,
  content,
  error,
  savedCitationText,
  previewFilePath,
  initialLocator,
  onLocationResolved,
  onMatch,
  isWebArchiveArticle,
  webArchiveHtmlPath,
  webArticle,
  onOpenPrimary,
}: SourceViewerProps) => {
  // File too large - show error message
  if (isFileTooLarge) {
    return (
      <div className="flex flex-col items-center justify-center h-64 text-center px-6">
        <div className="text-[hsl(var(--warning-fg))] mb-4">
          <svg className="w-16 h-16 mx-auto" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
          </svg>
        </div>
        <p className="text-lg font-semibold text-[hsl(var(--text-primary))] mb-2">
          File too large to preview
        </p>
        <p className="text-sm text-[hsl(var(--text-secondary))] mb-4">
          This file is {formatFileSize(source.fileSizeBytes)}. Preview limit is {formatFileSize(maxPreviewSize)}.
        </p>
        <button
          onClick={onOpenPrimary}
          className="rounded-md bg-[hsl(var(--accent))] px-4 py-2 text-sm font-medium text-[hsl(var(--accent-fg))] transition-colors duration-fast hover:bg-[hsl(var(--accent-hover))]"
        >
          Open externally
        </button>
      </div>
    );
  }

  if (isLoading) {
    return (
      <div className="flex flex-col items-center justify-center h-64">
        <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-[hsl(var(--accent))] mb-4" />
        {isFileLarge && (
          <p className="text-sm text-[hsl(var(--text-tertiary))]">
            Loading large file ({formatFileSize(source.fileSizeBytes)})...
          </p>
        )}
      </div>
    );
  }

  if (webArticle) {
    return webArticle;
  }

  if (error) {
    if (savedCitationText) {
      return (
        <div className="flex min-h-full flex-col">
          <p role="status" className="mb-4 rounded-md border border-subtle bg-surface-raised px-4 py-3 text-sm text-[hsl(var(--text-secondary))]">
            The original file is unavailable. Showing saved citation text; this is not a full file snapshot.
          </p>
          <TextViewer content={savedCitationText} language="text" />
        </div>
      );
    }
    return (
      <div className="flex flex-col items-center justify-center h-64 text-center">
        <p className="text-[hsl(var(--danger-fg))] mb-2">Couldn't load file</p>
        <p className="text-sm text-[hsl(var(--text-muted))]">{error}</p>
      </div>
    );
  }

  // Web archive articles should render from captured HTML snapshot, not raw markdown text.
  if (isWebArchiveArticle && webArchiveHtmlPath) {
    return <HTMLViewer htmlPath={webArchiveHtmlPath} />;
  }

  // Route to appropriate viewer based on MIME type
  if (mimeType === 'application/pdf') {
    return (
      <PDFViewer
        filePath={previewFilePath}
        highlight={initialLocator}
        onLocationResolved={onLocationResolved}
        onMatch={onMatch}
      />
    );
  }

  if (
    mimeType === 'application/vnd.openxmlformats-officedocument.wordprocessingml.document' ||
    previewFilePath.toLowerCase().endsWith('.docx')
  ) {
    return <DocxViewer filePath={previewFilePath} title={source.fileName} />;
  }

  if (mimeType.startsWith('text/x-') || mimeType === 'application/javascript') {
    const language = getLanguageFromFileName(source.fileName);
    return <TextViewer content={content} language={language} />;
  }

  if (mimeType === 'text/markdown' || source.fileName.endsWith('.md')) {
    return <MarkdownViewer content={content} />;
  }

  if (mimeType.startsWith('audio/')) {
    return <AudioViewer filePath={previewFilePath} {...audioViewerPropsFromSource(source)} />;
  }

  if (mimeType.startsWith('image/')) {
    return <ImageViewer filePath={previewFilePath} />;
  }

  // Fallback: plain text
  return <TextViewer content={content} language="text" />;
};
