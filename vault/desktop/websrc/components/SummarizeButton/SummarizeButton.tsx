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
        throw new Error('Failed to generate summary');
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
          p-2 rounded-lg transition-colors
          ${loading ? 'bg-[var(--accent-light)]' : 'bg-[var(--surface-elevated)] hover:bg-[var(--bg-tertiary)]'}
          border border-[var(--border-color)]
          disabled:opacity-50
          ${className}
        `}
        title="Generate summary"
      >
        {loading ? (
          <Loader2 className="w-5 h-5 animate-spin text-[var(--accent-primary)]" />
        ) : (
          <Sparkles className="w-5 h-5 text-[var(--accent-primary)]" />
        )}
      </button>
    );
  }

  return (
    <button
      onClick={handleSummarize}
      disabled={loading}
      className={`
        flex items-center gap-2 px-4 py-2 rounded-lg transition-colors
        ${loading
          ? 'bg-[var(--accent-primary)] text-white'
          : 'bg-[var(--surface-elevated)] hover:bg-[var(--bg-secondary)] text-[var(--text-primary)]'
        }
        border border-[var(--border-color)]
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
          <Sparkles className="w-4 h-4 text-[var(--accent-primary)]" />
          <span className="text-sm font-medium">Summarize</span>
        </>
      )}
    </button>
  );
}

export default SummarizeButton;
