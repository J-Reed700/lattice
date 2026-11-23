import { type KeyboardEvent, type ReactNode, type RefObject, useContext, useLayoutEffect, useRef, useState } from 'react';

import { ArrowUpRight, Bookmark, Check, Copy, Cpu, GitBranch, MoreHorizontal, Pencil, RefreshCw, Trash2 } from 'lucide-react';

import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { AnswerActionsMenu } from '@/features/chat/components/actions/AnswerActionsMenu';
import { ModelPickerPopover } from '@/features/chat/components/ModelPickerPopover';
import { TangentReplyAction, type TangentReplySource } from '@/features/chat/components/tangents/TangentReplyAction';
import { TangentSelectionContext } from '@/features/chat/components/tangents/TangentSelection';
import { ListenButton } from '@/features/speech/components/ListenButton';
import type { ChatModelChoice } from '@/lib/bindings';
import type { MessageVerificationSummary, SourceWithMetadata } from '@/types/conversation';

const DEFAULT_CHOICE: ChatModelChoice = { kind: 'default' };

/**
 * Actions available on a message.
 *
 * Assistant turns surface up to 4 crisp primary actions inline (Copy,
 * Reference, Regenerate, Listen) and group secondary actions (Tangent,
 * Use this answer, Try with another model, Branch here, Delete) inside a
 * clean More menu — keeping every action 1-2 clicks away without clutter.
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
  /** A finished answer to read aloud, as markdown. */
  listen?: { sourceId: string; markdown: string };
}

// Low contrast and full opacity always so actions remain keyboard-discoverable.
const FOCUS_CLASS =
  'rounded-md focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-bg';

const ACTION_CLASS =
  `inline-flex h-7 shrink-0 items-center gap-1.5 whitespace-nowrap rounded-lg px-2 text-text-muted transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.06)] hover:text-text-primary disabled:opacity-50 disabled:cursor-not-allowed ${FOCUS_CLASS}`;

const MENU_ITEM_CLASS =
  'flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-ui text-text-primary transition-colors duration-fast hover:bg-surface focus-visible:bg-surface focus-visible:outline-hidden disabled:cursor-not-allowed disabled:opacity-50';

type PrimaryVerbId = 'copy' | 'reference' | 'regenerate' | 'listen' | 'edit';

/** Primary verbs that can fold into "More" on very narrow columns (e.g. tangent panes). */
const FOLD_ORDER: readonly PrimaryVerbId[] = ['regenerate', 'reference', 'edit'];

const MORE_WIDTH_ESTIMATE = 64;

