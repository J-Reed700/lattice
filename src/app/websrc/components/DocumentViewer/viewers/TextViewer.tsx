/**
 * TextViewer
 *
 * Purpose: Display plain text files with proper formatting
 *
 * Features:
 * - Monospace font for readability
 * - Preserved whitespace and line breaks
 * - Scrollable content
 * - Text selection support
 *
 * States: default
 * Accessibility: WCAG AA, semantic HTML, screen reader friendly
 */

export interface TextViewerProps {
  content: string;
}

export function TextViewer({ content }: TextViewerProps) {
  if (!content) {
    return (
      <div className="flex items-center justify-center h-full">
        <div className="text-center max-w-md p-8">
          <div className="w-16 h-16 bg-[hsl(var(--surface-raised))] rounded-full flex items-center justify-center mx-auto mb-4">
            <svg
              className="w-8 h-8 text-[hsl(var(--text-tertiary))]"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
              />
            </svg>
          </div>
          <h3 className="text-lg font-semibold text-[hsl(var(--text-primary))] mb-2">
            Empty File
          </h3>
          <p className="text-[hsl(var(--text-secondary))]">This file has no content to display.</p>
        </div>
      </div>
    );
  }

  return (
    <div className="h-full overflow-auto bg-[hsl(var(--surface-raised))]">
      <pre className="p-8 whitespace-pre-wrap font-mono text-sm text-[hsl(var(--text-primary))] leading-relaxed">
        {content}
      </pre>
    </div>
  );
}

export default TextViewer;
