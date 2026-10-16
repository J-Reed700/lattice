import { type KeyboardEvent, type ReactNode, type RefObject, useLayoutEffect, useRef, useState } from 'react';

import { ArrowUpRight, Bookmark, Check, Copy, Cpu, GitBranch, MoreHorizontal, Pencil, RefreshCw, Trash2 } from 'lucide-react';


import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { AnswerActionsMenu } from '@/features/chat/components/actions/AnswerActionsMenu';
import { ModelPickerPopover } from '@/features/chat/components/ModelPickerPopover';
import { TangentReplyAction, type TangentReplySource } from '@/features/chat/components/tangents/TangentReplyAction';
import type { ChatModelChoice } from '@/lib/bindings';
import type { MessageVerificationSummary, SourceWithMetadata } from '@/types/conversation';

const DEFAULT_CHOICE: ChatModelChoice = { kind: 'default' };


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
  /** The chat's own model choice, which "Try with another model" shows checked. */
  modelChoice?: ChatModelChoice;
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
  tangentSource?: TangentReplySource;
}

// CHAT-POLISH-COMPONENTS §5: low contrast and full opacity always. Hiding the
// verbs until hover teaches nobody they exist and strands anyone on a keyboard.
const FOCUS_CLASS =
  'rounded-sm focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-[hsl(var(--ring))] focus-visible:ring-offset-2 focus-visible:ring-offset-[hsl(var(--bg))]';

const ACTION_CLASS =
  `inline-flex h-7 shrink-0 items-center gap-1.5 whitespace-nowrap rounded-md px-1.5 text-[hsl(var(--text-muted))] transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.06)] hover:text-[hsl(var(--text-primary))] disabled:opacity-50 disabled:cursor-not-allowed ${FOCUS_CLASS}`;

const MENU_ITEM_CLASS =
  'flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-sm text-[hsl(var(--text-primary))] transition-colors duration-fast hover:bg-surface-hover focus-visible:bg-surface-hover focus-visible:outline-hidden disabled:cursor-not-allowed disabled:opacity-50';

type VerbId = 'copy' | 'edit' | 'reference' | 'tangent' | 'answer' | 'regenerate' | 'tryWith' | 'branch' | 'delete';

/**
 * When the row would wrap, verbs fold into a visible "More" menu in this
 * order — and only then. Copy, Tangent and "Use this answer" never fold: they
 * are the verbs that carry an answer into the rest of the work.
 */
const FOLD_ORDER: readonly VerbId[] = ['branch', 'tryWith', 'regenerate', 'delete', 'reference', 'edit'];

/** Width of the "More" button before it has ever been drawn. */
const MORE_WIDTH_ESTIMATE = 64;

/**
 * How many of `foldable` (in fold order) must move into "More" for the row
 * to stay on one line. Widths are cached while each verb is inline, so a
 * folded verb is still costed when the row grows again. Without layout (jsdom,
 * no ResizeObserver) nothing folds.
 */
function useFoldCount(rowRef: RefObject<HTMLDivElement | null>, present: readonly VerbId[], labelKey: string): number {
  const [folded, setFolded] = useState(0);
  const widths = useRef(new Map<string, number>());
  const presentKey = present.join(',');

  useLayoutEffect(() => {
    const row = rowRef.current;
    if (!row || typeof ResizeObserver === 'undefined') return;
    const verbs = presentKey.split(',') as VerbId[];
    const order = FOLD_ORDER.filter((id) => verbs.includes(id));

    const fit = () => {
      const available = row.clientWidth;
      if (!available) return;
      row.querySelectorAll<HTMLElement>('[data-verb]').forEach((el) => {
        const id = el.dataset.verb;
        if (id) widths.current.set(id, el.getBoundingClientRect().width);
      });
      // A verb that folded before it was ever drawn has no width yet: unfold
      // everything for one pass (before paint) so it can be measured.
      if (order.some((id) => !widths.current.has(id))) {
        setFolded(0);
        return;
      }
      const gap = Number.parseFloat(getComputedStyle(row).columnGap) || 0;
      const required = (count: number) => {
        const foldedIds = new Set(order.slice(0, count));
        const inline = verbs.filter((id) => !foldedIds.has(id));
        const items = inline.length + (count > 0 ? 1 : 0);
        const more = count > 0 ? (widths.current.get('more') ?? MORE_WIDTH_ESTIMATE) : 0;
        return inline.reduce((sum, id) => sum + (widths.current.get(id) ?? 0), more) + gap * Math.max(0, items - 1);
      };
      let count = 0;
      while (count < order.length && required(count) > available + 0.5) count += 1;
      setFolded(count);
    };

    fit();
    const observer = new ResizeObserver(fit);
    observer.observe(row);
    return () => observer.disconnect();
  }, [rowRef, presentKey, labelKey]);

  return folded;
}

