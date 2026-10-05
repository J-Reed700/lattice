import { type FC } from 'react';

import { TiptapViewer } from '@/components/TiptapEditor';

interface MarkdownViewerProps {
  content: string;
}

export const MarkdownViewer: FC<MarkdownViewerProps> = ({ content }) => (
  <div className="max-w-none">
    <TiptapViewer content={content} />
  </div>
);
