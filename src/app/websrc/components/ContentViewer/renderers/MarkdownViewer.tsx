import { TiptapViewer } from '../../TiptapEditor';

interface MarkdownViewerProps {
  content: string;
}

export function MarkdownViewer({ content }: MarkdownViewerProps) {
  return (
    <div className="p-6 overflow-auto h-full">
      <TiptapViewer content={content} className="max-w-none prose-sm" />
    </div>
  );
}
