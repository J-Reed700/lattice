import { useEffect, useMemo, useState } from 'react';

import { ChevronLeft, ChevronRight, ZoomIn, ZoomOut, RotateCw } from 'lucide-react';
import { Document, Page, pdfjs } from 'react-pdf';
import 'react-pdf/dist/Page/AnnotationLayer.css';
import 'react-pdf/dist/Page/TextLayer.css';

import VaultAPI from '@/lib/api';
import { isRemoteFileSource, resolveLocalPathFromViewerSource } from '@/utils/fileSources';

/**
 * PdfViewer
 *
 * Purpose: Render PDF documents with navigation and zoom controls
 *
 * Features:
 * - Page navigation
 * - Zoom in/out
 * - Rotate document
 * - Text selection support
 * - Page counter
 *
 * States: loading, rendering, error
 * Accessibility: WCAG AA, keyboard navigation
 */

// Configure PDF.js worker
pdfjs.GlobalWorkerOptions.workerSrc = `https://unpkg.com/pdfjs-dist@${pdfjs.version}/build/pdf.worker.min.js`;

export interface PdfViewerProps {
  filePath: string;
}

export function PdfViewer({ filePath }: PdfViewerProps) {
  const [numPages, setNumPages] = useState<number>(0);
  const [pageNumber, setPageNumber] = useState<number>(1);
  const [scale, setScale] = useState<number>(1.2);
  const [rotation, setRotation] = useState<number>(0);
  const [error, setError] = useState<string | null>(null);
  const [pdfBytes, setPdfBytes] = useState<Uint8Array | null>(null);
  const isRemoteSource = useMemo(() => isRemoteFileSource(filePath), [filePath]);

  useEffect(() => {
    let isMounted = true;

    setError(null);
    setNumPages(0);
    setPageNumber(1);
    setPdfBytes(null);

    if (isRemoteSource) {
      return () => {
        isMounted = false;
      };
    }

    const loadPdfBytes = async () => {
      const resolvedPath = resolveLocalPathFromViewerSource(filePath);
      const fileResult = await VaultAPI.readFileBytes(resolvedPath);

      if (!fileResult.ok) {
        throw new Error(fileResult.error || 'Failed to read PDF bytes');
      }

      if (isMounted) {
        setPdfBytes(fileResult.data);
      }
    };

    loadPdfBytes().catch((loadError: unknown) => {
      if (isMounted) {
        setError(loadError instanceof Error ? loadError.message : 'Failed to load PDF document');
      }
    });

    return () => {
      isMounted = false;
    };
  }, [filePath, isRemoteSource]);

  const onDocumentLoadSuccess = ({ numPages }: { numPages: number }) => {
    setNumPages(numPages);
    setError(null);
  };

  const onDocumentLoadError = (error: Error) => {
    console.error('PDF load error:', error);
    setError('Failed to load PDF document');
  };

  const goToPreviousPage = () => {
    setPageNumber((prev) => Math.max(1, prev - 1));
  };

  const goToNextPage = () => {
    setPageNumber((prev) => Math.min(numPages, prev + 1));
  };

  const zoomIn = () => {
    setScale((prev) => Math.min(3, prev + 0.2));
  };

  const zoomOut = () => {
    setScale((prev) => Math.max(0.5, prev - 0.2));
  };

  const rotate = () => {
    setRotation((prev) => (prev + 90) % 360);
  };

  const fileSource = useMemo(
    () => (isRemoteSource ? filePath : (pdfBytes ? { data: pdfBytes } : null)),
    [filePath, isRemoteSource, pdfBytes]
  );

  if (error) {
    return (
      <div className="flex items-center justify-center h-full">
        <div className="text-center max-w-md p-8">
          <div className="w-16 h-16 bg-[var(--error-light)]/30 rounded-full flex items-center justify-center mx-auto mb-4">
            <svg
              className="w-8 h-8 text-[var(--error)]"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
              />
            </svg>
          </div>
          <h3 className="text-lg font-semibold text-[var(--text-primary)] mb-2">
            PDF Load Error
          </h3>
          <p className="text-[var(--text-secondary)]">{error}</p>
        </div>
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col bg-[var(--bg-primary)]">
      {/* Controls */}
      <div className="bg-[var(--surface-elevated)] border-b border-[var(--border-color)] px-4 py-3 flex items-center justify-between shadow-sm">
        {/* Page Navigation */}
        <div className="flex items-center gap-2">
          <button
            onClick={goToPreviousPage}
            disabled={pageNumber <= 1}
            className="p-2 rounded-lg hover:bg-[var(--surface-hover)] disabled:opacity-40 disabled:cursor-not-allowed transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent-primary)]"
            aria-label="Previous page"
          >
            <ChevronLeft className="w-5 h-5" />
          </button>

          <div className="flex items-center gap-2 min-w-[100px] justify-center">
            <span className="text-sm font-medium text-[var(--text-primary)]">
              Page {pageNumber}
            </span>
            {numPages > 0 && (
              <span className="text-sm text-[var(--text-secondary)]">of {numPages}</span>
            )}
          </div>

          <button
            onClick={goToNextPage}
            disabled={pageNumber >= numPages}
            className="p-2 rounded-lg hover:bg-[var(--surface-hover)] disabled:opacity-40 disabled:cursor-not-allowed transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent-primary)]"
            aria-label="Next page"
          >
            <ChevronRight className="w-5 h-5" />
          </button>
        </div>

        {/* Zoom and Rotate Controls */}
        <div className="flex items-center gap-2">
          <button
            onClick={zoomOut}
            className="p-2 rounded-lg hover:bg-[var(--surface-hover)] transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent-primary)]"
            aria-label="Zoom out"
          >
            <ZoomOut className="w-5 h-5" />
          </button>

          <span className="text-sm font-medium text-[var(--text-primary)] min-w-[60px] text-center">
            {Math.round(scale * 100)}%
          </span>

          <button
            onClick={zoomIn}
            className="p-2 rounded-lg hover:bg-[var(--surface-hover)] transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent-primary)]"
            aria-label="Zoom in"
          >
            <ZoomIn className="w-5 h-5" />
          </button>

          <div className="w-px h-6 bg-[var(--border-color)] mx-2" aria-hidden="true" />

          <button
            onClick={rotate}
            className="p-2 rounded-lg hover:bg-[var(--surface-hover)] transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent-primary)]"
            aria-label="Rotate 90 degrees"
          >
            <RotateCw className="w-5 h-5" />
          </button>
        </div>
      </div>

      {/* PDF Document */}
      <div className="flex-1 overflow-auto flex justify-center items-start p-8">
        <div className="shadow-2xl">
          {fileSource ? (
            <Document
              file={fileSource}
              onLoadSuccess={onDocumentLoadSuccess}
              onLoadError={onDocumentLoadError}
              loading={
                <div className="flex items-center justify-center p-12">
                  <div className="flex flex-col items-center gap-4">
                    <div className="animate-spin rounded-full h-12 w-12 border-4 border-[var(--accent-primary)] border-t-transparent" />
                    <p className="text-[var(--text-secondary)] font-medium">Loading PDF...</p>
                  </div>
                </div>
              }
            >
              <Page
                pageNumber={pageNumber}
                scale={scale}
                rotate={rotation}
                renderTextLayer
                renderAnnotationLayer
                loading={
                  <div className="flex items-center justify-center p-12 bg-[var(--surface-elevated)]">
                    <div className="animate-spin rounded-full h-8 w-8 border-4 border-[var(--accent-primary)] border-t-transparent" />
                  </div>
                }
              />
            </Document>
          ) : (
            <div className="flex items-center justify-center p-12">
              <div className="flex flex-col items-center gap-4">
                <div className="animate-spin rounded-full h-12 w-12 border-4 border-[var(--accent-primary)] border-t-transparent" />
                <p className="text-[var(--text-secondary)] font-medium">Loading PDF...</p>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

PdfViewer.displayName = 'PdfViewer';

export default PdfViewer;
