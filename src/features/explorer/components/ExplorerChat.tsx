import { useState } from 'react';

import { ChevronDown, Loader2, MessageSquarePlus, MoreHorizontal } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { IconButton } from '@/components/ui/IconButton';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { ChatPanel } from '@/features/chat/components/ChatPanel';
import { ConversationActionsMenu, ConversationRenameField, type ConversationMenuServices } from '@/features/chat/components/sidebar/ConversationActionsMenu';
import { useConversationRename } from '@/features/chat/components/sidebar/useConversationRename';
import { useConversationSynthesis } from '@/features/chat/components/sidebar/useConversationSynthesis';
import { useConversationExport } from '@/features/chat/hooks/useConversationExport';
import { useGenerateConversationFlashcardDeck } from '@/features/learning/recall/flashcards/useFlashcards';

import { ExplorerIndexNotice } from './ExplorerIndexNotice';
import { ExplorerSelectionChip } from './ExplorerSelectionChip';
import { ExplorerThreadList } from './ExplorerThreadList';


import type { ExplorerThreads } from '../hooks/useExplorerThread';

/**
 * The right column: this folder's threads and the ordinary chat.
 *
 * The thread list is a wide popover off the header rather than a pane: the
 * chat is the narrowest column of a three-way split, and a standing list
 * would take its width for good instead of only while a thread is picked.
 */
export function ExplorerChat({ threads }: { threads: ExplorerThreads }) {
  const synthesis = useConversationSynthesis();
  const exportActions = useConversationExport();
  const studyDeck = useGenerateConversationFlashcardDeck();
  const services: ConversationMenuServices = { synthesis, exportActions, studyDeck };
  /** The header's title and the list's rows rename separately, so neither unmounts the other. */
  const headerRename = useConversationRename();
  const listRename = useConversationRename();
  const [open, setOpen] = useState(false);
  const [actionsOpen, setActionsOpen] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<{ id: string; title: string } | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const active = [...threads.threads, ...threads.archived].find((thread) => thread.id === threads.activeId);
  const hasThreads = threads.threads.length + threads.archived.length > 0;
  const renamingActive = active?.id === headerRename.renamingId ? active : undefined;

  const askToDelete = (thread: { id: string; title: string }) => {
    setOpen(false);
    setDeleteError(null);
    setDeleteTarget({ id: thread.id, title: thread.title });
  };

  const deleteConversation = async () => {
    if (!deleteTarget || deleting) return;
    setDeleting(true);
    setDeleteError(null);
    try {
      if (await threads.remove(deleteTarget.id)) setDeleteTarget(null);
      else setDeleteError('The conversation could not be deleted. Try again.');
    } finally {
      setDeleting(false);
    }
  };

  return (
    <div className="flex h-full min-h-0 min-w-0 flex-col">
      <div className="flex h-11 shrink-0 items-center gap-1 border-b border-border-subtle bg-chrome px-2">
        {renamingActive ? (
          <ConversationRenameField conversation={renamingActive} rename={headerRename} className="px-1" />
        ) : (
          <Popover
            open={open}
            onOpenChange={(next) => {
              setOpen(next);
              if (!next) listRename.cancel();
            }}
          >
            <PopoverTrigger asChild>
              <button
                type="button"
                disabled={!hasThreads}
                className="row-hover inline-flex h-7 min-w-0 flex-1 items-center gap-1.5 rounded-md px-2 text-left text-[13px] text-text-primary disabled:opacity-60"
              >
                <span className="truncate">{active?.title ?? (threads.preparing ? 'Opening thread…' : 'No thread')}</span>
                {threads.threads.length > 1 && <span className="shrink-0 text-[11px] tabular-nums text-text-muted">{threads.threads.length}</span>}
                <ChevronDown className="ml-auto h-3.5 w-3.5 shrink-0 text-text-muted" strokeWidth={1.75} aria-hidden="true" />
              </button>
            </PopoverTrigger>
            <PopoverContent
              align="start"
              className="w-[22rem] max-w-[calc(100vw-2rem)] p-0"
              // Escape inside a title being renamed ends the rename, not the list.
              onEscapeKeyDown={(event) => { if (listRename.renamingId) event.preventDefault(); }}
            >
              <ExplorerThreadList
                threads={threads.threads}
                archived={threads.archived}
                activeId={threads.activeId}
                services={services}
                rename={listRename}
                deletingId={deleting ? deleteTarget?.id ?? null : null}
                onSelect={(id) => {
                  setOpen(false);
                  threads.select(id);
                }}
                onDelete={askToDelete}
              />
            </PopoverContent>
          </Popover>
        )}
        <IconButton label="New thread" onClick={() => void threads.create()} disabled={threads.preparing}>
          <MessageSquarePlus />
        </IconButton>
        {active && !renamingActive ? (
          <ConversationActionsMenu
            conversation={active}
            services={services}
            open={actionsOpen}
            onOpenChange={setActionsOpen}
            onRename={() => headerRename.begin(active.id, active.title)}
            onDelete={() => askToDelete(active)}
            deleting={deleting}
            deleteLabel="Delete conversation…"
            triggerLabel="Conversation actions"
            disabled={deleting}
          />
        ) : !active && (
          <IconButton label="Conversation actions" disabled>
            <MoreHorizontal />
          </IconButton>
        )}
      </div>
      {threads.activeId ? (
        <ChatPanel composerNotice={<ExplorerIndexNotice />} composerChips={<ExplorerSelectionChip />} />
      ) : (
        <div className="flex flex-1 flex-col items-center justify-center gap-3 px-6 text-center">
          {threads.error ? (
            <>
              <p role="alert" className="max-w-xs text-sm text-text-secondary">{threads.error}</p>
              <button type="button" onClick={threads.retry} className="rounded-md border border-border-subtle px-3 py-1.5 text-[13px] text-text-primary hover:bg-[hsl(var(--text-primary)/0.05)]">Try again</button>
            </>
          ) : (
            <p className="text-sm text-text-muted">Opening this folder’s thread…</p>
          )}
        </div>
      )}
      {deleteTarget && (
        <Dialog open onOpenChange={(next) => !next && !deleting && setDeleteTarget(null)}>
          <DialogContent className="sm:max-w-md">
            <DialogHeader>
              <DialogTitle>Delete this conversation?</DialogTitle>
              <DialogDescription>
                &ldquo;{deleteTarget.title}&rdquo; and all of its messages will be permanently deleted. This can&apos;t be undone.
              </DialogDescription>
            </DialogHeader>
            {deleteError && <p role="alert" className="text-sm text-danger-fg">{deleteError}</p>}
            <DialogFooter>
              <Button type="button" variant="outline" disabled={deleting} onClick={() => setDeleteTarget(null)}>Cancel</Button>
              <Button type="button" variant="destructive" disabled={deleting} onClick={() => void deleteConversation()}>
                {deleting ? <><Loader2 className="h-4 w-4 animate-spin" />Deleting…</> : 'Delete conversation'}
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      )}
    </div>
  );
}
