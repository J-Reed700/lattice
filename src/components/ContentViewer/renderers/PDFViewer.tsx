import { lazy, Suspense } from 'react';

import type { PDFViewerProps } from './PDFViewerImpl';

const PDFViewerImpl = lazy(() => import('./PDFViewerImpl').then((module) => ({ default: module.PDFViewer })));

export function PDFViewer(props: PDFViewerProps) {
  return (
    <Suspense fallback={<div className="flex h-full items-center justify-center text-sm text-text-muted">Loading PDF…</div>}>
      <PDFViewerImpl {...props} />
    </Suspense>
  );
}