/** Arrow keys move between the items of an open "More" menu. */
function moveMenuFocus(event: KeyboardEvent<HTMLDivElement>) {
  if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return;
  const items = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled)'));
  if (items.length === 0) return;
  event.preventDefault();
  const current = items.indexOf(document.activeElement as HTMLButtonElement);
  const next =
    event.key === 'Home' ? 0
    : event.key === 'End' ? items.length - 1
    : event.key === 'ArrowDown' ? (current + 1) % items.length
    : (current - 1 + items.length) % items.length;
  items[next]?.focus();
}

export function MessageActions({
  role,
  isLastTurn,
  canBookmark,
  canDelete,
  canBranch,
  isBookmarked,
  isBusy,
  modelChoice = DEFAULT_CHOICE,
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
  tangentSource,
}: MessageActionsProps) {
  const [copied, setCopied] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const [pickerOpen, setPickerOpen] = useState(false);
  const rowRef = useRef<HTMLDivElement>(null);
  const moreRef = useRef<HTMLButtonElement>(null);
  // Set while the menu hands focus to the model picker, so closing the menu
  // does not pull focus back to "More" (which would dismiss the picker).
  const handingOffRef = useRef(false);

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
  const showReference = canBookmark && !isUser;
  const showTangent = !isUser && Boolean(tangentSource);
  // An answer with nothing behind it has nowhere to go: no sources means no
  // journal entry worth keeping and nothing to compare.
  const showAnswerActions =
    !isUser && Boolean(answerSources?.length) && typeof answerMarkdown === 'string';

  const present: VerbId[] = ['copy'];
  if (showEdit) present.push('edit');
  if (showReference) present.push('reference');
  if (showTangent) present.push('tangent');
  if (showAnswerActions) present.push('answer');
  if (showRegenerate) present.push('regenerate');
  if (showTryWith) present.push('tryWith');
  if (showBranch) present.push('branch');
  if (canDelete) present.push('delete');

  const foldCount = useFoldCount(rowRef, present, `${copied}|${isBookmarked}`);
  const folded = new Set(FOLD_ORDER.filter((id) => present.includes(id)).slice(0, foldCount));
  const inline = (id: VerbId) => present.includes(id) && !folded.has(id);

  /** Close the menu, then run the verb (after the menu has handed focus back). */
  const fromMenu = (action: () => void) => () => {
    setMenuOpen(false);
    action();
  };

  const menuItem = (id: VerbId, label: string, icon: ReactNode, action: () => void, options: { disabled?: boolean; danger?: boolean } = {}) =>
    folded.has(id) ? (
      <button
        key={id}
        type="button"
        role="menuitem"
        disabled={options.disabled}
        onClick={fromMenu(action)}
        className={`${MENU_ITEM_CLASS} ${options.danger ? 'text-[hsl(var(--danger-fg))]' : ''}`}
      >
        {icon}
        {label}
      </button>
    ) : null;

  const menuIconClass = 'h-3.5 w-3.5 shrink-0 text-[hsl(var(--text-muted))]';

  const moreMenu = folded.size > 0 && (
    <span data-verb="more" className="inline-flex shrink-0">
      <Popover open={menuOpen} onOpenChange={setMenuOpen}>
        <PopoverTrigger asChild>
          <button
            ref={moreRef}
            type="button"
            aria-label="More actions for this message"
            aria-haspopup="menu"
            title="More actions"
            className={ACTION_CLASS}
          >
            <MoreHorizontal className="h-3.5 w-3.5" />
            More
          </button>
        </PopoverTrigger>
        <PopoverContent
          align={isUser ? 'end' : 'start'}
          className="w-56 p-1"
          onCloseAutoFocus={(event) => {
            if (!handingOffRef.current) return;
            handingOffRef.current = false;
            event.preventDefault();
          }}
        >
          <div role="menu" aria-label="More actions" className="flex flex-col" onKeyDown={moveMenuFocus}>
            {menuItem('edit', 'Edit', <Pencil className={menuIconClass} />, () => onEdit?.(), { disabled: isBusy })}
            {menuItem(
              'reference',
              isBookmarked ? 'Remove from references' : 'Add to references',
              <Bookmark className={menuIconClass} />,
              () => void onBookmarkToggle(),
            )}
            {menuItem('regenerate', 'Regenerate', <RefreshCw className={menuIconClass} />, () => void onRegenerate?.(), { disabled: isBusy })}
            {menuItem('tryWith', 'Try with another model…', <Cpu className={menuIconClass} />, () => {
              handingOffRef.current = true;
              setPickerOpen(true);
            }, { disabled: isBusy })}
            {menuItem('branch', 'Branch here', <GitBranch className={menuIconClass} />, () => void onBranch?.(), { disabled: isBusy })}
            {folded.has('delete') && folded.size > 1 && (
              <div role="separator" className="my-1 h-px bg-[hsl(var(--border-subtle))]" />
            )}
            {menuItem('delete', 'Delete message', <Trash2 className="h-3.5 w-3.5 shrink-0" />, () => void onDelete(), { disabled: isBusy, danger: true })}
          </div>
        </PopoverContent>
      </Popover>
    </span>
  );

  // Always visible, always quiet: verbs that only exist on hover teach nobody they exist.
  return (
    <div
      ref={rowRef}
      className={`-mx-1.5 mt-2 flex flex-wrap items-center gap-0.5 text-xs ${isUser ? 'justify-end' : ''}`}
    >
      <span data-verb="copy" className="inline-flex shrink-0">
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
      </span>

      {inline('edit') && (
        <span data-verb="edit" className="inline-flex shrink-0">
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
        </span>
      )}

      {inline('reference') && (
        <span data-verb="reference" className="inline-flex shrink-0">
          <button
            type="button"
            onClick={() => void onBookmarkToggle()}
            aria-label={isBookmarked ? 'Remove from references' : 'Add to references'}
            title={isBookmarked ? 'Remove from references' : 'Add to references'}
            className={`inline-flex h-7 shrink-0 items-center gap-1.5 whitespace-nowrap rounded-md px-1.5 transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.06)] ${FOCUS_CLASS} ${
              isBookmarked
                ? 'text-[hsl(var(--accent))] hover:text-[hsl(var(--accent-hover))]'
                : 'text-[hsl(var(--text-muted))] hover:text-[hsl(var(--text-secondary))]'
            }`}
          >
            <Bookmark className={`h-3.5 w-3.5 ${isBookmarked ? 'fill-current' : ''}`} />
            {isBookmarked ? 'Referenced' : 'Reference'}
          </button>
        </span>
      )}

      {showTangent && tangentSource && (
        <span data-verb="tangent" className="inline-flex shrink-0">
          <TangentReplyAction source={tangentSource} className={ACTION_CLASS} disabled={isBusy} />
        </span>
      )}

      {/* Always here, never behind a hover: the verbs that carry an answer into
          the rest of the work are the point of a research tool, and a menu
          nobody can see teaches nobody it exists. */}
      {showAnswerActions && (
        <span data-verb="answer" className="inline-flex shrink-0">
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
        </span>
      )}

      {inline('regenerate') && (
        <span data-verb="regenerate" className="inline-flex shrink-0">
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
        </span>
      )}

      {inline('tryWith') && (
        <span data-verb="tryWith" className="inline-flex shrink-0">
          <ModelPickerPopover
            choice={modelChoice}
            label="Answer again with"
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
        </span>
      )}

      {inline('branch') && (
        <span data-verb="branch" className="inline-flex shrink-0">
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
        </span>
      )}

      {/* Folded verbs stay one click away. "Try with another model" opens its
          picker from here, anchored to the More button it was chosen from. */}
      {folded.has('tryWith') ? (
        <ModelPickerPopover
          anchorOnly
          open={pickerOpen}
          onOpenChange={(open) => {
            setPickerOpen(open);
            // The picker has no trigger of its own; give focus back to "More".
            if (!open) requestAnimationFrame(() => moreRef.current?.focus());
          }}
          align="start"
          choice={modelChoice}
          label="Answer again with"
          onSelect={(modelId, modelLabel) => onTryWithModel?.(modelId, modelLabel)}
        >
          {moreMenu}
        </ModelPickerPopover>
      ) : (
        moreMenu
      )}

      {/* Destructive, so set apart from the verbs that keep the answer. */}
      {inline('delete') && (
        <span data-verb="delete" className="inline-flex shrink-0 items-center gap-1 pl-1">
          <span aria-hidden="true" className="h-4 w-px bg-[hsl(var(--border-default))]" />
          <button
            type="button"
            onClick={() => void onDelete()}
            disabled={isBusy}
            aria-label="Delete message"
            title="Delete message"
            className={`inline-flex h-7 shrink-0 items-center gap-1.5 whitespace-nowrap rounded-md px-1.5 text-[hsl(var(--text-muted))] transition-colors duration-fast hover:bg-[hsl(var(--danger)/0.1)] hover:text-[hsl(var(--danger-fg))] disabled:opacity-50 disabled:cursor-not-allowed ${FOCUS_CLASS}`}
          >
            <Trash2 className="h-3.5 w-3.5" />
            Delete
          </button>
        </span>
      )}
    </div>
  );
}