function useFoldCount(
  rowRef: RefObject<HTMLDivElement | null>,
  present: readonly PrimaryVerbId[],
  hasSecondary: boolean,
  labelKey: string,
): number {
  const [folded, setFolded] = useState(0);
  const widths = useRef(new Map<string, number>());
  const presentKey = present.join(',');

  useLayoutEffect(() => {
    const row = rowRef.current;
    if (!row || typeof ResizeObserver === 'undefined') return;
    const verbs = presentKey.split(',') as PrimaryVerbId[];
    const order = FOLD_ORDER.filter((id) => verbs.includes(id));

    const fit = () => {
      const available = row.clientWidth;
      if (!available) return;
      row.querySelectorAll<HTMLElement>('[data-verb]').forEach((el) => {
        const id = el.dataset.verb;
        if (id) widths.current.set(id, el.getBoundingClientRect().width);
      });
      if (order.some((id) => !widths.current.has(id))) {
        setFolded(0);
        return;
      }
      const gap = Number.parseFloat(getComputedStyle(row).columnGap) || 0;
      const required = (count: number) => {
        const foldedIds = new Set(order.slice(0, count));
        const inline = verbs.filter((id) => !foldedIds.has(id));
        const showMore = hasSecondary || count > 0;
        const items = inline.length + (showMore ? 1 : 0);
        const more = showMore ? (widths.current.get('more') ?? MORE_WIDTH_ESTIMATE) : 0;
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
  }, [rowRef, presentKey, hasSecondary, labelKey]);

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
  listen,
}: MessageActionsProps) {
  const tangents = useContext(TangentSelectionContext);
  const [copied, setCopied] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [answerMenuOpen, setAnswerMenuOpen] = useState(false);
  const rowRef = useRef<HTMLDivElement>(null);
  const moreRef = useRef<HTMLButtonElement>(null);
  // Set while the menu hands focus to an anchored popover (model picker or
  // answer actions), so closing "More" does not steal focus back.
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
  const showTangent = !isUser && Boolean(tangentSource) && Boolean(tangents);
  // An answer with nothing behind it has nowhere to go: no sources means no
  // journal entry worth keeping and nothing to compare.
  const showAnswerActions =
    !isUser && Boolean(answerSources?.length) && typeof answerMarkdown === 'string';

  const primaryPresent: PrimaryVerbId[] = ['copy'];
  if (showEdit) primaryPresent.push('edit');
  if (showReference) primaryPresent.push('reference');
  if (showRegenerate) primaryPresent.push('regenerate');
  if (listen && !isUser) primaryPresent.push('listen');

  const secondaryCount =
    (showTangent ? 1 : 0) +
    (showAnswerActions ? 1 : 0) +
    (showTryWith ? 1 : 0) +
    (showBranch ? 1 : 0) +
    (canDelete ? 1 : 0);

  const foldCount = useFoldCount(rowRef, primaryPresent, secondaryCount > 0, `${copied}|${isBookmarked}`);
  const folded = new Set(FOLD_ORDER.filter((id) => primaryPresent.includes(id)).slice(0, foldCount));
  const inline = (id: PrimaryVerbId) => primaryPresent.includes(id) && !folded.has(id);

  const hasMoreItems = secondaryCount > 0 || folded.size > 0;
  const nonDeleteMenuItemsCount =
    (folded.has('edit') ? 1 : 0) +
    (folded.has('reference') ? 1 : 0) +
    (folded.has('regenerate') ? 1 : 0) +
    (showTangent ? 1 : 0) +
    (showAnswerActions ? 1 : 0) +
    (showTryWith ? 1 : 0) +
    (showBranch ? 1 : 0);

  const fromMenu = (action: () => void) => () => {
    setMenuOpen(false);
    action();
  };

  const menuIconClass = 'h-3.5 w-3.5 shrink-0 text-text-muted';

  const moreButton = hasMoreItems ? (
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
          className="w-56 rounded-xl p-1"
          onCloseAutoFocus={(event) => {
            if (!handingOffRef.current) return;
            handingOffRef.current = false;
            event.preventDefault();
          }}
        >
          <div role="menu" aria-label="More actions" className="flex flex-col" onKeyDown={moveMenuFocus}>
            {folded.has('edit') && (
              <button
                type="button"
                role="menuitem"
                disabled={isBusy}
                onClick={fromMenu(() => onEdit?.())}
                className={MENU_ITEM_CLASS}
              >
                <Pencil className={menuIconClass} />
                Edit
              </button>
            )}
            {folded.has('reference') && (
              <button
                type="button"
                role="menuitem"
                onClick={fromMenu(() => void onBookmarkToggle())}
                className={MENU_ITEM_CLASS}
              >
                <Bookmark className={menuIconClass} />
                {isBookmarked ? 'Remove from references' : 'Add to references'}
              </button>
            )}
            {folded.has('regenerate') && (
              <button
                type="button"
                role="menuitem"
                disabled={isBusy}
                onClick={fromMenu(() => void onRegenerate?.())}
                className={MENU_ITEM_CLASS}
              >
                <RefreshCw className={menuIconClass} />
                Regenerate
              </button>
            )}
            {showTangent && tangentSource && (
              <TangentReplyAction
                source={tangentSource}
                role="menuitem"
                disabled={isBusy}
                className={MENU_ITEM_CLASS}
                onAfterTrigger={() => setMenuOpen(false)}
              />
            )}
            {showAnswerActions && (
              <button
                type="button"
                role="menuitem"
                aria-label="Use this answer elsewhere"
                onClick={() => {
                  handingOffRef.current = true;
                  setMenuOpen(false);
                  setAnswerMenuOpen(true);
                }}
                className={MENU_ITEM_CLASS}
              >
                <ArrowUpRight className={menuIconClass} />
                Use this answer…
              </button>
            )}
            {showTryWith && (
              <button
                type="button"
                role="menuitem"
                disabled={isBusy}
                onClick={() => {
                  handingOffRef.current = true;
                  setMenuOpen(false);
                  setPickerOpen(true);
                }}
                className={MENU_ITEM_CLASS}
              >
                <Cpu className={menuIconClass} />
                Try with another model…
              </button>
            )}
            {showBranch && (
              <button
                type="button"
                role="menuitem"
                disabled={isBusy}
                onClick={fromMenu(() => void onBranch?.())}
                className={MENU_ITEM_CLASS}
              >
                <GitBranch className={menuIconClass} />
                Branch here
              </button>
            )}
            {canDelete && nonDeleteMenuItemsCount > 0 && (
              <div role="separator" className="my-1 h-px bg-border-subtle" />
            )}
            {canDelete && (
              <button
                type="button"
                role="menuitem"
                disabled={isBusy}
                onClick={fromMenu(() => void onDelete())}
                className={`${MENU_ITEM_CLASS} text-danger-fg hover:bg-danger-muted`}
              >
                <Trash2 className="h-3.5 w-3.5 shrink-0" />
                Delete message
              </button>
            )}
          </div>
        </PopoverContent>
      </Popover>
    </span>
  ) : null;

  let anchoredMore: ReactNode = moreButton;
  if (showAnswerActions && anchoredMore) {
    anchoredMore = (
      <AnswerActionsMenu
        anchorOnly
        open={answerMenuOpen}
        onOpenChange={(open) => {
          setAnswerMenuOpen(open);
          if (!open) requestAnimationFrame(() => moreRef.current?.focus());
        }}
        align="start"
        sources={answerSources ?? []}
        markdown={answerMarkdown ?? ''}
        verification={answerVerification}
        conversationTitle={conversationTitle}
        turn={answerTurn}
      >
        {anchoredMore}
      </AnswerActionsMenu>
    );
  }
  if (showTryWith && anchoredMore) {
    anchoredMore = (
      <ModelPickerPopover
        anchorOnly
        open={pickerOpen}
        onOpenChange={(open) => {
          setPickerOpen(open);
          if (!open) requestAnimationFrame(() => moreRef.current?.focus());
        }}
        align="start"
        choice={modelChoice}
        label="Answer again with"
        onSelect={(modelId, modelLabel) => onTryWithModel?.(modelId, modelLabel)}
      >
        {anchoredMore}
      </ModelPickerPopover>
    );
  }

  return (
    <div
      ref={rowRef}
      className={`-mx-2 mt-2 flex flex-wrap items-center gap-0.5 text-xs ${isUser ? 'justify-end' : ''}`}
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
            title={isBookmarked ? 'Remove from references' : 'Save reference'}
            className={`inline-flex h-7 shrink-0 items-center gap-1.5 whitespace-nowrap rounded-lg px-2 transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.06)] ${FOCUS_CLASS} ${
              isBookmarked
                ? 'text-accent hover:text-accent-hover'
                : 'text-text-muted hover:text-text-primary'
            }`}
          >
            <Bookmark className={`h-3.5 w-3.5 ${isBookmarked ? 'fill-current' : ''}`} />
            {isBookmarked ? 'Referenced' : 'Reference'}
          </button>
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

      {inline('listen') && listen && (
        <span data-verb="listen" className="inline-flex shrink-0">
          <ListenButton sourceId={listen.sourceId} markdown={listen.markdown} subject="this answer" className={ACTION_CLASS} />
        </span>
      )}

      {anchoredMore}
    </div>
  );
}
