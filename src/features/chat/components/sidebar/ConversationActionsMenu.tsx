import {
  Archive,
  Check,
  ClipboardCopy,
  Combine,
  GraduationCap,
  Loader2,
  MessageSquareShare,
  MoreHorizontal,
  NotebookPen,
  Pencil,
  Pin,
  RotateCcw,
  Star,
  Trash2,
  X,
} from 'lucide-react';

import { IconButton } from '@/components/ui/IconButton';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import type { ConversationRename } from '@/features/chat/components/sidebar/useConversationRename';
import type { useConversationSynthesis } from '@/features/chat/components/sidebar/useConversationSynthesis';
import type { ConversationExportActions } from '@/features/chat/hooks/useConversationExport';
import type { useGenerateConversationFlashcardDeck } from '@/features/learning/recall/flashcards/useFlashcards';
import { cn } from '@/lib/utils';
import { useConversationsStore } from '@/shared/conversations/conversationsStore';
import { toast } from '@/stores/toastStore';
import type { Conversation } from '@/types/conversation';
import { handleAsyncEvent } from '@/utils/promiseHandlers';

const MENU_ITEM_CLASS = 'flex w-full items-center gap-2.5 rounded-sm px-2.5 py-2 text-left text-sm text-text-primary transition-colors duration-fast hover:bg-surface focus-visible:bg-surface focus-visible:outline-hidden disabled:cursor-not-allowed disabled:opacity-50';

type MenuConversation = Pick<Conversation, 'id' | 'title' | 'isSaved' | 'isPinned' | 'isArchived'>;

/**
 * The long-running actions behind the menu. A surface mounts them once and
 * hands them to every row, so a running export, synthesis or deck shows on
 * the row that started it and holds back the others.
 */
export interface ConversationMenuServices {
  synthesis: ReturnType<typeof useConversationSynthesis>;
  exportActions: ConversationExportActions;
  studyDeck: ReturnType<typeof useGenerateConversationFlashcardDeck>;
}

interface ConversationActionsMenuProps {
  conversation: MenuConversation;
  services: ConversationMenuServices;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onRename: () => void;
  onDelete: () => void;
  deleting?: boolean;
  /** "Delete conversation…" where the surface asks before deleting. */
  deleteLabel?: string;
  triggerLabel?: string;
  disabled?: boolean;
}

