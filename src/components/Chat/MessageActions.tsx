import { useState } from 'react';

import { ArrowUpRight, Bookmark, Check, Copy, Cpu, GitBranch, Pencil, RefreshCw, Trash2 } from 'lucide-react';

import type { MessageVerificationSummary, SourceWithMetadata } from '@/types/conversation';

import { AnswerActionsMenu } from './actions/AnswerActionsMenu';
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
  /**
   * The answer's cited passages. Present only on an assistant turn that has
   * any; without them there is nothing to put in a journal or compare, so the
   * "Use this answer" menu is not drawn at all.
   */
  answerSources?: readonly SourceWithMetadata[];
  /** The answer as markdown — what "Add to journal" writes. */
  answerMarkdown?: string;
  /** What the sentence check found, carried onto the journal page. */
  answerVerification?: MessageVerificationSummary | null;
  /** The thread this answer came from, named on the journal page. */
  conversationTitle?: string | null;
  /** `metadata.turn`, read defensively by the export; may be absent. */
  answerTurn?: unknown;
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
  `inline-flex h-7 items-center gap-1.5 rounded-md px-2 text-[hsl(var(--text-muted))] transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.06)] hover:text-[hsl(var(--text-primary))] disabled:opacity-50 disabled:cursor-not-allowed ${FOCUS_CLASS}`;

export function MessageActions({
  role,
  isLastTurn,
  canBookmark,
  canDelete,
  canBranch,
  isBookmarked,
  isBusy,
  activeModelId,
  answerSources,
  answerMarkdown,
  answerVerification = null,
  conversationTitle = null,
  answerTurn,
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
  // An answer with nothing behind it has nowhere to go: no sources means no
  // journal entry worth keeping and nothing to compare.
  const showAnswerActions =
    !isUser && Boolean(answerSources?.length) && typeof answerMarkdown === 'string';

  // Always visible, always quiet: verbs that only exist on hover teach nobody they exist.
  return (
    <div className={`-mx-2 mt-2 flex flex-wrap items-center gap-0.5 text-xs ${isUser ? 'justify-end' : ''}`}>
      <button
        type="button"
        onClick={() => void handleCopy()}
        aria-label="Copy message to clipboard"
        title="Copy"
        className={ACTION_CLASS}
      >
        {copied ? (
          <>
            <Check className="h-3.5 w-3.5" />
            Copied
          </>
        ) : (
          <>
            <Copy className="h-3.5 w-3.5" />
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
          <Pencil className="h-3.5 w-3.5" />
          Edit
        </button>
      )}

      {canBookmark && !isUser && (
        <button
          type="button"
          onClick={() => void onBookmarkToggle()}
          aria-label={isBookmarked ? 'Remove from references' : 'Add to references'}
          title={isBookmarked ? 'Remove from references' : 'Add to references'}
          className={`inline-flex h-7 items-center gap-1.5 rounded-md px-2 transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.06)] ${FOCUS_CLASS} ${
            isBookmarked
              ? 'text-[hsl(var(--accent))] hover:text-[hsl(var(--accent-hover))]'
              : 'text-[hsl(var(--text-muted))] hover:text-[hsl(var(--text-secondary))]'
          }`}
        >
          <Bookmark className={`h-3.5 w-3.5 ${isBookmarked ? 'fill-current' : ''}`} />
          {isBookmarked ? 'Referenced' : 'Reference'}
        </button>
      )}

      {/* Always here, never behind a hover: the verbs that carry an answer into
          the rest of the work are the point of a research tool, and a menu
          nobody can see teaches nobody it exists. */}
      {showAnswerActions && (
        <AnswerActionsMenu
          sources={answerSources ?? []}
          markdown={answerMarkdown ?? ''}
          verification={answerVerification}
          conversationTitle={conversationTitle}
          turn={answerTurn}
        >
          <button
            type="button"
            aria-label="Use this answer elsewhere"
            title="Add to journal, or compare its sources"
            className={ACTION_CLASS}
          >
            <ArrowUpRight className="h-3.5 w-3.5" />
            Use this answer
          </button>
        </AnswerActionsMenu>
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
          <RefreshCw className="h-3.5 w-3.5" />
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
            <Cpu className="h-3.5 w-3.5" />
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
          <GitBranch className="h-3.5 w-3.5" />
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
          className={`inline-flex h-7 items-center gap-1.5 rounded-md px-2 text-[hsl(var(--text-muted))] transition-colors duration-fast hover:bg-[hsl(var(--danger)/0.1)] hover:text-[hsl(var(--danger-fg))] disabled:opacity-50 disabled:cursor-not-allowed ${FOCUS_CLASS}`}
        >
          <Trash2 className="h-3.5 w-3.5" />
          Delete
        </button>
      )}
    </div>
  );
}
