interface QueryRewriteHeaderProps {
  isGenerating: boolean;
  onRegenerate: () => void;
  onClose: () => void;
}

export function QueryRewriteHeader({
  isGenerating,
  onRegenerate,
  onClose,
}: QueryRewriteHeaderProps) {
  return (
    <div className="flex items-center justify-between">
      <div className="flex items-center gap-2">
        <svg
          className="w-5 h-5 text-[hsl(var(--accent))]"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M13 10V3L4 14h7v7l9-11h-7z"
          />
        </svg>
        <h3 className="text-lg font-semibold text-[hsl(var(--text-primary))]">
          Query Suggestions
        </h3>
      </div>

      <div className="flex items-center gap-2">
        {!isGenerating && (
          <button
            onClick={onRegenerate}
            className="px-3 py-1.5 text-sm font-medium text-[hsl(var(--accent))] hover:bg-[hsl(var(--accent-muted))] rounded-lg transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--accent))]"
            aria-label="Regenerate suggestions"
          >
            Regenerate
          </button>
        )}
        <button
          onClick={onClose}
          className="p-1.5 text-[hsl(var(--text-tertiary))] hover:text-[hsl(var(--text-secondary))] hover:bg-[hsl(var(--surface-raised))] rounded-lg transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--text-secondary))]"
          aria-label="Close suggestions panel"
        >
          <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M6 18L18 6M6 6l12 12"
            />
          </svg>
        </button>
      </div>
    </div>
  );
}
