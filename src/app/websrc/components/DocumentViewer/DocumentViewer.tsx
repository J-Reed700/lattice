import { useState, useEffect, useCallback, memo } from 'react';

import { X, ChevronLeft, ChevronRight } from 'lucide-react';

import VaultAPI from '../../lib/api';
import { DocumentViewerSkeleton } from '../Skeleton';
import ViewerContent from './ViewerContent';
import ViewerHeader from './ViewerHeader';
import ViewerSidebar from './ViewerSidebar';

import type { SearchResult } from '../../types';

/**
 * DocumentViewer
 *
 * Purpose: Display documents from search results with multi-format support
 *
 * Features:
 * - Multi-format rendering (PDF, text, code, images, markdown)
 * - Navigate between search results
 * - View file metadata
 * - Open in external application
 * - Responsive layout with optional sidebar
 * - Keyboard shortcuts (Esc to close, arrow keys to navigate)
 *
 * States: loading, error, viewing
 * Accessibility: WCAG AA, keyboard navigation, screen reader support
 * Performance: Optimized with React.memo to prevent unnecessary re-renders
 */

export interface DocumentViewerProps {
  /** The search result to display */
  result: SearchResult;
  /** All search results for navigation */
  searchResults?: SearchResult[];
  /** Callback when viewer is closed */
  onClose: () => void;
  /** Callback when navigating to a different document */
  onNavigate?: (result: SearchResult) => void;
}

export interface FileData {
  path: string;
  fileName: string;
  fileType: string;
  content?: string;
  mimeType: string;
  sizeBytes: number;
  modifiedAt: string;
}

