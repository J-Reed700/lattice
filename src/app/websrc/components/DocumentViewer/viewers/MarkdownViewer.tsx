import { TiptapViewer } from '../../TiptapEditor';

export interface MarkdownViewerProps {
  content: string;
}

export function MarkdownViewer({ content }: MarkdownViewerProps) {
  if (!content) {
    return (
      <div className="flex items-center justify-center h-full">
        <div className="text-center max-w-md p-8">
          <div className="w-16 h-16 bg-[var(--bg-tertiary)] rounded-full flex items-center justify-center mx-auto mb-4">
            <svg
              className="w-8 h-8 text-[var(--text-tertiary)]"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M7 21h10a2 2 0 002-2V9.414a1 1 0 00-.293-.707l-5.414-5.414A1 1 0 0012.586 3H7a2 2 0 00-2 2v14a2 2 0 002 2z"
              />
            </svg>
          </div>
          <h3 className="text-lg font-semibold text-[var(--text-primary)] mb-2">
            Empty Document
          </h3>
          <p className="text-[var(--text-secondary)]">
            This markdown file has no content to display.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="h-full overflow-auto bg-[var(--surface-elevated)]">
      <div className="max-w-4xl mx-auto p-8">
        <TiptapViewer content={content} />
      </div>
    </div>
  );
}

MarkdownViewer.displayName = 'MarkdownViewer';

export default MarkdownViewer;