/** Every per-conversation action, the same in the Chat sidebar and in Explorer. */
export function ConversationActionsMenu({
  conversation,
  services,
  open,
  onOpenChange,
  onRename,
  onDelete,
  deleting = false,
  deleteLabel = 'Delete conversation',
  triggerLabel,
  disabled,
}: ConversationActionsMenuProps) {
  const setConversationSaved = useConversationsStore((state) => state.setConversationSaved);
  const setConversationPinned = useConversationsStore((state) => state.setConversationPinned);
  const setConversationArchived = useConversationsStore((state) => state.setConversationArchived);
  const { synthesis, exportActions, studyDeck } = services;
  const { synthesizeConversationToJournal, synthesizingConversationId, isSynthesizing, continueConversationInNewChat, continuingConversationId } = synthesis;
  const { copyConversationAsMarkdown, saveConversationToJournal, copyingConversationId, savingConversationId } = exportActions;
  const { id, title } = conversation;
  const close = () => onOpenChange(false);

  return (
    <Popover open={open} onOpenChange={onOpenChange}>
      <PopoverTrigger asChild>
        <IconButton label={triggerLabel ?? `Actions for ${title}`} disabled={disabled}>
          <MoreHorizontal />
        </IconButton>
      </PopoverTrigger>
      <PopoverContent aria-label={`Actions for ${title}`} align="end" sideOffset={5} className="w-60 p-1.5" onClick={(e) => e.stopPropagation()}>
        <div>
          {/* The two ways out come first: taking an answer somewhere else is
              what a researcher opens this menu for, and both act on this
              conversation, not on whichever one happens to be open. */}
          <button
            type="button"
            className={MENU_ITEM_CLASS}
            disabled={copyingConversationId === id}
            onClick={() => {
              close();
              void copyConversationAsMarkdown(id, title);
            }}
          >
            {copyingConversationId === id ? <Loader2 className="h-4 w-4 animate-spin" /> : <ClipboardCopy className="h-4 w-4" />}
            Copy as Markdown
          </button>
          <button
            type="button"
            className={MENU_ITEM_CLASS}
            disabled={savingConversationId === id}
            onClick={() => {
              close();
              void saveConversationToJournal(id, title);
            }}
          >
            {savingConversationId === id ? <Loader2 className="h-4 w-4 animate-spin" /> : <NotebookPen className="h-4 w-4" />}
            Save to journal
          </button>

          <div className="my-1 h-px bg-border-subtle" />

          <button
            type="button"
            className={MENU_ITEM_CLASS}
            disabled={studyDeck.isPending}
            onClick={() => {
              close();
              studyDeck.mutate(
                { conversationId: id, title: `${title} · Verified claims`.slice(0, 120) },
                { onError: (error) => toast.error("Couldn't create flashcards", { message: error.message }) },
              );
            }}
          >
            {studyDeck.isPending && studyDeck.variables?.conversationId === id ? <Loader2 className="h-4 w-4 animate-spin" /> : <GraduationCap className="h-4 w-4" />}
            Create flashcards
          </button>
          <button
            type="button"
            className={MENU_ITEM_CLASS}
            disabled={isSynthesizing}
            onClick={() => {
              close();
              void synthesizeConversationToJournal(id, title);
            }}
          >
            {synthesizingConversationId === id ? <Loader2 className="h-4 w-4 animate-spin" /> : <Combine className="h-4 w-4" />}
            Synthesize to journal
          </button>
          <button
            type="button"
            className={MENU_ITEM_CLASS}
            disabled={continuingConversationId !== null}
            title="Summarize this chat and start a new one in the same space from the summary"
            onClick={() => {
              close();
              void continueConversationInNewChat(id, title);
            }}
          >
            {continuingConversationId === id ? <Loader2 className="h-4 w-4 animate-spin" /> : <MessageSquareShare className="h-4 w-4" />}
            Continue in new chat
          </button>

          <div className="my-1 h-px bg-border-subtle" />

          <button type="button" className={MENU_ITEM_CLASS} onClick={() => { close(); onRename(); }}>
            <Pencil className="h-4 w-4" />Rename
          </button>
          <button type="button" className={MENU_ITEM_CLASS} onClick={() => { close(); void setConversationSaved(id, !conversation.isSaved); }}>
            <Star className={`h-4 w-4 ${conversation.isSaved ? 'fill-current text-accent' : ''}`} />{conversation.isSaved ? 'Remove from saved' : 'Save conversation'}
          </button>
          <button type="button" className={MENU_ITEM_CLASS} onClick={() => { close(); void setConversationPinned(id, !conversation.isPinned); }}>
            <Pin className={`h-4 w-4 ${conversation.isPinned ? 'text-accent' : ''}`} />{conversation.isPinned ? 'Unpin' : 'Pin'}
          </button>
          <button type="button" className={MENU_ITEM_CLASS} onClick={() => { close(); void setConversationArchived(id, !conversation.isArchived); }}>
            {conversation.isArchived ? <RotateCcw className="h-4 w-4" /> : <Archive className="h-4 w-4" />}{conversation.isArchived ? 'Restore' : 'Archive'}
          </button>

          <div className="my-1 h-px bg-border-subtle" />

          <button
            type="button"
            disabled={deleting}
            className={`${MENU_ITEM_CLASS} text-danger-fg hover:bg-danger-muted focus-visible:bg-danger-muted`}
            onClick={() => { close(); onDelete(); }}
          >
            {deleting ? <Loader2 className="h-4 w-4 animate-spin" /> : <Trash2 className="h-4 w-4" />}{deleteLabel}
          </button>
        </div>
      </PopoverContent>
    </Popover>
  );
}

/** The title field a row turns into while it is being renamed. */
export function ConversationRenameField({
  conversation,
  rename,
  className,
}: {
  conversation: Pick<Conversation, 'id' | 'title'>;
  rename: ConversationRename;
  className?: string;
}) {
  return (
    <div
      className={cn('flex min-w-0 flex-1 items-center gap-1', className)}
      onClick={(e) => e.stopPropagation()}
    >
      <input
        autoFocus
        value={rename.draft}
        maxLength={120}
        onChange={(e) => rename.setDraft(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter') {
            e.preventDefault();
            e.stopPropagation();
            void rename.save(conversation.id, conversation.title);
            return;
          }
          if (e.key === 'Escape') {
            e.preventDefault();
            e.stopPropagation();
            rename.cancel();
          }
        }}
        className="h-6 min-w-0 flex-1 rounded-sm border border-border-default bg-bg px-2 text-sm text-text-primary outline-hidden transition-colors duration-fast focus:border-accent"
        aria-label={`Rename conversation: ${conversation.title}`}
      />
      <IconButton
        label="Save conversation title"
        onClick={handleAsyncEvent(async (e) => {
          e.stopPropagation();
          await rename.save(conversation.id, conversation.title);
        })}
      >
        <Check />
      </IconButton>
      <IconButton
        label="Cancel rename"
        onClick={(e) => {
          e.stopPropagation();
          rename.cancel();
        }}
      >
        <X />
      </IconButton>
    </div>
  );
}