export const DocumentViewer = memo(({
  result,
  searchResults,
  onClose,
  onNavigate,
}: DocumentViewerProps) => {
  const [fileData, setFileData] = useState<FileData | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [currentIndex, setCurrentIndex] = useState(0);
  const [showSidebar, setShowSidebar] = useState(true);

  // Find current index in search results
  useEffect(() => {
    if (searchResults) {
      const index = searchResults.findIndex((r) => r.id === result.id);
      if (index >= 0) setCurrentIndex(index);
    }
  }, [result.id, searchResults]);

  // Load file data
  useEffect(() => {
    loadFile();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [result.id]);

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Escape: Close viewer
      if (e.key === 'Escape') {
        e.preventDefault();
        onClose();
      }

      // Arrow Left: Previous document
      if (e.key === 'ArrowLeft' && searchResults && currentIndex > 0) {
        e.preventDefault();
        handlePrevious();
      }

      // Arrow Right: Next document
      if (e.key === 'ArrowRight' && searchResults && currentIndex < searchResults.length - 1) {
        e.preventDefault();
        handleNext();
      }

      // I: Toggle sidebar
      if (e.key === 'i' || e.key === 'I') {
        e.preventDefault();
        setShowSidebar((prev) => !prev);
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [currentIndex, searchResults, onClose]);

  const loadFile = async () => {
    setLoading(true);
    setError(null);

    try {
      const filePath = result.metadata.path as string;
      const fileName = (result.metadata.filename as string) || 'Unknown';
      const fileType = ((result.metadata.file_type as string) || '').toLowerCase();

      // Determine if we need to read file content
      const textExtensions = ['txt', 'md', 'markdown', 'js', 'ts', 'tsx', 'jsx', 'py', 'rs', 'go', 'java', 'cpp', 'c', 'h', 'css', 'html', 'json', 'yaml', 'yml', 'toml', 'xml', 'sh', 'bash'];
      const needsContent = textExtensions.includes(fileType);

      let content: string | undefined;
      if (needsContent) {
        const contentResult = await VaultAPI.readFileContent(filePath);
        if (contentResult.ok) {
          content = contentResult.data;
        } else {
          console.warn('Failed to read file content:', contentResult.error);
          // Continue without content
        }
      }

      // Get file metadata
      const metadataResult = await VaultAPI.getFileMetadata(filePath);
      if (!metadataResult.ok) {
        setError(metadataResult.error);
        setLoading(false);
        return;
      }
      const metadata = metadataResult.data;

      // Determine MIME type
      const mimeType = getMimeType(fileType);

      setFileData({
        path: filePath,
        fileName,
        fileType,
        content,
        mimeType,
        sizeBytes: metadata.sizeBytes,
        modifiedAt: metadata.modifiedAt,
      });
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load file');
      console.error('Failed to load file:', err);
    } finally {
      setLoading(false);
    }
  };

  const handlePrevious = useCallback(() => {
    if (searchResults && currentIndex > 0) {
      const prev = searchResults[currentIndex - 1];
      onNavigate?.(prev);
    }
  }, [currentIndex, searchResults, onNavigate]);

  const handleNext = useCallback(() => {
    if (searchResults && currentIndex < searchResults.length - 1) {
      const next = searchResults[currentIndex + 1];
      onNavigate?.(next);
    }
  }, [currentIndex, searchResults, onNavigate]);

  const handleOpenExternal = async () => {
    if (fileData) {
      const result = await VaultAPI.openFile(fileData.path);

      if (!result.ok) {
        console.error('Failed to open file externally:', result.error);
      }
    }
  };

  const handleDownload = async () => {
    if (fileData) {
      const result = await VaultAPI.showInFolder(fileData.path);

      if (!result.ok) {
        console.error('Failed to show in folder:', result.error);
      }
    }
  };

  // Loading state
  if (loading) {
    return <DocumentViewerSkeleton showSidebar={showSidebar} />;
  }

  // Error state
  if (error || !fileData) {
    return (
      <div className="fixed inset-0 bg-black/60 backdrop-blur-sm flex items-center justify-center z-50">
        <div className="bg-[var(--surface-elevated)] rounded-xl p-8 max-w-md shadow-2xl">
          <div className="flex items-center gap-3 text-[var(--error)] mb-4">
            <X className="w-6 h-6" />
            <h2 className="text-xl font-bold">Error Loading Document</h2>
          </div>
          <p className="text-[var(--text-secondary)] mb-6">
            {error || 'Document could not be loaded'}
          </p>
          <button
            onClick={onClose}
            className="w-full px-4 py-2 bg-[var(--accent-primary)] text-white rounded-lg hover:bg-[var(--accent-hover)] transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent-primary)]"
          >
            Close
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex flex-col">
      {/* Header */}
      <ViewerHeader
        fileName={fileData.fileName}
        onClose={onClose}
        onOpenExternal={handleOpenExternal}
        onDownload={handleDownload}
        onToggleSidebar={() => setShowSidebar(!showSidebar)}
        showSidebar={showSidebar}
      />

      {/* Main Content Area */}
      <div className="flex-1 flex overflow-hidden">
        {/* Navigation Controls */}
        {searchResults && searchResults.length > 1 && (
          <div className="absolute left-6 top-1/2 -translate-y-1/2 z-10 flex flex-col gap-3">
            <button
              onClick={handlePrevious}
              disabled={currentIndex === 0}
              className="p-3 bg-[var(--surface-elevated)] rounded-full shadow-lg hover:shadow-xl transition-all disabled:opacity-40 disabled:cursor-not-allowed hover:bg-[var(--bg-secondary)] focus:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent-primary)]"
              title="Previous document (Left Arrow)"
              aria-label="Previous document"
            >
              <ChevronLeft className="w-6 h-6 text-[var(--text-secondary)]" />
            </button>
            <div className="text-xs text-center font-medium text-white bg-black/70 px-3 py-2 rounded-full backdrop-blur-sm">
              {currentIndex + 1} / {searchResults.length}
            </div>
            <button
              onClick={handleNext}
              disabled={currentIndex === searchResults.length - 1}
              className="p-3 bg-[var(--surface-elevated)] rounded-full shadow-lg hover:shadow-xl transition-all disabled:opacity-40 disabled:cursor-not-allowed hover:bg-[var(--bg-secondary)] focus:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent-primary)]"
              title="Next document (Right Arrow)"
              aria-label="Next document"
            >
              <ChevronRight className="w-6 h-6 text-[var(--text-secondary)]" />
            </button>
          </div>
        )}

        {/* Document Content */}
        <div
          className={`flex-1 overflow-hidden transition-all duration-300 ${
            showSidebar ? 'mr-96' : ''
          }`}
        >
          <ViewerContent fileData={fileData} searchQuery={result.content} />
        </div>

        {/* Sidebar */}
        {showSidebar && <ViewerSidebar fileData={fileData} result={result} />}
      </div>
    </div>
  );
}, (prevProps, nextProps) => 
  // Only re-render if the document ID changes
  // This prevents unnecessary re-renders when parent components update
   prevProps.result.id === nextProps.result.id &&
    prevProps.searchResults?.length === nextProps.searchResults?.length
);

DocumentViewer.displayName = 'DocumentViewer';

// Utility: Determine MIME type from file extension
function getMimeType(extension: string): string {
  const mimeTypes: Record<string, string> = {
    pdf: 'application/pdf',
    txt: 'text/plain',
    md: 'text/markdown',
    markdown: 'text/markdown',
    html: 'text/html',
    css: 'text/css',
    js: 'text/javascript',
    ts: 'text/typescript',
    tsx: 'text/typescript',
    jsx: 'text/javascript',
    json: 'application/json',
    xml: 'text/xml',
    py: 'text/x-python',
    rs: 'text/x-rust',
    go: 'text/x-go',
    java: 'text/x-java',
    c: 'text/x-c',
    cpp: 'text/x-c++',
    h: 'text/x-c',
    png: 'image/png',
    jpg: 'image/jpeg',
    jpeg: 'image/jpeg',
    gif: 'image/gif',
    svg: 'image/svg+xml',
    webp: 'image/webp',
  };

  return mimeTypes[extension.toLowerCase()] || 'application/octet-stream';
}
