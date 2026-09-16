import { type FC } from 'react';

import { PDFViewer as EmbeddedPDFViewer } from '../../ContentViewer/renderers/PDFViewer';

import type { PassageLocator, PassageMatchTier } from '../../../types/conversation';

interface PDFViewerProps {
  filePath: string;
  /** A cited passage to land on. The page is resolved by the embedded viewer. */
  highlight?: PassageLocator | null;
  onLocationResolved?: (_label: string) => void;
  onMatch?: (_tier: PassageMatchTier) => void;
}

export const PDFViewer: FC<PDFViewerProps> = ({
  filePath,
  highlight,
  onLocationResolved,
  onMatch,
}) => (
  <EmbeddedPDFViewer
    filePath={filePath}
    highlight={highlight}
    onLocationResolved={onLocationResolved}
    onMatch={onMatch}
  />
);
