import { useState } from 'react';

import { Bookmark, Check, Copy, Cpu, GitBranch, Pencil, RefreshCw, Trash2 } from 'lucide-react';

import { ModelPickerPopover } from './ModelPickerPopover';

/**
 * Actions available on a message.
 *
 * Which verbs appear depends on whose turn it is and whether it is the last
 * one — regenerating an answer in the middle of a thread would silently
 * discard everything after it, so it is simply not offered there.
 */

interface MessageActionsProps {
  role: string;
  /** True for the last turn of the conversation. */
  isLastTurn: boolean;
  canBookmark: boolean;
  canDelete: boolean;
  canBranch: boolean;
  isBookmarked: boolean;
  /** A generation is in flight; every mutating verb waits. */
  isBusy: boolean;
  activeModelId?: string | null;
  onCopy: () => Promise<void> | void;
  onBookmarkToggle: () => Promise<void> | void;
  onDelete: () => Promise<void> | void;
  onRegenerate?: () => Promise<void> | void;
  onTryWithModel?: (_modelId: string, _modelLabel: string) => Promise<void> | void;
  onEdit?: () => void;
  onBranch?: () => Promise<void> | void;
}

// CHAT-POLISH-COMPONENTS §5: low contrast and full opacity always. Hiding the
// verbs until hover teaches nobody they exist and strands anyone on a keyboard.
const FOCUS_CLASS =
  'rounded-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--ring))] focus-visible:ring-offset-2 focus-visible:ring-offset-[hsl(var(--bg))]';

const ACTION_CLASS =
  `inline-flex items-center gap-1.5 text-[hsl(var(--text-muted))] transition-colors duration-fast hover:text-[hsl(var(--text-secondary))] disabled:opacity-50 disabled:cursor-not-allowed ${FOCUS_CLASS}`;

export function MessageActions({
  role,
  isLastTurn,
  canBookmark,
  canDelete,
  canBranch,
  isBookmarked,
  isBusy,
  activeModelId,
  onCopy,
  onBookmarkToggle,
  onDelete,
  onRegenerate,
  onTryWithModel,
  onEdit,
  onBranch,
}: MessageActionsProps) {
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    await onCopy();
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const isUser = role === 'user';
  const showRegenerate = !isUser && isLastTurn && Boolean(onRegenerate);
  const showTryWith = !isUser && isLastTurn && Boolean(onTryWithModel);
  const showEdit = isUser && Boolean(onEdit);
  const showBranch = canBranch && Boolean(onBranch);

  return (
    <div className="mt-3 flex items-center gap-4 text-xs">
      <button
        type="button"
        onClick={() => void handleCopy()}
        aria-label="Copy message to clipboard"
        title="Copy"
        className={ACTION_CLASS}
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

      {showEdit && (
        <button
          type="button"
          onClick={onEdit}
          disabled={isBusy}
          aria-label="Edit this message"
          title="Edit"
          className={ACTION_CLASS}
        >
          <Pencil className="h-3 w-3" />
          Edit
        </button>
      )}

      {canBookmark && !isUser && (
        <button
          type="button"
          onClick={() => void onBookmarkToggle()}
          aria-label={isBookmarked ? 'Remove from references' : 'Add to references'}
          title={isBookmarked ? 'Remove from references' : 'Add to references'}
          className={`inline-flex items-center gap-1.5 transition-colors duration-fast ${FOCUS_CLASS} ${
            isBookmarked
              ? 'text-[hsl(var(--accent))] hover:text-[hsl(var(--accent-hover))]'
              : 'text-[hsl(var(--text-muted))] hover:text-[hsl(var(--text-secondary))]'
          }`}
        >
          <Bookmark className={`h-3 w-3 ${isBookmarked ? 'fill-current' : ''}`} />
          {isBookmarked ? 'Referenced' : 'Reference'}
        </button>
      )}

      {showRegenerate && (
        <button
          type="button"
          onClick={() => void onRegenerate?.()}
          disabled={isBusy}
          aria-label="Regenerate this answer"
          title="Regenerate"
          className={ACTION_CLASS}
        >
          <RefreshCw className="h-3 w-3" />
          Regenerate
        </button>
      )}

      {showTryWith && (
        <ModelPickerPopover
          activeModelId={activeModelId}
          onSelect={(modelId, modelLabel) => onTryWithModel?.(modelId, modelLabel)}
        >
          <button
            type="button"
            disabled={isBusy}
            aria-label="Try this question with another model"
            className={ACTION_CLASS}
          >
            <Cpu className="h-3 w-3" />
            Try with another model
          </button>
        </ModelPickerPopover>
      )}

      {showBranch && (
        <button
          type="button"
          onClick={() => void onBranch?.()}
          disabled={isBusy}
          aria-label="Branch the conversation here"
          title="Branch here"
          className={ACTION_CLASS}
        >
          <GitBranch className="h-3 w-3" />
          Branch here
        </button>
      )}

      {canDelete && (
        <button
          type="button"
          onClick={() => void onDelete()}
          disabled={isBusy}
          aria-label="Delete message"
          title="Delete message"
          className={`inline-flex items-center gap-1.5 text-[hsl(var(--text-muted))] transition-colors duration-fast hover:text-[hsl(var(--danger))] disabled:opacity-50 disabled:cursor-not-allowed ${FOCUS_CLASS}`}
        >
          <Trash2 className="h-3 w-3" />
          Delete
        </button>
      )}
    </div>
  );
}
