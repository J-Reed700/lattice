import { useEffect, useMemo, useState } from 'react';

import { ChevronLeft, ChevronRight, ZoomIn, ZoomOut , Loader2, AlertCircle } from 'lucide-react';
import { Document, Page, pdfjs } from 'react-pdf';

import VaultAPI from '../../../lib/api';
import { isRemoteFileSource, resolveLocalPathFromViewerSource } from '../../../utils/fileSources';

import 'react-pdf/dist/Page/AnnotationLayer.css';
import 'react-pdf/dist/Page/TextLayer.css';

// Configure PDF.js worker (bundled with app for CSP + offline support)
pdfjs.GlobalWorkerOptions.workerSrc = new URL(
  'pdfjs-dist/build/pdf.worker.min.mjs',
  import.meta.url
).toString();

interface PDFViewerProps {
  filePath: string;
  title?: string;
}

export function PDFViewer({ filePath, title: _title }: PDFViewerProps) {
  const [numPages, setNumPages] = useState<number>(0);
  const [pageNumber, setPageNumber] = useState(1);
  const [scale, setScale] = useState(1.0);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [pdfBytes, setPdfBytes] = useState<Uint8Array | null>(null);
  const isRemoteSource = useMemo(() => isRemoteFileSource(filePath), [filePath]);

  useEffect(() => {
    let isMounted = true;

    setIsLoading(true);
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
        setError(loadError instanceof Error ? loadError.message : 'Failed to load PDF bytes');
        setIsLoading(false);
      }
    });

    return () => {
      isMounted = false;
    };
  }, [filePath, isRemoteSource]);

  const onDocumentLoadSuccess = ({ numPages }: { numPages: number }) => {
    setNumPages(numPages);
    setIsLoading(false);
    setPageNumber((current) => (current > numPages ? 1 : current));
  };

  const onDocumentLoadError = (error: Error) => {
    console.error('PDF load error:', error);
    setError(error.message);
    setIsLoading(false);
  };

  const goToPrevPage = () => setPageNumber((p) => Math.max(1, p - 1));
  const goToNextPage = () => setPageNumber((p) => Math.min(numPages, p + 1));
  const zoomIn = () => setScale((s) => Math.min(3.0, s + 0.2));
  const zoomOut = () => setScale((s) => Math.max(0.5, s - 0.2));
  const pdfSource = useMemo(
    () => (isRemoteSource ? filePath : (pdfBytes ? { data: pdfBytes } : null)),
    [filePath, isRemoteSource, pdfBytes]
  );

  if (error) {
    return (
      <div className="flex items-center justify-center h-full p-8 bg-[var(--bg-secondary)]">
        <div className="text-center space-y-2">
          <AlertCircle className="w-12 h-12 text-[var(--error)] mx-auto" />
          <p className="text-[var(--error)] font-medium">Failed to load PDF</p>
          <p className="text-sm text-[var(--text-secondary)]">{error}</p>
        </div>
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col bg-[var(--bg-secondary)]">
      {/* Modern toolbar */}
      <div className="flex items-center justify-between px-6 py-3 bg-[var(--bg-primary)] border-b shadow-sm">
        {/* Page navigation */}
        <div className="flex items-center gap-3">
          <button
            onClick={goToPrevPage}
            disabled={pageNumber <= 1}
            className="p-2 rounded-md hover:bg-[var(--surface-hover)] disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
            aria-label="Previous page"
          >
            <ChevronLeft className="w-5 h-5 text-[var(--text-primary)]" />
          </button>
          <span className="text-sm font-medium text-[var(--text-primary)] min-w-[100px] text-center">
            Page {pageNumber} of {numPages || '...'}
          </span>
          <button
            onClick={goToNextPage}
            disabled={pageNumber >= numPages}
            className="p-2 rounded-md hover:bg-[var(--surface-hover)] disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
            aria-label="Next page"
          >
            <ChevronRight className="w-5 h-5 text-[var(--text-primary)]" />
          </button>
        </div>

        {/* Zoom controls */}
        <div className="flex items-center gap-3">
          <button
            onClick={zoomOut}
            className="p-2 rounded-md hover:bg-[var(--surface-hover)] transition-colors"
            aria-label="Zoom out"
          >
            <ZoomOut className="w-5 h-5 text-[var(--text-primary)]" />
          </button>
          <span className="text-sm font-medium text-[var(--text-primary)] min-w-[60px] text-center">
            {Math.round(scale * 100)}%
          </span>
          <button
            onClick={zoomIn}
            className="p-2 rounded-md hover:bg-[var(--surface-hover)] transition-colors"
            aria-label="Zoom in"
          >
            <ZoomIn className="w-5 h-5 text-[var(--text-primary)]" />
          </button>
        </div>
      </div>

      {/* PDF content area */}
      <div className="flex-1 overflow-auto flex items-start justify-center p-8 relative">
        {isLoading && (
          <div className="absolute inset-0 flex items-center justify-center bg-[var(--bg-secondary)]/70">
            <Loader2 className="w-8 h-8 animate-spin text-[var(--accent-primary)]" />
          </div>
        )}
        {pdfSource ? (
          <div className="shadow-2xl rounded-lg overflow-hidden">
            <Document
              file={pdfSource}
              onLoadSuccess={onDocumentLoadSuccess}
              onLoadError={onDocumentLoadError}
              loading={
                <div className="flex items-center justify-center p-12 bg-[var(--bg-primary)]">
                  <Loader2 className="w-8 h-8 animate-spin text-[var(--accent-primary)]" />
                </div>
              }
            >
              <Page
                pageNumber={pageNumber}
                scale={scale}
                renderTextLayer
                renderAnnotationLayer
              />
            </Document>
          </div>
        ) : (
          <div className="flex items-center justify-center p-12 bg-[var(--bg-primary)] rounded-lg shadow-2xl">
            <Loader2 className="w-8 h-8 animate-spin text-[var(--accent-primary)]" />
          </div>
        )}
      </div>
    </div>
  );
}
