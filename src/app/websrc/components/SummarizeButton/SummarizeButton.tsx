/**
 * SummarizeButton Component
 *
 * Quick action button to generate document summary.
 * Shows in document viewer toolbar.
 */

import { useState } from 'react';

import { Sparkles, Loader2 } from 'lucide-react';

interface DocumentSummary {
  summary: string;
  type: string;
  word_count: number;
  timestamp: string;
}

interface SummarizeButtonProps {
  fileId: string;
  onSummaryGenerated?: (summary: DocumentSummary) => void;
  onError?: (error: string) => void;
  className?: string;
  variant?: 'default' | 'compact';
}

export function SummarizeButton({
  fileId,
  onSummaryGenerated,
  onError,
  className = '',
  variant = 'default'
}: SummarizeButtonProps) {
  const [loading, setLoading] = useState(false);

  const handleSummarize = async () => {
    try {
      setLoading(true);

      // Generate TL;DR by default for quick summary
      const response = await fetch(
        `/api/v1/summarize/document/${fileId}?summary_type=tldr&max_words=50`,
        { method: 'POST' }
      );

      if (!response.ok) {
        throw new Error("Couldn't generate summary");
      }

      const summary = await response.json();
      onSummaryGenerated?.(summary);
    } catch (error) {
      console.error('Error generating summary:', error);
      const message = error instanceof Error ? error.message : String(error);
      onError?.(message);
    } finally {
      setLoading(false);
    }
  };

  if (variant === 'compact') {
    return (
      <button
        onClick={handleSummarize}
        disabled={loading}
        className={`
          p-2 rounded-md transition-colors duration-fast
          ${loading ? 'bg-[hsl(var(--accent-muted))]' : 'bg-[hsl(var(--surface-raised))] hover:bg-[hsl(var(--surface-raised))]'}
          border border-[hsl(var(--border-subtle))]
          disabled:opacity-50
          ${className}
        `}
        title="Generate summary"
      >
        {loading ? (
          <Loader2 className="w-4 h-4 animate-spin text-[hsl(var(--accent))]" />
        ) : (
          <Sparkles className="w-4 h-4 text-[hsl(var(--accent))]" strokeWidth={1.75} />
        )}
      </button>
    );
  }

  return (
    <button
      onClick={handleSummarize}
      disabled={loading}
      className={`
        flex items-center gap-2 px-4 py-2 rounded-md transition-colors duration-fast
        ${loading
          ? 'bg-[hsl(var(--accent))] text-[hsl(var(--accent-fg))]'
          : 'bg-[hsl(var(--surface-raised))] hover:bg-[hsl(var(--surface))] text-[hsl(var(--text-primary))]'
        }
        border border-[hsl(var(--border-subtle))]
        disabled:opacity-50
        ${className}
      `}
    >
      {loading ? (
        <>
          <Loader2 className="w-4 h-4 animate-spin" />
          <span className="text-sm font-medium">Generating...</span>
        </>
      ) : (
        <>
          <Sparkles className="w-4 h-4 text-[hsl(var(--accent))]" strokeWidth={1.75} />
          <span className="text-sm font-medium">Summarize</span>
        </>
      )}
    </button>
  );
}

export default SummarizeButton;
