import { useState } from 'react';

import { Bookmark, Check, Copy, Trash2 } from 'lucide-react';

interface MessageActionsProps {
  canBookmark: boolean;
  canDelete: boolean;
  isBookmarked: boolean;
  onCopy: () => Promise<void> | void;
  onBookmarkToggle: () => Promise<void> | void;
  onDelete: () => Promise<void> | void;
}

export function MessageActions({
  canBookmark,
  canDelete,
  isBookmarked,
  onCopy,
  onBookmarkToggle,
  onDelete,
}: MessageActionsProps) {
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    await onCopy();
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="mt-3 flex items-center gap-4 text-xs">
      <button
        type="button"
        onClick={() => void handleCopy()}
        aria-label="Copy message to clipboard"
        title="Copy"
        className="inline-flex items-center gap-1.5 text-[hsl(var(--text-muted))] transition-colors duration-fast hover:text-[hsl(var(--text-secondary))]"
      >
        {copied ? (
          <>
            <Check className="h-3 w-3" />
            Copied
          </>
        ) : (
          <>
            <Copy className="h-3 w-3" />
            Copy
          </>
        )}
      </button>
      {canBookmark && (
        <button
          type="button"
          onClick={() => void onBookmarkToggle()}
          aria-label={isBookmarked ? 'Remove from references' : 'Add to references'}
          title={isBookmarked ? 'Remove from references' : 'Add to references'}
          className={`inline-flex items-center gap-1.5 transition-colors duration-fast ${
            isBookmarked
              ? 'text-[hsl(var(--accent))] hover:text-[hsl(var(--accent-hover))]'
              : 'text-[hsl(var(--text-muted))] hover:text-[hsl(var(--text-secondary))]'
          }`}
        >
          <Bookmark className={`h-3 w-3 ${isBookmarked ? 'fill-current' : ''}`} />
          {isBookmarked ? 'Referenced' : 'Reference'}
        </button>
      )}
      {canDelete && (
        <button
          type="button"
          onClick={() => void onDelete()}
          aria-label="Delete message"
          title="Delete message"
          className="inline-flex items-center gap-1.5 text-[hsl(var(--text-muted))] transition-colors duration-fast hover:text-[hsl(var(--danger))]"
        >
          <Trash2 className="h-3 w-3" />
          Delete
        </button>
      )}
    </div>
  );
}
