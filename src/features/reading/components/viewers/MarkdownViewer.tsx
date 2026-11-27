import { type FC } from 'react';

import { TiptapViewer } from '@/components/TiptapEditor';

interface MarkdownViewerProps {
  content: string;
  sourceLines?: boolean;
}

export const MarkdownViewer: FC<MarkdownViewerProps> = ({ content, sourceLines }) => (
  <div className="max-w-none">
    <TiptapViewer content={content} sourceLines={sourceLines} />
  </div>
);
