import ReactMarkdown from 'react-markdown';
import rehypeRaw from 'rehype-raw';
import remarkGfm from 'remark-gfm';

/**
 * MarkdownViewer
 *
 * Purpose: Render markdown files with GitHub-flavored markdown support
 *
 * Features:
 * - GitHub-flavored markdown (tables, task lists, strikethrough)
 * - Syntax highlighting for code blocks
 * - Typography styles
 * - Dark mode support
 * - Responsive layout
 *
 * States: default
 * Accessibility: WCAG AA, semantic HTML
 */

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
      <div
        className="max-w-4xl mx-auto p-8
        prose prose-sm sm:prose-base lg:prose-lg
        prose-headings:font-bold prose-headings:tracking-tight
        prose-h1:text-3xl prose-h1:mb-4 prose-h1:border-b prose-h1:pb-2
        prose-h2:text-2xl prose-h2:mt-8 prose-h2:mb-4
        prose-h3:text-xl prose-h3:mt-6 prose-h3:mb-3
        prose-p:text-[var(--text-secondary)] prose-p:leading-relaxed
        prose-a:text-[var(--accent-primary)] prose-a:no-underline hover:prose-a:underline
        prose-strong:text-[var(--text-primary)] prose-strong:font-semibold
        prose-code:text-[var(--text-primary)]
        prose-code:bg-[var(--bg-secondary)]
        prose-code:px-1.5 prose-code:py-0.5 prose-code:rounded
        prose-code:before:content-none prose-code:after:content-none
        prose-pre:bg-[var(--bg-tertiary)]
        prose-pre:text-[var(--text-primary)]
        prose-pre:border prose-pre:border-[var(--border-color)]
        prose-blockquote:border-l-4 prose-blockquote:border-[var(--accent-primary)]
        prose-blockquote:bg-[var(--accent-light)]/20
        prose-blockquote:py-2 prose-blockquote:px-4 prose-blockquote:my-4
        prose-ul:list-disc prose-ol:list-decimal
        prose-li:text-[var(--text-secondary)]
        prose-table:border-collapse prose-table:w-full
        prose-thead:bg-[var(--bg-secondary)]
        prose-th:border prose-th:border-[var(--border-color)] prose-th:px-4 prose-th:py-2
        prose-td:border prose-td:border-[var(--border-color)] prose-td:px-4 prose-td:py-2
        prose-img:rounded-lg prose-img:shadow-lg
        prose-hr:border-[var(--border-color)]"
      >
        <ReactMarkdown
          remarkPlugins={[remarkGfm]}
          rehypePlugins={[rehypeRaw]}
          components={{
            // Custom rendering for code blocks with language
            code({ className, children, ...props }) {
              return (
                <code className={className} {...props}>
                  {children}
                </code>
              );
            },
            // Custom rendering for links to open externally
            a({ children, href, ...props }) {
              return (
                <a
                  href={href}
                  target="_blank"
                  rel="noopener noreferrer"
                  {...props}
                >
                  {children}
                </a>
              );
            },
            // Custom rendering for checkboxes in task lists
            input({ ...props }) {
              if (props.type === 'checkbox') {
                return (
                  <input
                    {...props}
                    className="mr-2 rounded border-[var(--border-color)] text-[var(--accent-primary)] ring-[var(--accent-primary)]"
                  />
                );
              }
              return <input {...props} />;
            },
          }}
        >
          {content}
        </ReactMarkdown>
      </div>
    </div>
  );
}

MarkdownViewer.displayName = 'MarkdownViewer';

export default MarkdownViewer;
