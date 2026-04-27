import { type FC } from 'react';

import { PDFViewer as EmbeddedPDFViewer } from '../../ContentViewer/renderers/PDFViewer';

interface PDFViewerProps {
  filePath: string;
}

export const PDFViewer: FC<PDFViewerProps> = ({ filePath }) => (
  <EmbeddedPDFViewer filePath={filePath} />
);
